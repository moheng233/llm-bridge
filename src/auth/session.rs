//! Session 管理（Phase 1.5）。
//!
//! 基于 `tower-sessions` + `MemoryStore` 实现服务端 Session。
//! Session 中存储 OIDC 流程中的 csrf_token、nonce，以及登录后的 user_id。

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
