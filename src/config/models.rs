use std::env;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSettings {
    pub gateway_id: String,
    pub server: ServerConfig,
    pub store_path: String,
    pub oidc: Option<OidcConfig>,
}

impl RuntimeSettings {
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            gateway_id: env_or_default("LLM_BRIDGE_GATEWAY_ID", "llm-bridge-v1"),
            server: ServerConfig::from_env()?,
            store_path: env_or_default("LLM_BRIDGE_STORE_PATH", "./data/llm-bridge"),
            oidc: OidcConfig::from_env_optional()?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub auth_token: Option<String>,
}

impl ServerConfig {
    fn from_env() -> Result<Self, String> {
        Ok(Self {
            host: env_or_default("LLM_BRIDGE_HOST", "127.0.0.1"),
            port: parse_env_or_default("LLM_BRIDGE_PORT", 3000)?,
            auth_token: env::var("LLM_BRIDGE_AUTH_TOKEN").ok(),
        })
    }
}

// ── OIDC 配置 ──

/// OIDC 单点登录配置（环境变量，不存数据库）。
///
/// 仅当 `LLM_BRIDGE_OIDC_ISSUER_URL` 设置时启用 OIDC。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcConfig {
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub scopes: String,
    pub base_url: String,
}

impl OidcConfig {
    fn from_env_optional() -> Result<Option<Self>, String> {
        let issuer_url = match env::var("LLM_BRIDGE_OIDC_ISSUER_URL") {
            Ok(v) if !v.is_empty() => v,
            _ => return Ok(None),
        };

        Ok(Some(Self {
            issuer_url,
            client_id: env_or_default("LLM_BRIDGE_OIDC_CLIENT_ID", ""),
            client_secret: env_or_default("LLM_BRIDGE_OIDC_CLIENT_SECRET", ""),
            scopes: env_or_default(
                "LLM_BRIDGE_OIDC_SCOPES",
                "openid profile email",
            ),
            base_url: env_or_default("LLM_BRIDGE_BASE_URL", "http://localhost:3000"),
        }))
    }
}

// ── Provider compatibility ──

/// API compatibility protocol that a provider supports.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, TS, toasty::Embed)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum ProviderCompatibility {
    OpenAiChatCompletions,
    OpenAiResponses,
    AnthropicMessages,
}

/// Per-compatibility settings: path suffix, custom HTTP headers, custom HTTP params.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilitySettings {
    pub path_suffix: Option<String>,
    #[serde(default)]
    pub custom_headers: HashMap<String, String>,
    #[serde(default)]
    pub custom_params: HashMap<String, String>,
}

/// A single API key entry with scheduling metadata.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyEntry {
    pub label: String,
    pub key: String,
    #[serde(default = "default_weight")]
    pub weight: u32,
}

fn default_weight() -> u32 {
    1
}

fn env_or_default(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn parse_env_or_default<T>(key: &str, default: T) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match env::var(key) {
        Ok(value) => value
            .parse::<T>()
            .map_err(|error| format!("invalid value for {key}: {error}")),
        Err(_) => Ok(default),
    }
}
