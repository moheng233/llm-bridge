//! HTTP server module — route definitions and server startup.
//!
//! All route groups use [`axfetchum::ApiRouter`] to simultaneously build
//! Axum routers and collect metadata for TypeScript client generation.
//!
//! Auth routes are unified: the same endpoints work with or without OIDC.
//! Handlers check `AppState.auth` internally to decide behavior.

// Handler 统一返回 `Result<T, axum::Response>`(axum 官方惯用法)。
// `Response<Body>` 体积 ≥ 128 字节,会触发 `clippy::result_large_err`。
// 此模式经社区广泛验证,在此模块级别显式 allow。
#![allow(clippy::result_large_err)]

pub mod admin;
pub mod auth;
pub mod openai_api;
pub mod tokens;

use std::sync::Arc;

use axfetchum::ApiRouter;
use tower_sessions::MemoryStore;
use tower_sessions::SessionManagerLayer;
use tracing::{info, instrument};

use crate::actors::gateway_manager::GatewayManagerMessage;
use crate::db;
use crate::store::Store;

use crate::server::admin::{admin_crud_routes, model_browse_routes};
use crate::server::auth::AuthState;

/// Shared application state for HTTP handlers.
#[derive(Clone)]
pub struct AppState {
    pub gateway_manager: ractor::ActorRef<GatewayManagerMessage>,
    pub store: Arc<Store>,
    pub auth_token: Option<String>,
    /// OIDC auth sub-state（仅在配置了 OIDC 时 Some）
    pub auth: Option<AuthState>,
    /// SQLite 数据库句柄（始终可用）
    pub db: db::Db,
}

// ── Route definitions (all via ApiRouter for TS client generation) ──

/// OpenAI-compatible API routes (`/v1/models`, `/v1/chat/completions`).
fn openai_routes() -> ApiRouter<AppState> {
    ApiRouter::<AppState>::new()
        .group("openai")
        .get("/v1/models", openai_api::list_models)
        .response::<openai_api::OpenAiModelList>()
        .auth()
        .done()
        .post("/v1/chat/completions", openai_api::chat_completions)
        .done()
}

/// Token management routes (`/api/v1/tokens`).
fn token_routes() -> ApiRouter<AppState> {
    ApiRouter::<AppState>::new()
        .group("tokens")
        .get("/api/v1/tokens", tokens::list_tokens)
        .response::<Vec<tokens::TokenListItem>>()
        .auth()
        .done()
        .post("/api/v1/tokens", tokens::create_token)
        .json::<crate::auth::token::CreateTokenRequest, crate::auth::token::CreateTokenResponse>()
        .auth()
        .done()
        .patch("/api/v1/tokens/{id}", tokens::update_token)
        .json::<crate::auth::token::UpdateTokenRequest, tokens::TokenListItem>()
        .auth()
        .done()
        .delete("/api/v1/tokens/{id}", tokens::delete_token)
        .auth()
        .done()
}

/// Auth routes — works in both OIDC and no-auth mode.
fn auth_routes() -> ApiRouter<AppState> {
    ApiRouter::<AppState>::new()
        .group("auth")
        .get("/auth/login", auth::login)
        .redirect()
        .done()
        .get("/auth/callback", auth::callback)
        .redirect()
        .done()
        .get("/auth/me", auth::me)
        .done()
        .post("/auth/logout", auth::logout)
        .done()
}

/// Merge all route collections (for TypeScript client generation via axfetchum).
pub fn all_api_routes() -> ApiRouter<AppState> {
    model_browse_routes()
        .merge(admin_crud_routes())
        .merge(openai_routes())
        .merge(token_routes())
        .merge(auth_routes())
}

/// Start the HTTP server on the given host:port.
#[instrument(
    level = "info",
    skip(state),
    fields(
        host = %host,
        port,
        oidc_configured = state.auth.is_some()
    )
)]
pub async fn start_server(state: AppState, host: &str, port: u16) -> Result<(), std::io::Error> {
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store).with_secure(false);

    let oidc_configured = state.auth.is_some();

    let (router, _routes) = all_api_routes().build();
    let app = router.with_state(state);
    let mut app: axum::Router = app;

    if oidc_configured {
        app = app.layer(session_layer);
    } else {
        info!("OIDC not configured — entering no-auth mode (auto-inject default admin)");
        // session_layer 必须是最外层，确保 no_auth_middleware 访问 Session 时已加载
        app = app
            .layer(axum::middleware::from_fn(auth::no_auth_middleware))
            .layer(session_layer);
    }

    let addr = format!("{}:{}", host, port);
    info!("Starting server on {}", addr);

    // ── 嵌入前端静态文件（可选 feature） ──
    #[cfg(feature = "embed-frontend")]
    let app = {
        app.fallback(axum::routing::get(
            |req: axum::http::Request<axum::body::Body>| async move {
                crate::embed::serve(req.uri().path()).await
            },
        ))
    };

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await
}
