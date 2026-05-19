//! 中间件模块 — 认证与授权提取器。
//!
//! 提供两个 axum 提取器：
//! - [`SessionAuth`]: Session Cookie 认证（用于 Web 前端 / Admin API）
//! - [`TokenAuth`]: Bearer Token 认证（用于 API 调用）

pub mod session_auth;
pub mod token_auth;
