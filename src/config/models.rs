use std::env;

use std::collections::HashMap;

use bincode_next::{Decode, Encode};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSettings {
    pub gateway_id: String,
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub model_catalog: ModelCatalogConfig,
}

impl RuntimeSettings {
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            gateway_id: env_or_default("LLM_BRIDGE_GATEWAY_ID", "llm-bridge-v1"),
            server: ServerConfig::from_env()?,
            database: DatabaseConfig::from_env(),
            model_catalog: ModelCatalogConfig::from_env()?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
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

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseConfig {
    pub path: String,
}

impl DatabaseConfig {
    fn from_env() -> Self {
        Self {
            path: env_or_default("LLM_BRIDGE_DB_PATH", "./data/llm-bridge"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogConfig {
    pub base_url: String,
    pub output_modalities: String,
    pub refresh_interval_secs: u64,
    pub request_timeout_secs: u64,
    pub strict_bootstrap: bool,
    pub count_consistency_check: bool,
    pub api_key: Option<String>,
}

impl ModelCatalogConfig {
    fn from_env() -> Result<Self, String> {
        Ok(Self {
            base_url: env_or_default(
                "LLM_BRIDGE_CATALOG_BASE_URL",
                "https://openrouter.ai/api/v1",
            ),
            output_modalities: env_or_default("LLM_BRIDGE_CATALOG_OUTPUT_MODALITIES", "text"),
            refresh_interval_secs: parse_env_or_default(
                "LLM_BRIDGE_CATALOG_REFRESH_INTERVAL_SECS",
                900,
            )?,
            request_timeout_secs: parse_env_or_default(
                "LLM_BRIDGE_CATALOG_REQUEST_TIMEOUT_SECS",
                15,
            )?,
            strict_bootstrap: parse_env_or_default("LLM_BRIDGE_CATALOG_STRICT_BOOTSTRAP", true)?,
            count_consistency_check: parse_env_or_default(
                "LLM_BRIDGE_CATALOG_COUNT_CONSISTENCY_CHECK",
                true,
            )?,
            api_key: env::var("LLM_BRIDGE_CATALOG_API_KEY").ok(),
        })
    }
}

/// API compatibility protocol that a provider supports.
/// Each compatibility can be independently enabled with its own settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Encode, Decode, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCompatibility {
    OpenAiChatCompletions,
    OpenAiResponses,
    AnthropicMessages,
}

/// Per-compatibility settings: path suffix, custom HTTP headers, custom HTTP params.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Encode, Decode, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilitySettings {
    /// Appended to the base URL path (e.g. "/v1" or "/openai/deployments/xxx")
    pub path_suffix: Option<String>,
    /// Extra HTTP headers to include in every request
    #[serde(default)]
    pub custom_headers: HashMap<String, String>,
    /// Extra JSON parameters to merge into the request body (stored as raw JSON string)
    #[serde(default)]
    pub custom_params: HashMap<String, String>,
}

/// Configuration for a single compatibility slot on a provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Encode, Decode, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCompatConfig {
    /// Whether this compatibility is enabled for the provider
    pub enabled: bool,
    /// Optional per-compatibility overrides (path suffix, headers, params)
    pub settings: Option<CompatibilitySettings>,
}

/// Legacy enum kept for backward compatibility during migration only.
/// New code should use `ProviderCompatibility` instead.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Encode, Decode, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    OpenAI,
    Anthropic,
    Gemini,
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
