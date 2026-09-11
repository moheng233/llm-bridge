//! HTTP 客户端构建统一入口：负责 rustls crypto provider 的安装。
//!
//! `reqwest` 以 `rustls-no-provider` 特性引入（见 `Cargo.toml`），该特性只启用
//! rustls 的 TLS 实现而不内嵌 crypto provider，因此进程必须在**构建第一个
//! `reqwest::Client` 之前**自行安装 provider，否则构建时会 panic：
//! `No rustls crypto provider is configured`。
//!
//! provider 是进程级全局状态且只能安装一次，所以本模块是库内唯一安装点：
//!
//! - [`ensure_crypto_provider`]：幂等安装 ring provider。任何自行构建
//!   `reqwest::Client` 的入口（bin / example / test）都应先调用它；
//! - [`client_builder`]：安装 provider 后返回带统一 User-Agent 的
//!   [`reqwest::ClientBuilder`]。
//!
//! 库内构建 `reqwest::Client` 的代码一律走 [`client_builder`]，
//! 避免新增调用点时遗漏安装。

use std::sync::Once;

/// 保证 provider 安装只执行一次（重复安装会返回 `Err`）。
static CRYPTO_PROVIDER: Once = Once::new();

/// 幂等安装 rustls crypto provider（ring）。
///
/// 重复调用安全：若进程内已安装 provider（无论由本模块还是其他代码安装），
/// 直接复用既有 provider。
pub fn ensure_crypto_provider() {
    CRYPTO_PROVIDER.call_once(|| {
        if let Err(existing) = rustls::crypto::ring::default_provider().install_default() {
            // 进程内已有其他 provider（如 aws-lc-rs），复用之，不覆盖。
            tracing::debug!(
                provider = ?existing,
                "rustls crypto provider already installed, reusing it"
            );
        }
    });
}

/// 构建带统一 User-Agent 的 [`reqwest::ClientBuilder`]。
///
/// 返回前会先调用 [`ensure_crypto_provider`]，调用方无需再关心 provider 安装。
pub fn client_builder() -> reqwest::ClientBuilder {
    ensure_crypto_provider();

    reqwest::Client::builder().user_agent(concat!(
        env!("CARGO_PKG_NAME"),
        "/",
        env!("CARGO_PKG_VERSION")
    ))
}
