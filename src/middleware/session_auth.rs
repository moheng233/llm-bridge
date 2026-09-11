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
use crate::db::models::{User, UserRole};

/// 已认证的 Session 用户提取器。
///
/// 从 Session Cookie 中提取 `user_id`，并**每次授权查询 DB** 校验：
/// - 用户仍存在且 `active = true`（否则 401，降权/禁用即时生效）
/// - 角色以 DB 当前值为准（Session 中缓存的 role 仅作参考，不作为授权依据）
///
/// 返回的 [`SessionUser`] 携带 DB 实时角色。
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

impl FromRequestParts<crate::server::AppState> for SessionAuth {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &crate::server::AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_e| unauthorized("failed to read session"))?;

        let cached: SessionUser = session
            .get("user")
            .await
            .map_err(|_e| unauthorized("failed to read session"))?
            .ok_or_else(|| unauthorized("not authenticated"))?;

        let user = load_live_user(&state.db, cached.user_id)
            .await
            .map_err(unauthorized)?;

        // 缓存角色与 DB 不一致时刷新 Session（下次请求少一次写）。
        if user.role != cached.role || user.name != cached.name {
            let _ = session
                .insert(
                    "user",
                    SessionUser {
                        user_id: user.user_id,
                        name: user.name.clone(),
                        role: user.role.clone(),
                    },
                )
                .await;
        }

        Ok(SessionAuth(user))
    }
}

/// 从 DB 加载当前有效用户；不存在 / 禁用 → 401。
pub(crate) async fn load_live_user(
    db: &crate::db::Db,
    user_id: u64,
) -> Result<SessionUser, &'static str> {
    let row = User::get_by_id(&mut db.clone(), &user_id)
        .await
        .map_err(|_e| "session user no longer exists")?;

    if !row.active {
        return Err("account disabled");
    }

    Ok(SessionUser {
        user_id: row.id,
        name: row.name,
        role: crate::auth::session::role_to_str(row.role).to_string(),
    })
}

pub(crate) async fn load_live_user_by_sub(
    db: &crate::db::Db,
    sub: &str,
) -> Result<SessionUser, &'static str> {
    let row = User::filter(User::fields().oidc_sub().eq(sub))
        .exec(&mut db.clone())
        .await
        .map_err(|_| "failed to load session user")?
        .into_iter()
        .next()
        .ok_or("session user no longer exists")?;
    load_live_user(db, row.id).await
}

/// Admin 角色提取器 — 仅在实时角色为 `admin` 时通过。
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

impl FromRequestParts<crate::server::AppState> for AdminAuth {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &crate::server::AppState,
    ) -> Result<Self, Self::Rejection> {
        let SessionAuth(user) = SessionAuth::from_request_parts(parts, state).await?;

        if !user.is_admin() {
            return Err(forbidden("admin role required"));
        }

        Ok(AdminAuth(user))
    }
}

/// 帮助 handler 区分 admin / member（W5 权限过滤用）。
pub fn is_admin_role(role: &str) -> bool {
    role.eq_ignore_ascii_case("admin")
}

/// DB 角色字符串（与 `role_to_str` 对齐）。
pub const ROLE_ADMIN: UserRole = UserRole::Admin;

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
