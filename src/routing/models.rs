use serde::{Deserialize, Serialize};

use crate::types::LMModelInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCandidate {
    pub provider_id: String,
    pub resolved_model_name: String,
    #[serde(default)]
    pub priority: u32,
    pub capabilities: LMModelInfo,
}

/// Represents a canonical model entry and its provider candidates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRef {
    pub canonical_model: String,
    /// Stable capabilities exposed to clients.
    pub capabilities: LMModelInfo,
    pub provider_candidates: Vec<ProviderCandidate>,
}

/// Fallback strategy for a route group
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FallbackChain {
    /// Ordered list of model references. First is primary, subsequent are fallbacks.
    pub models: Vec<ModelRef>,
}

/// Token policy determines how `max_input_tokens` and `max_output_tokens` are decided
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum TokenPolicy {
    /// Take the minimum of the Group's limit and the Model's limit
    Min,
    /// Override with a specific group-level limit
    Override(u32),
    /// Just use the provider's reported limit
    ProviderDefault,
}

/// Defines how requests are routed within a group
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutePolicy {
    pub fallback_chain: FallbackChain,
    #[serde(default = "default_token_policy")]
    pub token_policy: TokenPolicy,
}

fn default_token_policy() -> TokenPolicy {
    TokenPolicy::Min
}

/// The core route group that is exposed to the VS Code client as a selectable "model"
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteGroup {
    pub id: String,
    pub name: String,
    /// Optional overall token limit for the group. Handled by TokenPolicy.
    pub max_input_tokens: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub route_policy: RoutePolicy,
}

impl RouteGroup {
    /// Determine the effective token limit based on the token policy
    pub fn effective_input_tokens(&self, active_model: &ModelRef) -> u32 {
        match self.route_policy.token_policy {
            TokenPolicy::Min => {
                let group_limit = self.max_input_tokens.unwrap_or(u32::MAX);
                let model_limit = active_model.capabilities.max_input_tokens;
                std::cmp::min(group_limit, model_limit)
            }
            TokenPolicy::Override(limit) => limit,
            TokenPolicy::ProviderDefault => active_model.capabilities.max_input_tokens,
        }
    }
}
