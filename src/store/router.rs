//! 路由解析（Phase 3.4）。
//!
//! 从 SQLite 的 `provider_models` + `providers` 表中查询模型路由，
//! 替代旧的基于 models.dev catalog + providers.json 的内存解析。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use crate::config::models::{ApiKeyEntry, CompatibilitySettings, ProviderCompatibility};
use crate::db::{self, models::{Provider as DbProvider, ProviderModel as DbProviderModel}};
use crate::types::LMModelInfo;

/// 解析后的提供者路由（传递给 ProviderActor）。
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
    /// 选中 API Key 的 label
    pub key_label: String,
}

/// 可用模型摘要（用于 /v1/models 或 Admin API 列表）。
#[derive(Debug, Clone)]
pub struct AvailableModel {
    pub model_name: String,
    pub capabilities: LMModelInfo,
    pub provider_ids: Vec<String>,
    /// 模型描述
    pub description: Option<String>,
    /// 定价
    pub input_price_per_1m: Option<f64>,
    pub output_price_per_1m: Option<f64>,
    pub cache_read_price_per_1m: Option<f64>,
}

/// 加权轮询 Key 选择器（线程安全）。
pub struct KeySelector {
    counters: Mutex<HashMap<String, AtomicU64>>,
}

impl KeySelector {
    pub fn new() -> Self {
        Self {
            counters: Mutex::new(HashMap::new()),
        }
    }

    /// 从 API Key 列表中按加权轮询选出一个。
    pub fn select_key(&self, provider_id: &str, keys: &[ApiKeyEntry]) -> &ApiKeyEntry {
        if keys.len() == 1 {
            return &keys[0];
        }

        let total_weight: u64 = keys.iter().map(|k| k.weight as u64).sum();
        if total_weight == 0 {
            return &keys[0];
        }

        let mut counters = self.counters.lock().unwrap();
        let counter_key = provider_id.to_string();
        let counter = counters
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
}

/// 解析模型名 → 路由列表。
///
/// 从数据库查询匹配 `model_name` 的 `ProviderModel`，关联 `Provider`，
/// 过滤启用的提供者，按优先级排序，选择 API Key。
pub async fn resolve_model(
    db: &db::Db,
    key_selector: &KeySelector,
    model_name: &str,
) -> Result<Vec<ResolvedProviderRoute>, String> {
    // 查询匹配的 ProviderModel
    let provider_models = DbProviderModel::filter(
        DbProviderModel::fields().model_name().eq(model_name)
            .and(DbProviderModel::fields().enabled().eq(true))
    )
    .exec(&mut db.clone())
    .await
    .map_err(|e| e.to_string())?;

    let mut routes = Vec::new();

    for pm in provider_models {
        // 关联的 Provider
        let provider = match DbProvider::get_by_id(&mut db.clone(), &pm.provider_row_id).await {
            Ok(p) => p,
            Err(_) => continue,
        };

        if !provider.enabled {
            continue;
        }

        // 解析 API Keys
        let api_keys: Vec<ApiKeyEntry> =
            serde_json::from_str(&provider.api_keys).unwrap_or_default();
        if api_keys.is_empty() {
            continue;
        }

        let selected_key = key_selector.select_key(&provider.provider_id, &api_keys);

        let compatibility: ProviderCompatibility =
            serde_json::from_str(&format!("\"{}\"", pm.compatibility)).unwrap_or(
                ProviderCompatibility::OpenAiChatCompletions,
            );

        let compat_settings: Option<CompatibilitySettings> = provider
            .compat_settings
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok());

        let capabilities = LMModelInfo {
            name: pm.model_name.clone(),
            max_input_tokens: pm.max_input_tokens as u32,
            max_output_tokens: pm.max_output_tokens as u32,
            tool_calling: pm.tool_calling,
            vision: pm.vision,
            thinking: if pm.thinking { Some(true) } else { None },
            adaptive_thinking: if pm.adaptive_thinking { Some(true) } else { None },
            edit_tools: crate::types::EndpointEditToolName::empty(),
        };

        routes.push(ResolvedProviderRoute {
            model_name: model_name.to_string(),
            capabilities,
            provider_name: provider.provider_id.clone(),
            provider_model_name: pm.provider_model_id.clone(),
            priority: provider.priority as u32,
            compatibility,
            compat_settings,
            base_url: provider.base_url.clone(),
            api_key: selected_key.key.clone(),
            key_label: selected_key.label.clone(),
        });
    }

    routes.sort_by_key(|r| r.priority);
    Ok(routes)
}

/// 列出所有已启用提供者的可用模型。
pub async fn list_available_models(db: &db::Db) -> Result<Vec<AvailableModel>, String> {
    // 加载所有启用的 provider_models，join providers
    let all_models = DbProviderModel::filter(
        DbProviderModel::fields().enabled().eq(true)
    )
    .exec(&mut db.clone())
    .await
    .map_err(|e| e.to_string())?;

    let mut result: HashMap<String, AvailableModel> = HashMap::new();

    for pm in all_models {
        let provider = match DbProvider::get_by_id(&mut db.clone(), &pm.provider_row_id).await {
            Ok(p) => p,
            Err(_) => continue,
        };

        if !provider.enabled {
            continue;
        }

        let entry = result
            .entry(pm.model_name.clone())
            .or_insert_with(|| AvailableModel {
                model_name: pm.model_name.clone(),
                capabilities: LMModelInfo {
                    name: pm.model_name.clone(),
                    max_input_tokens: pm.max_input_tokens as u32,
                    max_output_tokens: pm.max_output_tokens as u32,
                    tool_calling: pm.tool_calling,
                    vision: pm.vision,
                    thinking: if pm.thinking { Some(true) } else { None },
                    adaptive_thinking: if pm.adaptive_thinking { Some(true) } else { None },
                    edit_tools: crate::types::EndpointEditToolName::empty(),
                },
                provider_ids: Vec::new(),
                description: pm.description.clone(),
                input_price_per_1m: pm.input_price_per_1m,
                output_price_per_1m: pm.output_price_per_1m,
                cache_read_price_per_1m: pm.cache_read_price_per_1m,
            });

        if !entry.provider_ids.contains(&provider.provider_id) {
            entry.provider_ids.push(provider.provider_id.clone());
        }
    }

    Ok(result.into_values().collect())
}

/// 列出所有模型（包括未启用的）。
pub async fn list_all_models(db: &db::Db) -> Result<Vec<AvailableModel>, String> {
    let all_models = DbProviderModel::all()
        .exec(&mut db.clone())
        .await
        .map_err(|e| e.to_string())?;

    let mut result: HashMap<String, AvailableModel> = HashMap::new();

    for pm in all_models {
        let provider = match DbProvider::get_by_id(&mut db.clone(), &pm.provider_row_id).await {
            Ok(p) => p,
            Err(_) => continue,
        };

        let entry = result
            .entry(pm.model_name.clone())
            .or_insert_with(|| AvailableModel {
                model_name: pm.model_name.clone(),
                capabilities: LMModelInfo {
                    name: pm.model_name.clone(),
                    max_input_tokens: pm.max_input_tokens as u32,
                    max_output_tokens: pm.max_output_tokens as u32,
                    tool_calling: pm.tool_calling,
                    vision: pm.vision,
                    thinking: if pm.thinking { Some(true) } else { None },
                    adaptive_thinking: if pm.adaptive_thinking { Some(true) } else { None },
                    edit_tools: crate::types::EndpointEditToolName::empty(),
                },
                provider_ids: Vec::new(),
                description: pm.description.clone(),
                input_price_per_1m: pm.input_price_per_1m,
                output_price_per_1m: pm.output_price_per_1m,
                cache_read_price_per_1m: pm.cache_read_price_per_1m,
            });

        if !entry.provider_ids.contains(&provider.provider_id) {
            entry.provider_ids.push(provider.provider_id.clone());
        }
    }

    Ok(result.into_values().collect())
}
