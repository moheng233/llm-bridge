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

pub use catalog::*;
pub use error::StoreError;
pub use providers::*;

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
            for (pid, pdata) in &data {
                if !providers_map.contains_key(pid) {
                    let compat = npm_to_compatibilities(&pdata.npm);
                    providers_map.insert(
                        pid.clone(),
                        ProviderConfig {
                            enabled: false,
                            priority: 0,
                            base_url_override: None,
                            api_keys: Vec::new(),
                            compatibilities: compat,
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
    pub fn list_all_models(&self) -> Vec<AvailableModel> {
        let catalog = self.catalog.read().unwrap();
        let mut seen: HashMap<String, AvailableModel> = HashMap::new();

        for (pid, pdata) in catalog.iter() {
            for (mid, mdata) in &pdata.models {
                if mdata.status.as_deref() == Some("deprecated") {
                    continue;
                }
                let entry = seen.entry(mid.clone()).or_insert_with(|| AvailableModel {
                    model_name: mid.clone(),
                    capabilities: LMModelInfo::from(mdata),
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

            for (mid, mdata) in &pdata.models {
                if mdata.status.as_deref() == Some("deprecated") {
                    continue;
                }
                let entry = available.entry(mid.clone()).or_insert_with(|| AvailableModel {
                    model_name: mid.clone(),
                    capabilities: LMModelInfo::from(mdata),
                    provider_ids: Vec::new(),
                });
                entry.provider_ids.push(pid.clone());
            }
        }

        available.into_values().collect()
    }

    /// Get a single model's info from the catalog.
    pub fn get_model_info(&self, model_name: &str) -> Option<LMModelInfo> {
        let catalog = self.catalog.read().unwrap();
        for pdata in catalog.values() {
            if let Some(mdata) = pdata.models.get(model_name) {
                if mdata.status.as_deref() == Some("deprecated") {
                    continue;
                }
                return Some(LMModelInfo::from(mdata));
            }
        }
        None
    }

    // ── Route Resolution ──

    /// Resolve a canonical model name to a list of provider routes.
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
            let Some(mdata) = pdata.models.get(model_name) else { continue };
            if mdata.status.as_deref() == Some("deprecated") {
                continue;
            }

            // Select a key via weighted round-robin.
            let selected_key = self.select_key(pid, &pconfig.api_keys);

            let base_url = pconfig
                .base_url_override
                .clone()
                .or_else(|| determine_provider_base_url(pdata, mdata));

            let capabilities = LMModelInfo::from(mdata);

            // Create one route per enabled compatibility.
            for (compat, compat_config) in &pconfig.compatibilities {
                if !compat_config.enabled {
                    continue;
                }
                routes.push(ResolvedProviderRoute {
                    model_name: model_name.to_string(),
                    capabilities: capabilities.clone(),
                    provider_name: pid.clone(),
                    provider_model_name: mdata.id.clone(),
                    priority: pconfig.priority,
                    compatibility: compat.clone(),
                    compat_settings: compat_config.settings.clone(),
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
                    compatibilities: config.compatibilities.clone(),
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
    pub compatibilities: HashMap<ProviderCompatibility, ProviderCompatConfig>,
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
