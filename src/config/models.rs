use std::env;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogConfig {
    pub base_url: String,
    pub output_modalities: String,
    pub refresh_interval_secs: u64,
    pub request_timeout_secs: u64,
    pub strict_bootstrap: bool,
    pub count_consistency_check: bool,
    pub api_key: KeyringSecretRef,
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
            api_key: KeyringSecretRef {
                service: env_or_default("LLM_BRIDGE_CATALOG_KEYRING_SERVICE", "llm-bridge"),
                account: env_or_default("LLM_BRIDGE_CATALOG_KEYRING_ACCOUNT", "openrouter-catalog"),
            },
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    OpenAI,
    Anthropic,
    Gemini,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KeyringSecretRef {
    pub service: String,
    pub account: String,
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
