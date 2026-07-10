//! Session 认证提取器（Phase 2.6）。
//!
//! 从 `tower-sessions` Session 中提取 [`SessionUser`]，
//! 可选的 Admin 角色检查。

use axum::{
    Json,
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use tower_sessions::Session;

use crate::auth::session::SessionUser;

/// 已认证的 Session 用户提取器。
///
/// 从 Session Cookie 中提取 [`SessionUser`]，未认证返回 401。
///
/// # 使用示例
///
/// ```ignore
/// async fn my_handler(SessionAuth(user): SessionAuth) -> impl IntoResponse {
///     // user: SessionUser { user_id, name, role }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct SessionAuth(pub SessionUser);

impl<S> FromRequestParts<S> for SessionAuth
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, _state)
            .await
            .map_err(|_e| unauthorized("failed to read session"))?;

        let user: SessionUser = session
            .get("user")
            .await
            .map_err(|_e| unauthorized("failed to read session"))?
            .ok_or_else(|| unauthorized("not authenticated"))?;

        Ok(SessionAuth(user))
    }
}

/// Admin 角色提取器 — 仅在 SessionUser 角色为 `admin` 时通过。
///
/// # 使用示例
///
/// ```ignore
/// async fn admin_handler(AdminAuth(user): AdminAuth) -> impl IntoResponse {
///     // ...
/// }
/// ```
#[derive(Debug, Clone)]
pub struct AdminAuth(pub SessionUser);

impl<S> FromRequestParts<S> for AdminAuth
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let SessionAuth(user) = SessionAuth::from_request_parts(parts, state).await?;

        if user.role.to_lowercase() != "admin" {
            return Err(forbidden("admin role required"));
        }

        Ok(AdminAuth(user))
    }
}

fn unauthorized(msg: impl Into<String>) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"error": msg.into()})),
    )
        .into_response()
}

fn forbidden(msg: impl Into<String>) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({"error": msg.into()})),
    )
        .into_response()
}
