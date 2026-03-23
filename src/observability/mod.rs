use opentelemetry::trace::TracerProvider;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

pub struct ObservabilityGuard {
    logger_provider: SdkLoggerProvider,
    tracer_provider: SdkTracerProvider,
}

pub fn init(service_name: &str) -> Result<ObservabilityGuard, Box<dyn std::error::Error>> {
    let service_name = service_name.to_owned();
    let resource = Resource::builder()
        .with_service_name(service_name.clone())
        .build();

    let logger_provider = SdkLoggerProvider::builder()
        .with_resource(resource.clone())
        .with_batch_exporter(
            opentelemetry_otlp::LogExporter::builder()
                .with_http()
                .build()?,
        )
        .build();

    let tracer_provider = SdkTracerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(
            opentelemetry_otlp::SpanExporter::builder()
                .with_http()
                .build()?,
        )
        .build();

    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .with_level(true)
        .finish()
        .with(
            tracing_opentelemetry::layer()
                .with_tracer(tracer_provider.tracer(service_name.to_owned())),
        )
        .with(
            opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(
                &logger_provider,
            ),
        );

    tracing::subscriber::set_global_default(subscriber)?;

    Ok(ObservabilityGuard {
        logger_provider,
        tracer_provider,
    })
}

impl ObservabilityGuard {
    pub fn shutdown(self) {
        if let Err(error) = self.tracer_provider.shutdown() {
            tracing::warn!(error = %error, "failed to shut down tracer provider");
        }

        if let Err(error) = self.logger_provider.shutdown() {
            tracing::warn!(error = %error, "failed to shut down logger provider");
        }
    }
}
