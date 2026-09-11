//! Auth API 端点（Phase 1.6）。
//!
//! | 端点 | 方法 | 说明 |
//! |------|------|------|
//! | `/auth/login` | GET | OIDC 已配置 → 302 重定向到 IdP；未配置 → 直接跳转 `/` |
//! | `/auth/me` | GET | 返回当前登录用户信息（DB 实时角色/状态） |
//! | `/auth/logout` | POST | 销毁 Session |
//!
//! ## 设计原则
//!
//! 无论 OIDC 是否配置，对外接口完全一致。handler 内部根据
//! `AppState.auth` 是否为 `Some` 决定具体行为：
//! - OIDC 已配置：标准 OIDC 流程
//! - OIDC 未配置：跳过认证步骤（`login` 直接跳转，`no_auth_middleware` 自动注入管理员 Session）

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use tracing::{info, instrument, warn};
use ts_rs::TS;

use crate::auth::session::{OidcContext, SessionUser};
use crate::db;
use crate::db::models::{User, UserRole};
use crate::server::AppState;

/// OIDC 子状态（仅在配置了 OIDC 时存在）。
#[derive(Clone)]
pub struct AuthState {
    pub oidc: crate::auth::oidc::OidcService,
    pub db: db::Db,
}

/// `/auth/me` 响应 — 统一对外角色 DTO（TS 绑定 camelCase：userId/name/role/email/avatarUrl）。
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct MeResponse {
    pub user_id: u64,
    pub name: String,
    /// `"admin"` 或 `"member"`
    pub role: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
}

// ── 无授权模式：自动注入管理员 Session ──

/// Axum 中间件：无 OIDC 配置时，自动为每个请求注入保留管理员 Session。
///
/// 管理员为 DB 中持久化的 `__no_auth_admin__` 用户（由 Deploy 在启动时
/// 调用 [`crate::auth::session::ensure_no_auth_admin_user`] 创建），
/// 每次 DB 查询构造 SessionUser（角色/存活与其他用户同一契约）。
/// 仅当 Session 中尚无用户时注入；登出后下次请求重新注入。
pub async fn no_auth_middleware(
    State(db): State<db::Db>,
    session: Session,
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Response {
    let has_user: Option<SessionUser> = session.get("user").await.ok().flatten();
    if has_user.is_none() {
        let user = crate::middleware::session_auth::load_live_user_by_sub(
            &db,
            crate::auth::session::NO_AUTH_ADMIN_SUB,
        )
        .await;
        match user {
            Ok(u) => {
                let _ = session.insert("user", u).await;
            }
            Err(_) => {
                // 保留用户缺失（启动未调用 ensure）：降级为 401 行为由
                // SessionAuth 提取器接管（Session 无 user → 401），不伪 admin。
                warn!("no-auth admin user missing in DB — requests will be unauthenticated");
            }
        }
    }
    next.run(request).await
}

// ── GET /auth/login ──

#[derive(Debug, Deserialize)]
pub struct LoginQuery {
    pub next: Option<String>,
}

#[instrument(level = "info", skip(state, session))]
pub async fn login(
    State(state): State<AppState>,
    session: Session,
    Query(query): Query<LoginQuery>,
) -> Result<Redirect, Response> {
    let next = query
        .next
        .filter(|path| safe_login_next(path))
        .unwrap_or_else(|| "/".into());
    let Some(auth) = &state.auth else {
        return Ok(Redirect::temporary(&next));
    };

    let (auth_url, csrf_token, nonce) = auth.oidc.login_url();

    let context = OidcContext {
        csrf_token: csrf_token.clone(),
        nonce,
    };

    session
        .insert("oidc_context", context)
        .await
        .map_err(|e| internal_error(&e.to_string()))?;

    session
        .insert("login_next", next)
        .await
        .map_err(|error| internal_error(&error.to_string()))?;

    Ok(Redirect::temporary(&auth_url))
}

// ── GET /auth/callback ──

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: String,
    pub state: String,
}

#[instrument(level = "info", skip(state, session))]
pub async fn callback(
    State(state): State<AppState>,
    session: Session,
    Query(query): Query<CallbackQuery>,
) -> Result<Response, Response> {
    let auth = state
        .auth
        .as_ref()
        .ok_or_else(|| (StatusCode::NOT_FOUND, "OIDC not configured").into_response())?;

    // 验证 CSRF state
    let context: OidcContext = session
        .get("oidc_context")
        .await
        .map_err(|e| internal_error(&e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "session expired or missing OIDC context",
            )
                .into_response()
        })?;

    if query.state != context.csrf_token {
        return Err((StatusCode::BAD_REQUEST, "CSRF state mismatch").into_response());
    }

    // OIDC 验证
    let oidc_user = auth
        .oidc
        .callback(&query.code, &context.nonce)
        .await
        .map_err(|e| {
            warn!(error = %e, "OIDC callback failed");
            (StatusCode::UNAUTHORIZED, e).into_response()
        })?;

    // 查找或创建用户
    let user = upsert_user_from_oidc(&auth.db, &oidc_user)
        .await
        .map_err(|e| {
            warn!(error = %e, "failed to upsert user");
            internal_error("failed to create or update user")
        })?;

    // 写入 Session（role 用统一小写字符串；实际授权仍以 DB 实时查询为准）
    let session_user = SessionUser {
        user_id: user.id,
        name: user.name.clone(),
        role: crate::auth::session::role_to_str(user.role).to_string(),
    };

    session
        .insert("user", session_user)
        .await
        .map_err(|e| internal_error(&e.to_string()))?;

    info!(
        user_id = user.id,
        oidc_sub = %oidc_user.sub,
        "user authenticated via OIDC"
    );

    let next: Option<String> = session.get("login_next").await.ok().flatten();
    session.remove::<String>("login_next").await.ok();

    let redirect_to = next
        .filter(|path| safe_login_next(path))
        .unwrap_or_else(|| "/".into());
    Ok(Redirect::temporary(&redirect_to).into_response())
}

fn safe_login_next(path: &str) -> bool {
    path.starts_with('/')
        && !path.starts_with("//")
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
}

// ── GET /auth/me ──

#[instrument(level = "debug", skip(state, session))]
pub async fn me(
    State(state): State<AppState>,
    session: Session,
) -> Result<Json<MeResponse>, Response> {
    let cached: SessionUser = session
        .get("user")
        .await
        .map_err(|e| internal_error(&e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "not authenticated"})),
            )
                .into_response()
        })?;

    // 每次查询 DB 实时角色与存活状态（与 SessionAuth 同一契约）。
    let user = crate::middleware::session_auth::load_live_user(&state.db, cached.user_id)
        .await
        .map_err(|message| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": message})),
            )
                .into_response()
        })?;
    let row = User::get_by_id(&mut state.db.clone(), &user.user_id)
        .await
        .map_err(|e| internal_error(&e.to_string()))?;
    let (email, avatar_url) = (row.email, row.avatar_url);

    Ok(Json(MeResponse {
        user_id: user.user_id,
        name: user.name,
        role: user.role,
        email,
        avatar_url,
    }))
}

// ── POST /auth/logout ──

#[instrument(level = "debug", skip(session))]
pub async fn logout(session: Session) -> Result<Response, Response> {
    session
        .flush()
        .await
        .map_err(|e| internal_error(&e.to_string()))?;
    Ok((StatusCode::OK, "logged out").into_response())
}

// ── Internal helpers ──

fn internal_error(msg: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": msg})),
    )
        .into_response()
}

/// 查找或创建 OIDC 用户（管理员首任机制）。
///
/// - 如果 `oidc_sub` 已存在 → 更新 name / email / avatar_url
/// - 如果数据库中尚无任何用户 → 自动赋予 Admin 角色
/// - 否则 → 赋予 Member 角色
async fn upsert_user_from_oidc(
    db: &db::Db,
    oidc_user: &crate::auth::oidc::OidcUser,
) -> Result<User, String> {
    if oidc_user.sub == crate::auth::session::NO_AUTH_ADMIN_SUB {
        return Err("OIDC subject conflicts with reserved local identity".into());
    }
    // 查找是否已存在
    let existing = User::filter(User::fields().oidc_sub().eq(&oidc_user.sub))
        .exec(&mut db.clone())
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .next();

    if let Some(mut user) = existing {
        // 更新用户信息
        if let Some(ref email) = oidc_user.email {
            user.email = Some(email.clone());
        }
        if let Some(ref avatar) = oidc_user.avatar_url {
            user.avatar_url = Some(avatar.clone());
        }
        user.updated_at = jiff::Timestamp::now();

        User::filter(User::fields().id().eq(user.id))
            .update()
            .name(oidc_user.name.clone())
            .email(user.email.clone())
            .avatar_url(user.avatar_url.clone())
            .updated_at(user.updated_at)
            .exec(&mut db.clone())
            .await
            .map_err(|e| e.to_string())?;

        Ok(user)
    } else {
        // 检查是否首个用户
        let all_users = User::all()
            .exec(&mut db.clone())
            .await
            .map_err(|e| e.to_string())?;
        let is_first = all_users
            .iter()
            .all(|user| user.oidc_sub == crate::auth::session::NO_AUTH_ADMIN_SUB);

        let role = if is_first {
            info!(
                email = ?oidc_user.email,
                "first user promoted to admin"
            );
            UserRole::Admin
        } else {
            UserRole::Member
        };

        let user = toasty::create!(User {
            oidc_sub: oidc_user.sub.clone(),
            name: oidc_user.name.clone(),
            email: oidc_user.email.clone().unwrap_or_default(),
            role,
            active: true,
        })
        .exec(&mut db.clone())
        .await
        .map_err(|e| e.to_string())?;

        Ok(user)
    }
}
