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
    pub model_catalog: ModelCatalogConfig,
}

impl RuntimeSettings {
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            gateway_id: env_or_default("LLM_BRIDGE_GATEWAY_ID", "llm-bridge-v1"),
            server: ServerConfig::from_env()?,
            store_path: env_or_default("LLM_BRIDGE_STORE_PATH", "./data/llm-bridge"),
            model_catalog: ModelCatalogConfig::from_env()?,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogConfig {
    pub base_url: String,
    pub refresh_interval_secs: u64,
    pub request_timeout_secs: u64,
    pub strict_bootstrap: bool,
}

impl ModelCatalogConfig {
    fn from_env() -> Result<Self, String> {
        Ok(Self {
            base_url: env_or_default(
                "LLM_BRIDGE_CATALOG_BASE_URL",
                "https://models.dev",
            ),
            refresh_interval_secs: parse_env_or_default(
                "LLM_BRIDGE_CATALOG_REFRESH_INTERVAL_SECS",
                900,
            )?,
            request_timeout_secs: parse_env_or_default(
                "LLM_BRIDGE_CATALOG_REQUEST_TIMEOUT_SECS",
                30,
            )?,
            strict_bootstrap: parse_env_or_default("LLM_BRIDGE_CATALOG_STRICT_BOOTSTRAP", true)?,
        })
    }
}

/// API compatibility protocol that a provider supports.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, TS, toasty::Embed)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
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

/// Configuration for a single compatibility slot on a provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCompatConfig {
    pub enabled: bool,
    pub settings: Option<CompatibilitySettings>,
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

/// User configuration for a single provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub priority: u32,
    pub base_url_override: Option<String>,
    #[serde(default)]
    pub api_keys: Vec<ApiKeyEntry>,
    /// Custom HTTP headers/params/path-suffix (applied to all compatibilities).
    pub compat_settings: Option<CompatibilitySettings>,
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
