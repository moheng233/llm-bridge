use std::sync::Arc;

use llm_bridge::actors;
use llm_bridge::auth::oidc::OidcService;
use llm_bridge::config;
use llm_bridge::db;
use llm_bridge::http::ensure_crypto_provider;
use llm_bridge::observability;
use llm_bridge::server;
use llm_bridge::server::auth::AuthState;
use llm_bridge::store::Store;

use ractor::Actor;
use tracing::{info, warn};

use crate::actors::gateway_manager::{GatewayManagerActor, GatewayManagerArgs};
use crate::config::models::RuntimeSettings;
use crate::server::AppState;
use crate::server::start_server;

type MainResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

#[tokio::main]
async fn main() -> MainResult {
    // 统一入口：安装 rustls crypto provider（reqwest 使用 rustls-no-provider 特性）
    ensure_crypto_provider();

    let observability = observability::init("llm-bridge")?;

    info!("llm-bridge process starting");

    let server_result = run_server().await;
    tokio::task::spawn_blocking(move || observability.shutdown()).await?;
    server_result
}

#[tracing::instrument]
async fn run_server() -> MainResult {
    let settings = load_runtime_settings()?;

    let db = match &settings.database_url {
        Some(url) => db::init(db::all_models(), url).await,
        None => db::init_sqlite(db::all_models(), std::path::Path::new(&settings.store_path)).await,
    }
    .map_err(|error| std::io::Error::other(error.to_string()))?;

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
    if auth_state.is_none() {
        llm_bridge::auth::session::ensure_no_auth_admin_user(&db)
            .await
            .map_err(std::io::Error::other)?;
    }

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

    let trace_writer = llm_bridge::observability::trace_writer::TraceWriter::spawn(db.clone());
    let capture_content = settings.observability.capture_content;

    // 保留策略后台任务（PLAN.md §5 O5）：定期清理过期 trace；usage_daily 永久保留。
    let retention_handle = llm_bridge::observability::retention::spawn_retention_task(
        db.clone(),
        settings.observability.trace_retention_days,
    );

    let state = AppState {
        gateway_manager,
        store,
        auth_token: settings.server.auth_token.clone(),
        auth: auth_state,
        db: db.clone(),
        trace_writer,
        capture_content,
        public_base_url: settings.public_base_url.clone(),
        catalog: Arc::new(llm_bridge::server::models_dev::CatalogService::new(
            settings.models_import.source_url.clone(),
        )),
    };

    info!(
        capture_content,
        trace_retention_days = settings.observability.trace_retention_days,
        "observability: trace writer started"
    );

    let server_result = start_server(state, &settings.server.host, settings.server.port).await;

    info!("stopping gateway manager actor");
    gateway_handle.abort();
    if let Some(h) = retention_handle {
        h.abort();
    }

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
