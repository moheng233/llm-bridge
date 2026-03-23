pub mod actors;
pub mod config;
pub mod db;
pub mod observability;
pub mod protocol;
pub mod routing;
pub mod server;
pub mod types;

use std::sync::Arc;

use ractor::Actor;
use tracing::info;

use crate::actors::gateway_manager::{
    GatewayManagerActor, GatewayManagerArgs, GatewayManagerMessage,
};
use crate::config::models::RuntimeSettings;
use crate::db::DatabaseRepo;
use crate::server::ws::{AppState, start_server};

type MainResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

#[tokio::main]
async fn main() -> MainResult {
    let observability = observability::init("llm-bridge")?;

    info!("llm-bridge process starting");

    let server_result = run_server().await;
    observability.shutdown();
    server_result
}

#[tracing::instrument]
async fn run_server() -> MainResult {
    let settings = load_runtime_settings()?;
    let database = open_database(&settings)?;

    let (gateway_manager, gateway_handle) = Actor::spawn(
        None,
        GatewayManagerActor,
        GatewayManagerArgs {
            settings: settings.clone(),
            database: Arc::clone(&database),
        },
    )
    .await
    .map_err(|error| std::io::Error::other(error.to_string()))?;

    let state = build_app_state(&settings, gateway_manager, database);

    let server_result = start_server(state, &settings.server.host, settings.server.port).await;

    info!("stopping gateway manager actor");
    gateway_handle.abort();

    server_result.map_err(Into::into)
}

fn load_runtime_settings() -> Result<RuntimeSettings, std::io::Error> {
    let settings = RuntimeSettings::from_env().map_err(std::io::Error::other)?;

    info!(
        gateway_id = %settings.gateway_id,
        host = %settings.server.host,
        port = settings.server.port,
        auth_required = settings.server.auth_token.is_some(),
        "runtime settings loaded"
    );

    Ok(settings)
}

fn open_database(settings: &RuntimeSettings) -> Result<Arc<DatabaseRepo>, crate::db::DbError> {
    let database = DatabaseRepo::open(&settings.database.path)?;
    info!(db_path = %settings.database.path, "database opened");
    Ok(Arc::new(database))
}

fn build_app_state(
    settings: &RuntimeSettings,
    gateway_manager: ractor::ActorRef<GatewayManagerMessage>,
    database: Arc<DatabaseRepo>,
) -> AppState {
    AppState {
        gateway_manager,
        gateway_id: settings.gateway_id.clone(),
        auth_token: settings.server.auth_token.clone(),
        db: database,
    }
}
