use serde::{Deserialize, Serialize};

use crate::config::openrouter_catalog::ModelCatalogSnapshot;
use crate::routing::models::{
    FallbackChain, ModelRef, ProviderCandidate, RouteGroup, RoutePolicy, TokenPolicy,
};
use crate::types::LMModelInfo;

/// The overall configuration file format
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub server: ServerConfig,
    pub observability: ObservabilityConfig,
    #[serde(default)]
    pub model_catalog: ModelCatalogConfig,
    pub providers: Vec<ProviderConfig>,
    pub route_groups: Vec<RouteGroupConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogConfig {
    pub source_provider_id: String,
    pub base_url: String,
    pub output_modalities: String,
    pub refresh_interval_secs: u64,
    pub request_timeout_secs: u64,
    pub strict_bootstrap: bool,
    pub count_consistency_check: bool,
}

impl Default for ModelCatalogConfig {
    fn default() -> Self {
        Self {
            source_provider_id: "openrouter".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            output_modalities: "text".to_string(),
            refresh_interval_secs: 900,
            request_timeout_secs: 15,
            strict_bootstrap: true,
            count_consistency_check: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub auth_token: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3000,
            auth_token: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservabilityConfig {
    pub enabled: bool,
    pub host: Option<String>,
    pub public_key: Option<String>,
    pub secret_key: Option<String>,
    pub sample_rate: Option<f64>,
    pub flush_timeout_ms: Option<u64>,
    pub redaction_rules: Vec<String>,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: None,
            public_key: None,
            secret_key: None,
            sample_rate: Some(1.0),
            flush_timeout_ms: Some(2000),
            redaction_rules: vec![],
        }
    }
}

/// Provider Type Enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    OpenAI,
    Anthropic,
    Gemini,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub id: String,
    pub provider_type: ProviderType,
    pub api_key: String,
    pub base_url: Option<String>,
    /// Provider 声明它支持哪些标准模型，并定义各模型在 provider 侧可用的真实名称。
    #[serde(default)]
    pub model_bindings: Vec<ProviderModelBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelBinding {
    pub canonical_model: String,
    pub aliases: Vec<String>,
    #[serde(default)]
    pub priority: u32,
    #[serde(default)]
    pub capabilities_override: Option<ModelCapabilitiesOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapabilitiesOverride {
    pub max_input_tokens: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub tool_calling: Option<bool>,
    pub vision: Option<bool>,
    pub thinking: Option<bool>,
    pub adaptive_thinking: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteGroupConfig {
    pub id: String,
    pub name: String,
    pub max_input_tokens: Option<u32>,
    pub max_output_tokens: Option<u32>,
    /// Ordered list of canonical model names.
    pub models: Vec<String>,
    #[serde(default)]
    pub token_policy: Option<TokenPolicy>,
}

impl AppConfig {
    pub fn to_route_groups_with_catalog(
        &self,
        model_catalog: &ModelCatalogSnapshot,
    ) -> Result<Vec<RouteGroup>, String> {
        let mut groups = Vec::new();

        for config_group in &self.route_groups {
            let mut chain = Vec::new();
            for canonical_model in &config_group.models {
                if canonical_model.contains('/') {
                    return Err(format!(
                        "Route group {} contains legacy model reference {}. Use canonical model names instead",
                        config_group.id, canonical_model
                    ));
                }

                let capabilities = model_catalog
                    .get(canonical_model)
                    .ok_or_else(|| {
                        format!(
                            "Canonical model {} not found in catalog for route group {}",
                            canonical_model, config_group.id
                        )
                    })?
                    .clone();

                let mut provider_candidates =
                    collect_provider_candidates(&self.providers, canonical_model, &capabilities)?;

                provider_candidates.sort_by_key(|candidate| candidate.priority);

                chain.push(ModelRef {
                    canonical_model: canonical_model.clone(),
                    capabilities,
                    provider_candidates,
                });
            }

            if chain.is_empty() {
                return Err(format!("Route group {} has no models", config_group.id));
            }

            groups.push(RouteGroup {
                id: config_group.id.clone(),
                name: config_group.name.clone(),
                max_input_tokens: config_group.max_input_tokens,
                max_output_tokens: config_group.max_output_tokens,
                route_policy: RoutePolicy {
                    fallback_chain: FallbackChain { models: chain },
                    token_policy: config_group
                        .token_policy
                        .clone()
                        .unwrap_or(TokenPolicy::Min),
                },
            });
        }

        Ok(groups)
    }
}

fn collect_provider_candidates(
    providers: &[ProviderConfig],
    canonical_model: &str,
    default_capabilities: &LMModelInfo,
) -> Result<Vec<ProviderCandidate>, String> {
    let mut candidates = Vec::new();

    for provider in providers {
        for binding in &provider.model_bindings {
            if binding.canonical_model != canonical_model {
                continue;
            }

            if binding.aliases.is_empty() {
                return Err(format!(
                    "Provider {} binding for {} must declare at least one alias",
                    provider.id, canonical_model
                ));
            }

            let merged_capabilities = apply_capability_override(
                default_capabilities,
                binding.capabilities_override.as_ref(),
            );

            for alias in &binding.aliases {
                candidates.push(ProviderCandidate {
                    provider_id: provider.id.clone(),
                    resolved_model_name: alias.clone(),
                    priority: binding.priority,
                    capabilities: merged_capabilities.clone(),
                });
            }
        }
    }

    if candidates.is_empty() {
        return Err(format!(
            "No provider bindings found for canonical model {}",
            canonical_model
        ));
    }

    Ok(candidates)
}

fn apply_capability_override(
    base: &LMModelInfo,
    capabilities_override: Option<&ModelCapabilitiesOverride>,
) -> LMModelInfo {
    let Some(override_config) = capabilities_override else {
        return base.clone();
    };

    LMModelInfo {
        name: base.name.clone(),
        max_input_tokens: override_config
            .max_input_tokens
            .unwrap_or(base.max_input_tokens),
        max_output_tokens: override_config
            .max_output_tokens
            .unwrap_or(base.max_output_tokens),
        tool_calling: override_config.tool_calling.unwrap_or(base.tool_calling),
        vision: override_config.vision.unwrap_or(base.vision),
        thinking: override_config.thinking.or(base.thinking),
        adaptive_thinking: override_config.adaptive_thinking.or(base.adaptive_thinking),
        edit_tools: base.edit_tools,
    }
}
