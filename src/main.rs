use std::sync::Arc;

use llm_bridge::actors;
use llm_bridge::config;
use llm_bridge::observability;
use llm_bridge::server;
use llm_bridge::store::Store;

use ractor::Actor;
use rustls::crypto::ring::default_provider;
use tracing::info;

use crate::actors::gateway_manager::{
    GatewayManagerActor, GatewayManagerArgs,
};
use crate::config::models::RuntimeSettings;
use crate::server::openai_api::AppState;
use crate::server::openai_api::start_server;

type MainResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

#[tokio::main]
async fn main() -> MainResult {
    default_provider()
        .install_default()
        .expect("failed to install rustls ring crypto provider");

    let observability = observability::init("llm-bridge")?;

    info!("llm-bridge process starting");

    let server_result = run_server().await;
    observability.shutdown();
    server_result
}

#[tracing::instrument]
async fn run_server() -> MainResult {
    let settings = load_runtime_settings()?;
    let store = open_store(&settings.store_path)?;

    let (gateway_manager, gateway_handle) = Actor::spawn(
        None,
        GatewayManagerActor,
        GatewayManagerArgs {
            settings: settings.clone(),
            store: Arc::clone(&store),
        },
    )
    .await
    .map_err(|error| std::io::Error::other(error.to_string()))?;

    let state = AppState {
        gateway_manager,
        store,
        auth_token: settings.server.auth_token.clone(),
    };

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
        store_path = %settings.store_path,
        auth_required = settings.server.auth_token.is_some(),
        catalog_base_url = %settings.model_catalog.base_url,
        "runtime settings loaded"
    );

    Ok(settings)
}

fn open_store(path: &str) -> Result<Arc<Store>, std::io::Error> {
    let store = Store::open(path).map_err(std::io::Error::other)?;
    info!(store_path = %path, "store opened");
    Ok(Arc::new(store))
}
