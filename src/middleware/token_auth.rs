//! Bearer Token 认证提取器（Phase 2.5）。
//!
//! 从 `Authorization: Bearer <token>` 头中提取并验证 API Token。
//! 验证通过后返回对应的数据库 [`Token`] 记录，后续 handler 可据此
//! 检查模型权限和配额。

use axum::{
    Json,
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use tracing::warn;

use crate::db::models::Token;
use crate::server::AppState;

/// Bearer Token 认证提取器。
///
/// 从请求头提取 Bearer Token，bcrypt 验证后返回数据库中的 Token 行。
///
/// # 使用示例
///
/// ```ignore
/// async fn chat_completions(
///     TokenAuth(token): TokenAuth,
///     ...
/// ) -> impl IntoResponse {
///     // token: Token { id, user_id, allowed_models, ... }
/// }
/// ```
#[derive(Debug)]
pub struct TokenAuth(pub Token);

impl FromRequestParts<AppState> for TokenAuth {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Extract Bearer token from Authorization header
        let header = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| unauthorized("missing Authorization header"))?;

        let token_str = header.strip_prefix("Bearer ").ok_or_else(|| {
            unauthorized("invalid Authorization format, expected 'Bearer <token>'")
        })?;

        // Look up token in database
        // Load all active tokens (small team assumption — 少量 token)
        let all_tokens = Token::all()
            .exec(&mut state.db.clone())
            .await
            .map_err(|e| {
                warn!(error = %e, "failed to query tokens");
                internal_error("database error")
            })?;

        // bcrypt verify each token
        for token in all_tokens {
            if !token.active {
                continue;
            }

            let is_match = bcrypt::verify(token_str, &token.token_hash).unwrap_or(false);
            if is_match {
                return Ok(TokenAuth(token));
            }
        }

        Err(unauthorized("invalid or inactive API token"))
    }
}

fn unauthorized(msg: impl Into<String>) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"error": {"message": msg.into(), "type": "unauthorized", "code": "unauthorized"}})),
    )
        .into_response()
}

fn internal_error(msg: impl Into<String>) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": {"message": msg.into(), "type": "internal_error", "code": "internal_error"}})),
    )
        .into_response()
}
