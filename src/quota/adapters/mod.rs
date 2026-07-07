//! 适配器模块入口 — 定义统一 trait、错误类型、通用额度模型与工厂函数。

pub mod types;
mod umans;

pub use types::{CreditQuota, QuotaInfo, QuotaWindow, RequestQuota};
pub use umans::UmansQuotaAdapter;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;
use ts_rs::TS;

use crate::config::models::{ProviderQuotaAdapter, QuotaAdapterConfig};
use crate::db::models::Provider;

/// 单个 API Key 的额度查询结果 — 供 Admin API 响应组装使用。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct KeyQuota {
    /// API Key 的 label（来自 `Provider.api_keys[*].label`）。
    pub label: String,
    /// 掩码后的 API Key（仅展示用）。
    pub masked_key: String,
    /// 查询成功时的额度信息；失败时为 `None`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota: Option<QuotaInfo>,
    /// 错误信息（查询失败时）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 查询时间戳（Unix 秒）。
    pub fetched_at: i64,
}

/// Provider 实时额度响应（`GET /api/v1/admin/providers/{id}/quota`）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ProviderQuotaResponse {
    /// Provider 的 row id。
    pub id: u64,
    /// Provider 唯一标识（如 `"umans"`）。
    pub provider_id: String,
    /// 使用的适配器类型。
    pub adapter: ProviderQuotaAdapter,
    /// 每个 API Key 的额度查询结果（顺序与 `Provider.api_keys` 一致，过滤后仅保留匹配项）。
    pub keys: Vec<KeyQuota>,
}

/// 额度适配器统一接口 — 每个 [`ProviderQuotaAdapter`] 变体对应一个实现。
///
/// 实现者负责：
/// - 构造并发送上游额度查询请求
/// - 解析响应并映射为 [`QuotaInfo`]
#[ractor::async_trait]
pub trait QuotaAdapter: Send + Sync {
    /// 返回适配器的标识枚举（供响应序列化与日志使用）。
    fn kind(&self) -> ProviderQuotaAdapter;

    /// 查询单个 API Key 的额度。
    ///
    /// `config` 来自 `Provider.quota_adapter_config` 的反序列化结果，字段全部可选；
    /// 未提供某字段时实现者应使用内置默认值（如默认 endpoint）。
    async fn fetch_quota(
        &self,
        api_key: &str,
        config: &QuotaAdapterConfig,
    ) -> Result<QuotaInfo, QuotaAdapterError>;
}

/// 额度适配器错误。
#[derive(Debug, Error)]
pub enum QuotaAdapterError {
    #[error("network error: {0}")]
    Network(String),
    #[error("upstream returned status {status}: {body}")]
    UpstreamStatus { status: u16, body: String },
    #[error("response parse error: {0}")]
    Parse(String),
    #[error("unauthorized: upstream rejected api key")]
    Unauthorized,
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    #[error("other: {0}")]
    Other(String),
}

impl From<reqwest::Error> for QuotaAdapterError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            QuotaAdapterError::Network(format!("timeout: {e}"))
        } else if e.is_connect() {
            QuotaAdapterError::Network(format!("connect: {e}"))
        } else {
            QuotaAdapterError::Network(e.to_string())
        }
    }
}

/// 工厂函数 — 根据枚举变体返回对应适配器实例。
///
/// 返回 `Arc<dyn QuotaAdapter>` 包装；内部适配器自带 `reqwest::Client` 复用连接池。
/// 未识别的变体返回 `None`（调用方应视为“未配置适配器”）。
pub fn adapter_for(adapter: ProviderQuotaAdapter) -> Option<Arc<dyn QuotaAdapter>> {
    match adapter {
        ProviderQuotaAdapter::Umans => Some(Arc::new(UmansQuotaAdapter::new())),
    }
}

/// 内部构造默认 `QuotaAdapterConfig`（当 DB 字段为 NULL 或解析失败时回退）。
fn parse_config(raw: &Option<String>) -> QuotaAdapterConfig {
    match raw {
        Some(s) if !s.trim().is_empty() => {
            serde_json::from_str(s).unwrap_or_default()
        }
        _ => QuotaAdapterConfig::default(),
    }
}

/// 高层封装：为单个 Provider 查询所有（或按 `key_label_filter` 过滤）API Key 的额度。
///
/// 并发查询每个 Key，单 Key 失败不影响其余，最终聚合为 `ProviderQuotaResponse`。
/// 若 `provider.quota_adapter` 为 `None` 返回 `Ok(None)`（调用方据此返回 400/404）。
///
/// 复用适配器实例：所有 Key 共享同一 `Arc<dyn QuotaAdapter>`，避免重复创建 client。
pub async fn fetch_provider_quota(
    provider: &Provider,
) -> Result<Option<ProviderQuotaResponse>, QuotaAdapterError> {
    let Some(adapter_kind) = provider.quota_adapter.clone() else {
        return Ok(None);
    };

    let Some(adapter) = adapter_for(adapter_kind.clone()) else {
        return Ok(None);
    };

    let config = parse_config(&provider.quota_adapter_config);
    let api_keys: &Vec<crate::config::models::ApiKeyEntry> = &provider.api_keys;

    let now = jiff::Timestamp::now().as_millisecond() / 1000;

    // 并发查询所有匹配 label 的 key
    let selected: Vec<&crate::config::models::ApiKeyEntry> = match &config.key_label_filter {
        Some(label) => api_keys.iter().filter(|k| &k.label == label).collect(),
        None => api_keys.iter().collect(),
    };

    // 用 Mutex 收集以保证顺序与线程安全；并发任务通过 join 拉起。
    let collected: Arc<Mutex<Vec<KeyQuota>>> = Arc::new(Mutex::new(Vec::with_capacity(selected.len())));

    let mut tasks = Vec::new();
    for k in selected {
        let adapter = adapter.clone();
        let config = config.clone();
        let key_str = k.key.clone();
        let label = k.label.clone();
        let masked = crate::store::mask_key(&k.key);
        let collected = collected.clone();
        tasks.push(tokio::spawn(async move {
            let result = adapter.fetch_quota(&key_str, &config).await;
            let entry = match result {
                Ok(quota) => KeyQuota {
                    label,
                    masked_key: masked,
                    quota: Some(quota),
                    error: None,
                    fetched_at: now,
                },
                Err(e) => KeyQuota {
                    label,
                    masked_key: masked,
                    quota: None,
                    error: Some(e.to_string()),
                    fetched_at: now,
                },
            };
            collected.lock().await.push(entry);
        }));
    }
    for t in tasks {
        let _ = t.await;
    }
    let results = collected.lock().await.drain(..).collect();

    Ok(Some(ProviderQuotaResponse {
        id: provider.id,
        provider_id: provider.provider_id.clone(),
        adapter: adapter_kind,
        keys: results,
    }))
}
