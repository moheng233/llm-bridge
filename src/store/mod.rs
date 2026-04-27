use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use crate::config::models::{
    ApiKeyEntry, CompatibilitySettings, ProviderCompatConfig, ProviderCompatibility, ProviderConfig,
};
use crate::models_dev::{CatalogCache, ModelsDevModel, ModelsDevProvider, ModelsDevRoot};
use crate::types::LMModelInfo;

mod catalog;
mod error;
mod providers;

pub use error::StoreError;

/// Central data store — holds models.dev catalog and user provider configuration.
pub struct Store {
    /// Path to the data directory (stores catalog_cache.json, providers.json).
    path: PathBuf,
    /// Full models.dev snapshot (from catalog_cache.json).
    catalog: RwLock<Arc<ModelsDevRoot>>,
    /// User-managed provider configurations (from providers.json).
    providers: RwLock<HashMap<String, ProviderConfig>>,
    /// Per-key weighted round-robin counters (key = `{provider_id}/{key_label}`).
    key_usage: RwLock<HashMap<String, AtomicU64>>,
    /// Metadata from the last catalog refresh.
    metadata: RwLock<StoreMetadata>,
}

#[derive(Debug, Clone)]
pub struct StoreMetadata {
    pub fetched_at: i64,
    pub etag: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AvailableModel {
    pub model_name: String,
    pub capabilities: LMModelInfo,
    /// Provider IDs that have this model (for admin display).
    pub provider_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedProviderRoute {
    pub model_name: String,
    pub capabilities: LMModelInfo,
    pub provider_name: String,
    pub provider_model_name: String,
    pub priority: u32,
    pub compatibility: ProviderCompatibility,
    pub compat_settings: Option<CompatibilitySettings>,
    pub base_url: Option<String>,
    pub api_key: String,
    /// The label of the selected API key.
    pub key_label: String,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&path)?;

        // Load providers first (small, critical for user config).
        let providers = providers::load_providers(&path)?;
        // Load catalog cache (may be empty on first run).
        let (catalog, metadata) = catalog::load_catalog_cache(&path)?;

        Ok(Self {
            path,
            catalog: RwLock::new(Arc::new(catalog)),
            providers: RwLock::new(providers),
            key_usage: RwLock::new(HashMap::new()),
            metadata: RwLock::new(metadata),
        })
    }

    // ── Catalog ──

    pub fn catalog_model_count(&self) -> usize {
        let mut count = 0;
        for provider in self.catalog.read().unwrap().values() {
            count += provider.models.len();
        }
        count
    }

    /// Replace the entire catalog and auto-register new providers.
    pub fn replace_catalog(&self, data: ModelsDevRoot, metadata: StoreMetadata) -> Result<(), StoreError> {
        // Write catalog cache to disk.
        catalog::save_catalog_cache(
            &self.path,
            &CatalogCache {
                fetched_at: metadata.fetched_at,
                etag: metadata.etag.clone(),
                data: data.clone(),
            },
        )?;

        // Auto-register newly discovered providers.
        {
            let mut providers_map = self.providers.read().unwrap().clone();
            for (pid, _pdata) in &data {
                if !providers_map.contains_key(pid) {
                    providers_map.insert(
                        pid.clone(),
                        ProviderConfig {
                            enabled: false,
                            priority: 0,
                            base_url_override: None,
                            api_keys: Vec::new(),
                            compat_settings: None,
                        },
                    );
                }
            }
            providers::save_providers(&self.path, &providers_map)?;
            *self.providers.write().unwrap() = providers_map;
        }

        *self.catalog.write().unwrap() = Arc::new(data);
        *self.metadata.write().unwrap() = metadata;

        Ok(())
    }

    pub fn get_metadata(&self) -> StoreMetadata {
        self.metadata.read().unwrap().clone()
    }

    // ── Models ──

    /// List all models known to the catalog (regardless of provider availability).
    /// Models are grouped by their human-readable `name` field, merging providers
    /// that offer the same named model.
    pub fn list_all_models(&self) -> Vec<AvailableModel> {
        let catalog = self.catalog.read().unwrap();
        let mut seen: HashMap<String, AvailableModel> = HashMap::new();

        for (pid, pdata) in catalog.iter() {
            for (_mid, mdata) in &pdata.models {
                if mdata.status.as_deref() == Some("deprecated") {
                    continue;
                }
                let model_name = mdata.name.clone();
                let entry = seen.entry(model_name.clone()).or_insert_with(|| AvailableModel {
                    model_name,
                    capabilities: model_info_from_models_dev(mdata),
                    provider_ids: Vec::new(),
                });
                entry.provider_ids.push(pid.clone());
            }
        }

        seen.into_values().collect()
    }

    /// List models that have at least one enabled provider with a valid API key.
    pub fn list_available_models(&self) -> Vec<AvailableModel> {
        let catalog = self.catalog.read().unwrap();
        let providers_map = self.providers.read().unwrap();
        let mut available: HashMap<String, AvailableModel> = HashMap::new();

        for (pid, pdata) in catalog.iter() {
            let Some(pconfig) = providers_map.get(pid) else { continue };
            if !pconfig.enabled || pconfig.api_keys.is_empty() {
                continue;
            }

            for (_mid, mdata) in &pdata.models {
                if mdata.status.as_deref() == Some("deprecated") {
                    continue;
                }
                let model_name = mdata.name.clone();
                let entry = available.entry(model_name.clone()).or_insert_with(|| AvailableModel {
                    model_name,
                    capabilities: model_info_from_models_dev(mdata),
                    provider_ids: Vec::new(),
                });
                entry.provider_ids.push(pid.clone());
            }
        }

        available.into_values().collect()
    }

    /// Get a single model's info from the catalog (looked up by human-readable name).
    pub fn get_model_info(&self, model_name: &str) -> Option<LMModelInfo> {
        let catalog = self.catalog.read().unwrap();
        for pdata in catalog.values() {
            if let Some(mdata) = pdata.models.values().find(|m| m.name == model_name) {
                if mdata.status.as_deref() == Some("deprecated") {
                    continue;
                }
                return Some(model_info_from_models_dev(mdata));
            }
        }
        None
    }

    // ── Route Resolution ──

    /// Resolve a canonical model name to a list of provider routes.
    /// The model_name is matched against the human-readable `name` field of models.
    /// Returns routes sorted by provider priority, with a selected API key for each.
    pub fn resolve_model(&self, model_name: &str) -> Vec<ResolvedProviderRoute> {
        let catalog = self.catalog.read().unwrap();
        let providers_map = self.providers.read().unwrap();
        let mut routes = Vec::new();

        for (pid, pdata) in catalog.iter() {
            let Some(pconfig) = providers_map.get(pid) else { continue };
            if !pconfig.enabled || pconfig.api_keys.is_empty() {
                continue;
            }
            let Some(mdata) = pdata.models.values().find(|m| m.name == model_name) else {
                continue;
            };
            if mdata.status.as_deref() == Some("deprecated") {
                continue;
            }

            // Select a key via weighted round-robin.
            let selected_key = self.select_key(pid, &pconfig.api_keys);

            let base_url = pconfig
                .base_url_override
                .clone()
                .or_else(|| determine_provider_base_url(pdata, mdata));

            let capabilities = model_info_from_models_dev(mdata);

            // Create one route per auto-derived compatibility.
            let derived_compat = npm_to_compatibilities(&pdata.npm);
            for compat in derived_compat.keys() {
                routes.push(ResolvedProviderRoute {
                    model_name: model_name.to_string(),
                    capabilities: capabilities.clone(),
                    provider_name: pid.clone(),
                    provider_model_name: mdata.id.clone(),
                    priority: pconfig.priority,
                    compatibility: compat.clone(),
                    compat_settings: pconfig.compat_settings.clone(),
                    base_url: base_url.clone(),
                    api_key: selected_key.key.clone(),
                    key_label: selected_key.label.clone(),
                });
            }
        }

        routes.sort_by_key(|r| r.priority);
        routes
    }

    /// Weighted round-robin key selection.
    fn select_key<'a>(&self, provider_id: &str, keys: &'a [ApiKeyEntry]) -> &'a ApiKeyEntry {
        if keys.len() == 1 {
            return &keys[0];
        }

        let total_weight: u64 = keys.iter().map(|k| k.weight as u64).sum();
        if total_weight == 0 {
            return &keys[0];
        }

        let mut usage = self.key_usage.write().unwrap();
        let counter_key = format!("{provider_id}");
        let counter = usage
            .entry(counter_key.clone())
            .or_insert_with(|| AtomicU64::new(0));

        let current = counter.fetch_add(1, Ordering::Relaxed) % total_weight;

        let mut cumulative = 0u64;
        for k in keys {
            cumulative += k.weight as u64;
            if current < cumulative {
                return k;
            }
        }

        &keys[keys.len() - 1]
    }

    // ── Providers ──

    pub fn list_providers(&self) -> Vec<ProviderInfo> {
        let catalog = self.catalog.read().unwrap();
        let providers_map = self.providers.read().unwrap();

        providers_map
            .iter()
            .map(|(pid, config)| {
                let catalog_info = catalog.get(pid);
                let derived_compat: HashMap<ProviderCompatibility, ProviderCompatConfig> = catalog_info
                    .map(|ci| {
                        npm_to_compatibilities(&ci.npm)
                            .into_iter()
                            .map(|(k, _v)| (k, ProviderCompatConfig { enabled: true, settings: None }))
                            .collect()
                    })
                    .unwrap_or_default();
                ProviderInfo {
                    id: pid.clone(),
                    name: catalog_info
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| pid.clone()),
                    enabled: config.enabled,
                    priority: config.priority,
                    api_keys: config.api_keys.iter().map(|k| ApiKeyDisplay {
                        label: k.label.clone(),
                        weight: k.weight,
                        masked_key: mask_key(&k.key),
                    }).collect(),
                    compatibilities: derived_compat,
                    compat_settings: config.compat_settings.clone(),
                    base_url_override: config.base_url_override.clone(),
                    model_count: catalog_info.map(|p| p.models.len()).unwrap_or(0),
                }
            })
            .collect()
    }

    pub fn get_provider_config(&self, provider_id: &str) -> Option<ProviderConfig> {
        self.providers.read().unwrap().get(provider_id).cloned()
    }

    pub fn upsert_provider(&self, provider_id: &str, config: ProviderConfig) -> Result<(), StoreError> {
        let mut providers_map = self.providers.read().unwrap().clone();
        if let Some(existing) = providers_map.get_mut(provider_id) {
            *existing = config;
        } else {
            providers_map.insert(provider_id.to_string(), config);
        }
        providers::save_providers(&self.path, &providers_map)?;
        *self.providers.write().unwrap() = providers_map;
        Ok(())
    }

    pub fn delete_provider(&self, provider_id: &str) -> Result<bool, StoreError> {
        let mut providers_map = self.providers.read().unwrap().clone();
        let existed = providers_map.remove(provider_id).is_some();
        if existed {
            providers::save_providers(&self.path, &providers_map)?;
            *self.providers.write().unwrap() = providers_map;
        }
        Ok(existed)
    }
}

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub priority: u32,
    pub api_keys: Vec<ApiKeyDisplay>,
    /// Compatibilities auto-derived from models.dev catalog (read-only display).
    pub compatibilities: HashMap<ProviderCompatibility, ProviderCompatConfig>,
    /// User-editable custom HTTP settings.
    pub compat_settings: Option<CompatibilitySettings>,
    pub base_url_override: Option<String>,
    pub model_count: usize,
}

#[derive(Debug, Clone)]
pub struct ApiKeyDisplay {
    pub label: String,
    pub weight: u32,
    pub masked_key: String,
}

fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        return "***".to_string();
    }
    format!("{}...{}", &key[..4], &key[key.len() - 4..])
}

/// Build an `LMModelInfo` from a models.dev model entry.
pub fn model_info_from_models_dev(m: &ModelsDevModel) -> LMModelInfo {
    let default_context = 4096u32;
    let limit = m.limit.as_ref();
    LMModelInfo {
        name: m.name.clone(),
        max_input_tokens: limit.map(|l| l.context).unwrap_or(default_context),
        max_output_tokens: limit.map(|l| l.output).unwrap_or(default_context),
        tool_calling: m.tool_call,
        vision: m
            .modalities
            .as_ref()
            .map(|mods| mods.input.iter().any(|s| s == "image"))
            .unwrap_or(false),
        thinking: if m.reasoning { Some(true) } else { None },
        adaptive_thinking: m
            .interleaved
            .as_ref()
            .map(|il| il.is_active())
            .or(Some(false))
            .filter(|&b| b),
        edit_tools: crate::types::EndpointEditToolName::empty(),
    }
}

/// Determine the effective base URL for a provider.
/// Priority: user override > per-model provider.api > provider-level api.
fn determine_provider_base_url(
    pdata: &ModelsDevProvider,
    mdata: &ModelsDevModel,
) -> Option<String> {
    // Per-model provider override (from extends/wrapper models).
    if let Some(ref mp) = mdata.provider {
        if let Some(ref api) = mp.api {
            return Some(api.clone());
        }
    }
    // Provider-level api field.
    pdata.api.clone()
}

/// Map an AI SDK npm package name to a set of compatibilities.
/// Returns a map where each compatibility is disabled by default.
fn npm_to_compatibilities(
    npm: &str,
) -> HashMap<ProviderCompatibility, ProviderCompatConfig> {
    let mut map = HashMap::new();
    let default = ProviderCompatConfig {
        enabled: false,
        settings: None,
    };

    if npm.contains("@ai-sdk/openai") || npm.contains("@ai-sdk/openai-compatible") {
        map.insert(ProviderCompatibility::OpenAiChatCompletions, default.clone());
    }
    if npm.contains("@ai-sdk/anthropic") {
        map.insert(ProviderCompatibility::AnthropicMessages, default.clone());
    }

    // If no recognized npm package, default to OpenAI-compatible.
    if map.is_empty() {
        map.insert(ProviderCompatibility::OpenAiChatCompletions, default.clone());
    }

    map
}
