//! 认证模块 — OIDC 单点登录、Session 管理、Token 认证、配额管理。
//!
//! 子模块：
//! - `oidc`: OIDC Service（Phase 1.4 ✅）
//! - `session`: Session 数据类型（Phase 1.5 ✅）
//! - `token`: API Token 服务（Phase 2.1 ✅）
//! - `quota`: 配额服务（Phase 2.2 ✅）

pub mod oidc;
pub mod session;
pub mod token;
pub mod quota;
