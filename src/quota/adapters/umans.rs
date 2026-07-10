//! umans 平台额度适配器。
//!
//! ## 接口
//!
//! ```bash
//! curl https://api.code.umans.ai/v1/usage \
//!   -H "Authorization: Bearer sk-your-umans-api-key"
//! ```
//!
//! ## 响应示例
//!
//! ```json
//! {
//!   "plan": { "display_name": "Code Max" },
//!   "limits": {
//!     "requests":    { "limit": 200, "hard_cap": 400, "burst_pct": 1.0, "window_seconds": 18000 },
//!     "concurrency": { "limit": 4,   "hard_cap": 8,   "burst_pct": 1.0 }
//!   },
//!   "usage": {
//!     "requests_in_window": 48,
//!     "remaining_requests": 152,
//!     "concurrent_sessions": 1,
//!     "tokens_in": 1200000,
//!     "tokens_out": 340000,
//!     "priority": { "low": false, "boxed_until": null, "reason": null }
//!   }
//! }
//! ```
//!
//! 归一化为 [`QuotaInfo::Requests`]：
//! - `window = SlidingSeconds(window_seconds)`（5h = 18000）
//! - `limit = limits.requests.limit`
//! - `used = usage.requests_in_window`
//! - `remaining = usage.remaining_requests`
//! - `hard_cap = limits.requests.hard_cap`
//! - `concurrency_limit = limits.concurrency.limit`
//! - `concurrent_sessions = usage.concurrent_sessions`
//! - `extra`：`plan.display_name`、`tokens_in`、`tokens_out`、`priority.low`、`priority.reason`

use std::collections::HashMap;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;
use tracing::debug;

use crate::config::models::{ProviderQuotaAdapter, QuotaAdapterConfig};

use super::types::{QuotaInfo, QuotaWindow, RequestQuota};
use super::{QuotaAdapter, QuotaAdapterError};

const DEFAULT_BASE_URL: &str = "https://api.code.umans.ai/v1/usage";

/// umans 适配器实例 — 共享一个 `reqwest::Client` 复用连接池。
pub struct UmansQuotaAdapter {
    client: reqwest::Client,
}

impl UmansQuotaAdapter {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("failed to build reqwest client for umans adapter");
        Self { client }
    }
}

impl Default for UmansQuotaAdapter {
    fn default() -> Self {
        Self::new()
    }
}

// ── 响应 schema（仅包含需要解析的字段，其余忽略） ──

#[derive(Debug, Deserialize)]
struct UmansUsageResponse {
    #[serde(default)]
    plan: UmansPlan,
    #[serde(default)]
    limits: UmansLimits,
    #[serde(default)]
    usage: UmansUsage,
}

#[derive(Debug, Default, Deserialize)]
struct UmansPlan {
    #[serde(default)]
    display_name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct UmansLimits {
    #[serde(default)]
    requests: Option<UmansRequestLimit>,
    #[serde(default)]
    concurrency: Option<UmansConcurrencyLimit>,
}

#[derive(Debug, Default, Deserialize)]
struct UmansRequestLimit {
    #[serde(default)]
    limit: Option<u64>,
    #[serde(default)]
    hard_cap: Option<u64>,
    #[serde(default)]
    burst_pct: Option<f64>,
    #[serde(default)]
    window_seconds: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct UmansConcurrencyLimit {
    #[serde(default)]
    limit: Option<u64>,
    #[serde(default)]
    hard_cap: Option<u64>,
    #[serde(default)]
    burst_pct: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct UmansUsage {
    #[serde(default)]
    requests_in_window: Option<u64>,
    #[serde(default)]
    remaining_requests: Option<u64>,
    #[serde(default)]
    concurrent_sessions: Option<u64>,
    #[serde(default)]
    tokens_in: Option<u64>,
    #[serde(default)]
    tokens_out: Option<u64>,
    #[serde(default)]
    priority: UmansPriority,
}

#[derive(Debug, Default, Deserialize)]
struct UmansPriority {
    #[serde(default, rename = "low")]
    low: Option<bool>,
    /// umans 返回该字段但我们当前不展示；保留解析以便未来扩展。
    #[allow(dead_code)]
    #[serde(default)]
    boxed_until: Option<Value>,
    #[serde(default)]
    reason: Option<String>,
}

#[ractor::async_trait]
impl QuotaAdapter for UmansQuotaAdapter {
    fn kind(&self) -> ProviderQuotaAdapter {
        ProviderQuotaAdapter::Umans
    }

    async fn fetch_quota(
        &self,
        api_key: &str,
        config: &QuotaAdapterConfig,
    ) -> Result<QuotaInfo, QuotaAdapterError> {
        let endpoint = config
            .base_url
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(DEFAULT_BASE_URL);

        debug!(%endpoint, "umans: fetching quota");

        let resp = self
            .client
            .get(endpoint)
            .bearer_auth(api_key)
            .send()
            .await?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(QuotaAdapterError::Unauthorized);
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(QuotaAdapterError::UpstreamStatus {
                status: status.as_u16(),
                body,
            });
        }

        let parsed: UmansUsageResponse = resp
            .json()
            .await
            .map_err(|e| QuotaAdapterError::Parse(format!("decode body: {e}")))?;

        Ok(map_to_quota(parsed))
    }
}

/// 将 umans 原始响应映射为 [`QuotaInfo::Requests`]。
fn map_to_quota(resp: UmansUsageResponse) -> QuotaInfo {
    let req_limit = resp.limits.requests;
    let conc = resp.limits.concurrency;

    let limit = req_limit.as_ref().and_then(|l| l.limit).unwrap_or(0);
    let hard_cap = req_limit.as_ref().and_then(|l| l.hard_cap);
    let window_seconds = req_limit
        .as_ref()
        .and_then(|l| l.window_seconds)
        .unwrap_or(0);
    let window = if window_seconds > 0 {
        QuotaWindow::SlidingSeconds(window_seconds)
    } else {
        QuotaWindow::Unknown
    };

    let used = resp.usage.requests_in_window.unwrap_or(0);
    let remaining = resp
        .usage
        .remaining_requests
        .unwrap_or_else(|| limit.saturating_sub(used));

    let concurrency_limit = conc.as_ref().and_then(|c| c.limit);
    let concurrent_sessions = resp.usage.concurrent_sessions;

    // ── extra：保留 plan / 类型标识 / tokens / priority 等 ──
    let mut extra: HashMap<String, Value> = HashMap::new();
    if let Some(plan) = &resp.plan.display_name {
        extra.insert("plan_display_name".to_string(), Value::String(plan.clone()));
    }
    if let Some(tokens_in) = resp.usage.tokens_in {
        extra.insert("tokens_in".to_string(), Value::from(tokens_in));
    }
    if let Some(tokens_out) = resp.usage.tokens_out {
        extra.insert("tokens_out".to_string(), Value::from(tokens_out));
    }
    if let Some(low) = resp.usage.priority.low {
        extra.insert("priority_low".to_string(), Value::Bool(low));
    }
    if let Some(reason) = &resp.usage.priority.reason {
        extra.insert("priority_reason".to_string(), Value::String(reason.clone()));
    }
    if let Some(burst) = req_limit.as_ref().and_then(|l| l.burst_pct) {
        extra.insert("requests_burst_pct".to_string(), serde_json::json!(burst));
    }
    if let Some(burst) = conc.as_ref().and_then(|c| c.burst_pct) {
        extra.insert(
            "concurrency_burst_pct".to_string(),
            serde_json::json!(burst),
        );
    }
    if let Some(hc) = conc.as_ref().and_then(|c| c.hard_cap) {
        extra.insert("concurrency_hard_cap".to_string(), Value::from(hc));
    }

    QuotaInfo::Requests(RequestQuota {
        window,
        limit,
        used,
        remaining,
        hard_cap,
        concurrency_limit,
        concurrent_sessions,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sample_response() {
        let raw = r#"{
            "plan": { "display_name": "Code Max" },
            "limits": {
                "requests":    { "limit": 200, "hard_cap": 400, "burst_pct": 1.0, "window_seconds": 18000 },
                "concurrency": { "limit": 4,   "hard_cap": 8,   "burst_pct": 1.0 }
            },
            "usage": {
                "requests_in_window": 48,
                "remaining_requests": 152,
                "concurrent_sessions": 1,
                "tokens_in": 1200000,
                "tokens_out": 340000,
                "priority": { "low": false, "boxed_until": null, "reason": null }
            }
        }"#;

        let parsed: UmansUsageResponse = serde_json::from_str(raw).unwrap();
        let quota = match map_to_quota(parsed) {
            QuotaInfo::Requests(r) => r,
            QuotaInfo::Credits(_) => panic!("expected Requests"),
        };

        assert_eq!(quota.limit, 200);
        assert_eq!(quota.used, 48);
        assert_eq!(quota.remaining, 152);
        assert_eq!(quota.hard_cap, Some(400));
        assert_eq!(quota.concurrency_limit, Some(4));
        assert_eq!(quota.concurrent_sessions, Some(1));
        assert_eq!(quota.window, QuotaWindow::SlidingSeconds(18000));
        assert_eq!(
            quota.extra.get("plan_display_name"),
            Some(&Value::String("Code Max".to_string()))
        );
        assert_eq!(quota.extra.get("tokens_in"), Some(&Value::from(1200000u64)));
        assert_eq!(quota.extra.get("tokens_out"), Some(&Value::from(340000u64)));
        assert_eq!(quota.extra.get("priority_low"), Some(&Value::Bool(false)));
    }

    #[test]
    fn handles_missing_fields_gracefully() {
        let raw = r#"{}"#;
        let parsed: UmansUsageResponse = serde_json::from_str(raw).unwrap();
        let quota = match map_to_quota(parsed) {
            QuotaInfo::Requests(r) => r,
            QuotaInfo::Credits(_) => panic!("expected Requests"),
        };
        assert_eq!(quota.limit, 0);
        assert_eq!(quota.used, 0);
        assert_eq!(quota.remaining, 0);
        assert_eq!(quota.hard_cap, None);
        assert_eq!(quota.concurrency_limit, None);
        assert_eq!(quota.concurrent_sessions, None);
        assert_eq!(quota.window, QuotaWindow::Unknown);
        assert!(quota.extra.is_empty());
    }
}
