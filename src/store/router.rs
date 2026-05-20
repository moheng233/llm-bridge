//! 路由解析（Phase 3.4 + Phase 4 重构）。
//!
//! 基于 `models` + `model_providers` + `providers` 三表 JOIN 查询。
//!
//! - `resolve_model`: 查找模型 → 关联提供者 → 构建路由列表（按 priority 排序 fallback）
//! - `list_available_models`: 列出所有已启用模型的可用提供者
//! - `list_all_models`: 列出所有模型（含未启用的）

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use crate::config::models::{ApiKeyEntry, CompatibilitySettings, ProviderCompatibility};
use crate::db::{self, models::{LLMModel as DbLLMModel, ModelProvider as DbModelProvider, Provider as DbProvider}};
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

/// 模型的一个提供者信息（用于 API 返回）。
#[derive(Debug, Clone)]
pub struct ModelProviderInfo {
    pub provider_id: String,
    pub provider_display_name: String,
    pub provider_model_id: String,
    pub compatibility: ProviderCompatibility,
    /// 能力覆盖（nullable = 使用模型标称值）
    pub max_input_tokens: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub tool_calling: Option<bool>,
    pub vision: Option<bool>,
    pub thinking: Option<bool>,
    pub adaptive_thinking: Option<bool>,
    /// 提供者特定定价
    pub input_price_per_1m: Option<f64>,
    pub output_price_per_1m: Option<f64>,
    pub cache_read_price_per_1m: Option<f64>,
    pub enabled: bool,
    pub priority: i64,
}

/// 可用模型摘要（用于 /v1/models 或 Admin API 列表）。
#[derive(Debug, Clone)]
pub struct AvailableModel {
    pub model_name: String,
    pub display_name: String,
    pub description: Option<String>,
    /// 模型的标称能力
    pub nominal_capabilities: LMModelInfo,
    /// 提供此模型的所有提供者（含各自能力覆盖和定价）
    pub providers: Vec<ModelProviderInfo>,
}

/// 加权轮询 Key 选择器（线程安全）。
pub struct KeySelector {
    counters: Mutex<HashMap<String, AtomicU64>>,
}

impl Default for KeySelector {
    fn default() -> Self {
        Self::new()
    }
}

impl KeySelector {
    pub fn new() -> Self {
        Self {
            counters: Mutex::new(HashMap::new()),
        }
    }

    /// 从 API Key 列表中按加权轮询选出一个。
    pub fn select_key<'a>(&self, provider_id: &str, keys: &'a [ApiKeyEntry]) -> &'a ApiKeyEntry {
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

/// 解析模型名 → 路由列表（按优先级排序，用于 fallback）。
///
/// 1. 从 `models` 表查找 model_name
/// 2. 从 `model_providers` 表查找关联（启用 + 按 priority 排序）
/// 3. 从 `providers` 表获取提供者配置和 API Keys
pub async fn resolve_model(
    db: &db::Db,
    key_selector: &KeySelector,
    model_name: &str,
) -> Result<Vec<ResolvedProviderRoute>, String> {
    // 查找模型
    let models = DbLLMModel::filter(
        DbLLMModel::fields().model_name().eq(model_name)
    )
    .exec(&mut db.clone())
    .await
    .map_err(|e| e.to_string())?;

    let model = match models.into_iter().next() {
        Some(m) => m,
        None => return Ok(Vec::new()),
    };

    // 查找启用的 ModelProvider 关联，按 priority 排序
    let model_providers = DbModelProvider::filter(
        DbModelProvider::fields().model_id().eq(model.id)
            .and(DbModelProvider::fields().enabled().eq(true))
    )
    .exec(&mut db.clone())
    .await
    .map_err(|e| e.to_string())?;

    // 按 priority 排序
    let mut sorted: Vec<_> = model_providers;
    sorted.sort_by_key(|mp| mp.priority);

    let mut routes = Vec::new();

    for mp in sorted {
        // 关联的 Provider
        let provider = match DbProvider::get_by_id(&mut db.clone(), &mp.provider_id).await {
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

        let compat_settings: Option<CompatibilitySettings> = provider
            .compat_settings
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok());

        // 构建能力：优先使用 ModelProvider 覆盖，fallback 到 Model 标称值
        let capabilities = LMModelInfo {
            name: model.model_name.clone(),
            max_input_tokens: mp.max_input_tokens.unwrap_or(model.max_input_tokens) as u32,
            max_output_tokens: mp.max_output_tokens.unwrap_or(model.max_output_tokens) as u32,
            tool_calling: mp.tool_calling.unwrap_or(model.tool_calling),
            vision: mp.vision.unwrap_or(model.vision),
            thinking: if mp.thinking.unwrap_or(model.thinking) { Some(true) } else { None },
            adaptive_thinking: if mp.adaptive_thinking.unwrap_or(model.adaptive_thinking) { Some(true) } else { None },
            edit_tools: crate::types::EndpointEditToolName::empty(),
        };

        routes.push(ResolvedProviderRoute {
            model_name: model.model_name.clone(),
            capabilities,
            provider_name: provider.provider_id.clone(),
            provider_model_name: mp.provider_model_id.clone(),
            priority: mp.priority as u32,
            compatibility: mp.compatibility.clone(),
            compat_settings,
            base_url: provider.base_url.clone(),
            api_key: selected_key.key.clone(),
            key_label: selected_key.label.clone(),
        });
    }

    Ok(routes)
}

/// 列出所有已启用提供者的可用模型。
pub async fn list_available_models(db: &db::Db) -> Result<Vec<AvailableModel>, String> {
    list_models_internal(db, true).await
}

/// 列出所有模型（包括未启用的，供 Admin 使用）。
pub async fn list_all_models(db: &db::Db) -> Result<Vec<AvailableModel>, String> {
    list_models_internal(db, false).await
}

/// 内部：列出模型及其提供者。
async fn list_models_internal(db: &db::Db, only_enabled: bool) -> Result<Vec<AvailableModel>, String> {
    // 加载所有模型
    let all_models = DbLLMModel::all()
        .exec(&mut db.clone())
        .await
        .map_err(|e| e.to_string())?;

    let mut result: HashMap<u64, AvailableModel> = HashMap::new();

    for model in all_models {
        // 查找该模型的所有 ModelProvider 关联
        let mp_filter = if only_enabled {
            DbModelProvider::filter(
                DbModelProvider::fields().model_id().eq(model.id)
                    .and(DbModelProvider::fields().enabled().eq(true))
            )
        } else {
            DbModelProvider::filter(
                DbModelProvider::fields().model_id().eq(model.id)
            )
        };

        let model_providers = mp_filter
            .exec(&mut db.clone())
            .await
            .map_err(|e| e.to_string())?;

        // 按 priority 排序
        let mut sorted_mps: Vec<_> = model_providers;
        sorted_mps.sort_by_key(|mp| mp.priority);

        let mut provider_infos = Vec::new();

        for mp in sorted_mps {
            let provider = match DbProvider::get_by_id(&mut db.clone(), &mp.provider_id).await {
                Ok(p) => p,
                Err(_) => continue,
            };

            if only_enabled && !provider.enabled {
                continue;
            }

            provider_infos.push(ModelProviderInfo {
                provider_id: provider.provider_id.clone(),
                provider_display_name: provider.display_name.clone(),
                provider_model_id: mp.provider_model_id.clone(),
                compatibility: mp.compatibility.clone(),
                max_input_tokens: mp.max_input_tokens,
                max_output_tokens: mp.max_output_tokens,
                tool_calling: mp.tool_calling,
                vision: mp.vision,
                thinking: mp.thinking,
                adaptive_thinking: mp.adaptive_thinking,
                input_price_per_1m: mp.input_price_per_1m,
                output_price_per_1m: mp.output_price_per_1m,
                cache_read_price_per_1m: mp.cache_read_price_per_1m,
                enabled: mp.enabled && provider.enabled,
                priority: mp.priority,
            });
        }

        if only_enabled && provider_infos.is_empty() {
            continue; // 无可用提供者时跳过
        }

        let nominal_capabilities = LMModelInfo {
            name: model.model_name.clone(),
            max_input_tokens: model.max_input_tokens as u32,
            max_output_tokens: model.max_output_tokens as u32,
            tool_calling: model.tool_calling,
            vision: model.vision,
            thinking: if model.thinking { Some(true) } else { None },
            adaptive_thinking: if model.adaptive_thinking { Some(true) } else { None },
            edit_tools: crate::types::EndpointEditToolName::empty(),
        };

        result.insert(model.id, AvailableModel {
            model_name: model.model_name,
            display_name: model.display_name,
            description: model.description,
            nominal_capabilities,
            providers: provider_infos,
        });
    }

    Ok(result.into_values().collect())
}
