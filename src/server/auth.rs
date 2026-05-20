//! Auth API 端点（Phase 1.6）。
//!
//! | 端点 | 方法 | 说明 |
//! |------|------|------|
//! | `/auth/login` | GET | 发起 OIDC 登录，302 重定向到 IdP |
//! | `/auth/callback` | GET | OIDC 回调，验证后签发 Session |
//! | `/auth/me` | GET | 返回当前登录用户信息 |
//! | `/auth/logout` | POST | 销毁 Session |
//!
//! ## 无授权模式
//!
//! 当 OIDC 未配置（`RuntimeSettings.oidc` 为 `None`）时，系统自动进入无授权模式：
//! - 所有请求自动注入默认管理员 Session（user_id=0, name="admin", role="admin"）
//! - `/auth/login` 直接跳转到 `/`
//! - `/auth/me` 返回默认管理员信息
//! - 无需登录即可访问管理后台

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use tower_sessions::Session;
use tracing::{info, instrument, warn};

use crate::auth::session::{OidcContext, SessionUser};
use crate::db;
use crate::db::models::{User, UserRole};

/// 传递给 Auth 路由的共享状态。
#[derive(Clone)]
pub struct AuthState {
    pub oidc: crate::auth::oidc::OidcService,
    pub db: db::Db,
}

/// 无授权模式下的默认管理员 Session 用户。
fn no_auth_user() -> SessionUser {
    SessionUser {
        user_id: 0,
        name: "admin".to_string(),
        role: "admin".to_string(),
    }
}

// ── 无授权模式：自动注入管理员 Session ──

/// Axum 中间件：无 OIDC 配置时，自动为每个请求注入默认管理员 Session。
///
/// 仅当 Session 中尚无用户时注入；如果用户主动登出（Session flush），
/// 下次请求会重新注入。
pub async fn no_auth_middleware(
    session: Session,
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Response {
    let has_user: Option<SessionUser> = session.get("user").await.ok().flatten();
    if has_user.is_none() {
        let _ = session.insert("user", no_auth_user()).await;
    }
    next.run(request).await
}

/// 无授权模式下的 `/auth/login`：直接跳转到 `/`。
#[instrument(level = "debug")]
pub async fn no_auth_login() -> Redirect {
    Redirect::temporary("/")
}

// ── GET /auth/login ──

#[derive(Debug, Deserialize)]
pub struct LoginQuery {
    /// 登录成功后跳转回的目标页面（可选）
    pub next: Option<String>,
}

#[instrument(level = "info", skip(state, session, _query))]
pub async fn login(
    State(state): State<AuthState>,
    session: Session,
    Query(_query): Query<LoginQuery>,
) -> Result<Redirect, Response> {
    let (auth_url, csrf_token, nonce) = state.oidc.login_url();

    let context = OidcContext {
        csrf_token: csrf_token.clone(),
        nonce,
    };

    session
        .insert("oidc_context", context)
        .await
        .map_err(|e| internal_error(&e.to_string()))?;

    // 如果有 next 参数，存入 session 以便回调后跳转
    if let Some(next) = _query.next {
        session
            .insert("login_next", next)
            .await
            .map_err(|e| internal_error(&e.to_string()))?;
    }

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
    State(state): State<AuthState>,
    session: Session,
    Query(query): Query<CallbackQuery>,
) -> Result<Response, Response> {
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
        return Err((
            StatusCode::BAD_REQUEST,
            "CSRF state mismatch",
        )
            .into_response());
    }

    // OIDC 验证
    let oidc_user = state
        .oidc
        .callback(&query.code, &context.nonce)
        .await
        .map_err(|e| {
            warn!(error = %e, "OIDC callback failed");
            (StatusCode::UNAUTHORIZED, e).into_response()
        })?;

    // 清理 OIDC 上下文
    session
        .remove::<OidcContext>("oidc_context")
        .await
        .ok();

    // 查找或创建用户
    let user = upsert_user_from_oidc(&state.db, &oidc_user).await.map_err(|e| {
        warn!(error = %e, "failed to upsert user");
        internal_error("failed to create or update user")
    })?;

    // 写入 Session
    let session_user = SessionUser {
        user_id: user.id,
        name: user.name.clone(),
        role: format!("{:?}", user.role),
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

    // 跳转到目标页面或前端首页
    let next: Option<String> = session.get("login_next").await.ok().flatten();
    session.remove::<String>("login_next").await.ok();

    let redirect_to = next.unwrap_or_else(|| "/".to_string());
    Ok(Redirect::temporary(&redirect_to).into_response())
}

// ── GET /auth/me ──

#[instrument(level = "debug", skip(session))]
pub async fn me(session: Session) -> Result<Json<SessionUser>, Response> {
    let user: SessionUser = session.get("user").await.map_err(|e| {
        internal_error(&e.to_string())
    })?.ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, "not authenticated").into_response()
    })?;

    Ok(Json(user))
}

// ── POST /auth/logout ──

#[instrument(level = "debug", skip(session))]
pub async fn logout(session: Session) -> Result<Response, Response> {
    session.flush().await.map_err(|e| internal_error(&e.to_string()))?;
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
async fn upsert_user_from_oidc(db: &db::Db, oidc_user: &crate::auth::oidc::OidcUser) -> Result<User, String> {
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
        let all_users = User::all().exec(&mut db.clone()).await.map_err(|e| e.to_string())?;
        let is_first = all_users.is_empty();

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
