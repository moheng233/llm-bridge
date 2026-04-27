// models.dev native types — directly mirrors https://models.dev/api.json structure.
// Store holds a parsed ModelsDevRoot as the primary data model.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Top-level type: api.json is a flat map of provider_id → provider.
pub type ModelsDevRoot = HashMap<String, ModelsDevProvider>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsDevProvider {
    pub id: String,
    pub name: String,
    /// AI SDK npm package name (e.g. `@ai-sdk/openai`, `@ai-sdk/openai-compatible`).
    pub npm: String,
    /// Environment variable names recommended for auth.
    #[serde(default)]
    pub env: Vec<String>,
    /// Base URL for the provider's API endpoint.
    pub api: Option<String>,
    /// Link to the provider's documentation.
    pub doc: String,
    /// Models offered by this provider, keyed by model id.
    #[serde(default)]
    pub models: HashMap<String, ModelsDevModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsDevModel {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub attachment: bool,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub tool_call: bool,
    pub structured_output: Option<bool>,
    pub temperature: Option<bool>,
    pub knowledge: Option<String>,
    pub release_date: Option<String>,
    pub last_updated: Option<String>,
    #[serde(default)]
    pub open_weights: bool,
    pub cost: Option<ModelsDevCost>,
    pub limit: Option<ModelsDevLimit>,
    pub modalities: Option<ModelsDevModalities>,
    pub interleaved: Option<ModelsDevInterleavedOrBool>,
    /// `"alpha" | "beta" | "deprecated"` or absent.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub family: Option<String>,
    /// Per-model provider overrides (e.g. a specific npm/api for this model within a wrapper).
    #[serde(default)]
    pub provider: Option<ModelsDevModelProvider>,
    /// `experimental.modes` settings.
    #[serde(default)]
    pub experimental: HashMap<String, Value>,
    /// Catch-all for any other unknown fields.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsDevModelProvider {
    pub npm: Option<String>,
    pub api: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsDevCost {
    pub input: f64,
    pub output: f64,
    pub reasoning: Option<f64>,
    pub cache_read: Option<f64>,
    pub cache_write: Option<f64>,
    pub input_audio: Option<f64>,
    pub output_audio: Option<f64>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsDevLimit {
    pub context: u32,
    pub input: Option<u32>,
    pub output: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsDevModalities {
    #[serde(default)]
    pub input: Vec<String>,
    #[serde(default)]
    pub output: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsDevInterleaved {
    /// `"reasoning_content"` or `"reasoning_details"`.
    pub field: String,
}

/// `interleaved` can be a boolean `true` or an object `{field: "..."}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModelsDevInterleavedOrBool {
    Bool(bool),
    Object(ModelsDevInterleaved),
}

impl ModelsDevInterleavedOrBool {
    pub fn is_active(&self) -> bool {
        match self {
            Self::Bool(b) => *b,
            Self::Object(_) => true,
        }
    }
}

/// Cached snapshot of models.dev data with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogCache {
    /// Unix timestamp when the cache was fetched.
    pub fetched_at: i64,
    /// The raw api.json content.
    pub data: ModelsDevRoot,
    /// Optional ETag from the last fetch for conditional requests.
    pub etag: Option<String>,
}
