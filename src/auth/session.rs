//! Session 管理（Phase 1.5）。
//!
//! 基于 `tower-sessions` + `MemoryStore` 实现服务端 Session。
//! Session 中存储 OIDC 流程中的 csrf_token、nonce，以及登录后的 user_id。

use crate::db::models::User;
use crate::db::models::UserRole;
use serde::{Deserialize, Serialize};

/// Session 中存储的 OIDC 上下文（登录流程中间状态）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcContext {
    /// CSRF 防护 token
    pub csrf_token: String,
    /// ID Token 防重放 nonce
    pub nonce: String,
}

/// Session 中存储的已登录用户信息（精简，避免冗余查询）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUser {
    pub user_id: u64,
    pub name: String,
    pub role: String,
}

impl SessionUser {
    /// 角色是否为 admin（大小写不敏感，兼容历史 Session 数据）。
    pub fn is_admin(&self) -> bool {
        self.role.eq_ignore_ascii_case("admin")
    }
}

/// [`UserRole`] → 对外角色字符串（与 Session / UserResponse 序列化统一）。
pub fn role_to_str(role: UserRole) -> &'static str {
    match role {
        UserRole::Admin => "admin",
        UserRole::Member => "member",
    }
}

/// no-auth 模式保留管理员用户（`users.oidc_sub` 固定值）。
///
/// 命名空间化避免与真实 OIDC `sub` 碰撞；[`upsert_user_from_oidc`]
/// 的"首个用户升级 Admin"判断会排除此保留 sub。
pub const NO_AUTH_ADMIN_SUB: &str = "__no_auth_admin__";

/// 查找（不创建）保留 no-auth 管理员用户。
pub async fn get_no_auth_admin_user(db: &crate::db::Db) -> Result<Option<User>, String> {
    let rows = User::filter(User::fields().oidc_sub().eq(NO_AUTH_ADMIN_SUB))
        .exec(&mut db.clone())
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows.into_iter().next())
}

/// 确保保留 no-auth 管理员用户存在并返回其 id。
///
/// 无授权模式启动时由 Deploy 在 `main` 中调用一次（auth 为 None 时），
/// 后续 no_auth_middleware 按固定 sub 从 DB 实时读取构造 SessionUser。
pub async fn ensure_no_auth_admin_user(db: &crate::db::Db) -> Result<u64, String> {
    if let Some(user) = get_no_auth_admin_user(db).await? {
        return Ok(user.id);
    }
    let user = toasty::create!(User {
        oidc_sub: NO_AUTH_ADMIN_SUB.to_string(),
        name: "admin".to_string(),
        role: crate::db::models::UserRole::Admin,
        active: true,
    })
    .exec(&mut db.clone())
    .await
    .map_err(|e| e.to_string())?;
    Ok(user.id)
}
