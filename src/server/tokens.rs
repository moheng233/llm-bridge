//! Token 管理 API（Phase 2.3）。
//!
//! | 端点 | 方法 | 认证 | 说明 |
//! |------|------|------|------|
//! | `/api/v1/tokens` | GET | Session | 列出当前用户的所有 Token |
//! | `/api/v1/tokens` | POST | Session | 创建新 Token（返回明文，仅此一次） |
//! | `/api/v1/tokens/{id}` | PATCH | Session | 更新 Token 配置 |
//! | `/api/v1/tokens/{id}` | DELETE | Session | 删除 Token |

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tracing::instrument;

use crate::auth::token::{self, CreateTokenRequest, CreateTokenResponse, UpdateTokenRequest};
use crate::middleware::session_auth::SessionAuth;
use crate::server::openai_api::AppState;

/// Token 列表项（不含 token_hash）。
#[derive(Debug, Serialize)]
pub struct TokenListItem {
    pub id: u64,
    pub name: String,
    pub token_prefix: String,
    pub allowed_models: Vec<String>,
    pub request_quota: i64,
    pub token_quota: i64,
    pub quota_period: String,
    pub active: bool,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

impl From<crate::db::models::Token> for TokenListItem {
    fn from(t: crate::db::models::Token) -> Self {
        let allowed_models = token::parse_allowed_models(&t);
        Self {
            id: t.id,
            name: t.name,
            token_prefix: t.token_prefix,
            allowed_models,
            request_quota: t.request_quota,
            token_quota: t.token_quota,
            quota_period: t.quota_period,
            active: t.active,
            created_at: t.created_at.as_millisecond() / 1000,
            last_used_at: t.last_used_at,
        }
    }
}

// ── GET /api/v1/tokens ──

#[instrument(level = "debug", skip(state))]
pub async fn list_tokens(
    State(state): State<AppState>,
    SessionAuth(user): SessionAuth,
) -> Result<Json<Vec<TokenListItem>>, Response> {
    let tokens = token::list_user_tokens(&state.db, user.user_id)
        .await
        .map_err(|e| internal_error(&e))?;

    Ok(Json(tokens.into_iter().map(TokenListItem::from).collect()))
}

// ── POST /api/v1/tokens ──

#[instrument(level = "info", skip(state))]
pub async fn create_token(
    State(state): State<AppState>,
    SessionAuth(user): SessionAuth,
    Json(req): Json<CreateTokenRequest>,
) -> Result<Json<CreateTokenResponse>, Response> {
    let response = token::create_token(&state.db, user.user_id, req)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e})),
            )
                .into_response()
        })?;

    Ok(Json(response))
}

// ── PATCH /api/v1/tokens/{id} ──

#[instrument(level = "info", skip(state))]
pub async fn update_token(
    State(state): State<AppState>,
    SessionAuth(user): SessionAuth,
    Path(token_id): Path<u64>,
    Json(req): Json<UpdateTokenRequest>,
) -> Result<Json<TokenListItem>, Response> {
    // 验证所有权
    let token = get_token_or_not_found(&state.db, token_id).await?;

    if token.user_id != user.user_id {
        return Err(forbidden("not your token"));
    }

    let updated = token::update_token(&state.db, token_id, req)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e})),
            )
                .into_response()
        })?;

    Ok(Json(TokenListItem::from(updated)))
}

// ── DELETE /api/v1/tokens/{id} ──

#[instrument(level = "info", skip(state))]
pub async fn delete_token(
    State(state): State<AppState>,
    SessionAuth(user): SessionAuth,
    Path(token_id): Path<u64>,
) -> Result<Response, Response> {
    // 验证所有权
    let token = get_token_or_not_found(&state.db, token_id).await?;

    if token.user_id != user.user_id {
        return Err(forbidden("not your token"));
    }

    token::delete_token(&state.db, token_id)
        .await
        .map_err(|e| internal_error(&e))?;

    Ok((StatusCode::OK, Json(serde_json::json!({"deleted": true}))).into_response())
}

// ── Helpers ──

async fn get_token_or_not_found(
    db: &crate::db::Db,
    token_id: u64,
) -> Result<crate::db::models::Token, Response> {
    match token::get_token(db, token_id).await {
        Ok(t) => Ok(t),
        Err(e) => Err(not_found(&format!("token not found: {e}"))),
    }
}

fn internal_error(msg: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": msg})),
    )
        .into_response()
}

fn not_found(msg: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": msg})),
    )
        .into_response()
}

fn forbidden(msg: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({"error": msg})),
    )
        .into_response()
}
