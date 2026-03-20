pub mod actors;
pub mod config;
pub mod db;
pub mod observability;
pub mod protocol;
pub mod routing;
pub mod server;
pub mod types;

use opentelemetry::trace::TracerProvider;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_stdout::LogExporter;
use opentelemetry_stdout::SpanExporter;
use ractor::Actor;
use tracing::info;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

use crate::actors::gateway_manager::{GatewayManagerActor, GatewayManagerArgs};
use crate::config::models::RuntimeSettings;
use crate::db::DatabaseRepo;
use crate::server::ws::{AppState, start_server};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = SdkLoggerProvider::builder()
        .with_simple_exporter(
            opentelemetry_otlp::LogExporter::builder()
                .with_http()
                .build()
                .unwrap(),
        )
        .build();

    let provider_tracer = SdkTracerProvider::builder()
        .with_simple_exporter(
            opentelemetry_otlp::SpanExporter::builder()
                .with_http()
                .build()
                .unwrap(),
        )
        .build();

    let otel_layer = OpenTelemetryTracingBridge::new(&provider);

    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .with_level(true)
        .finish()
        .with(otel_layer)
        .with(tracing_opentelemetry::layer().with_tracer(provider_tracer.tracer("main")));

    tracing::subscriber::set_global_default(subscriber)?;

    info!("llm-bridge process starting");

    run_server().await
}

#[tracing::instrument]
async fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    let settings = RuntimeSettings::from_env()?;
    info!(
        gateway_id = %settings.gateway_id,
        host = %settings.server.host,
        port = settings.server.port,
        auth_required = settings.server.auth_token.is_some(),
        "runtime settings loaded"
    );

    let database = DatabaseRepo::open(&settings.database.path)?;
    info!(db_path = %settings.database.path, "database opened");

    let (gateway_manager, gateway_handle) = Actor::spawn(
        None,
        GatewayManagerActor,
        GatewayManagerArgs {
            settings: settings.clone(),
            database,
        },
    )
    .await?;

    let state = AppState {
        gateway_manager,
        gateway_id: settings.gateway_id.clone(),
        auth_token: settings.server.auth_token.clone(),
    };

    let server_result = start_server(state, &settings.server.host, settings.server.port).await;

    info!("stopping gateway manager actor");
    gateway_handle.abort();

    server_result?;
    Ok(())
}
