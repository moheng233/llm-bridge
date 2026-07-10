use std::sync::Arc;

use llm_bridge::actors;
use llm_bridge::auth::oidc::OidcService;
use llm_bridge::config;
use llm_bridge::db;
use llm_bridge::observability;
use llm_bridge::server;
use llm_bridge::server::auth::AuthState;
use llm_bridge::store::Store;

use ractor::Actor;
use rustls::crypto::ring::default_provider;
use tracing::{info, warn};

use crate::actors::gateway_manager::{GatewayManagerActor, GatewayManagerArgs};
use crate::config::models::RuntimeSettings;
use crate::server::AppState;
use crate::server::start_server;

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

    // Phase 3: Store 现在由 toasty Db 构建，不再用 JSON 文件
    let db = db::init(
        db::all_models(),
        &format!("sqlite:{}/llm-bridge.db", settings.store_path),
    )
    .await
    .map_err(|e| std::io::Error::other(e.to_string()))?;

    let store = Arc::new(Store::new(db.clone()));
    info!(store_path = %settings.store_path, "store initialized");

    // Phase 1: OIDC discovery（如果配置了 OIDC）
    let auth_state = if let Some(oidc_config) = &settings.oidc {
        match OidcService::discover(oidc_config).await {
            Ok(oidc) => {
                info!("OIDC service initialized");
                Some(AuthState {
                    oidc,
                    db: db.clone(),
                })
            }
            Err(e) => {
                warn!(error = %e, "OIDC initialization failed — continuing without OIDC");
                None
            }
        }
    } else {
        None
    };

    let (gateway_manager, gateway_handle) = Actor::spawn(
        None,
        GatewayManagerActor,
        GatewayManagerArgs {
            settings: settings.clone(),
            store: Arc::clone(&store),
            db: db.clone(),
        },
    )
    .await
    .map_err(|error| std::io::Error::other(error.to_string()))?;

    let state = AppState {
        gateway_manager,
        store,
        auth_token: settings.server.auth_token.clone(),
        auth: auth_state,
        db: db.clone(),
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
        "runtime settings loaded"
    );

    Ok(settings)
}
