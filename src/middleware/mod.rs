//! 中间件模块 — 认证与授权提取器。
//!
//! 提供两个 axum 提取器：
//! - [`SessionAuth`]: Session Cookie 认证（用于 Web 前端 / Admin API）
//! - [`TokenAuth`]: Bearer Token 认证（用于 API 调用）
//!
//! 以及请求标识中间件：
//! - [`request_id`]: 为每个请求生成唯一 `request_id` 并贯穿全链路（PLAN.md §5 O1）

pub mod request_id;
pub mod session_auth;
pub mod token_auth;
