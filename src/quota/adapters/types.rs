//! 通用额度模型 — 将不同供应商的额度 API 响应归一化为统一表达。
//!
//! 分为两大类：
//! - [`QuotaInfo::Requests`]：按请求次数配额（带窗口类型、并发限制）
//! - [`QuotaInfo::Credits`]：按金额/额度配额（带计量单位）
//!
//! 适配器特定字段（如 umans 的 `tokens_in`/`tokens_out`/`plan`）放入 `extra` JSON map，
//! 便于前端展示而不必为每个供应商定制主结构。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// 归一化的额度信息。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", tag = "kind", content = "data")]
pub enum QuotaInfo {
    /// 按请求次数计费/限流的供应商（如 umans、Claude Pro）。
    Requests(RequestQuota),
    /// 按金额/额度计费的供应商（如 OpenRouter、OpenAI API）。
    Credits(CreditQuota),
}

/// 按请求次数的额度模型。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct RequestQuota {
    /// 配额窗口类型（5 小时滑动、日、周、月等）。
    pub window: QuotaWindow,
    /// 限额（软上限，即正常额度）。
    pub limit: u64,
    /// 当前窗口已使用请求数。
    pub used: u64,
    /// 剩余请求数（`limit - used`，但部分供应商会单独返回）。
    pub remaining: u64,
    /// 硬上限（部分供应商支持 burst）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hard_cap: Option<u64>,
    /// 并发会话上限（若供应商支持）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrency_limit: Option<u64>,
    /// 当前并发会话数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrent_sessions: Option<u64>,
    /// 适配器特定字段（如 umans 的 `tokens_in`、`tokens_out`、`plan`、`priority`）。
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    #[ts(type = "Record<string, any>")]
    pub extra: HashMap<String, Value>,
}

/// 按金额/额度的模型。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CreditQuota {
    /// 剩余额度数值。
    pub amount: f64,
    /// 单位（如 `"USD"`、`"credits"`、`"CNY"`）。
    pub currency: String,
    /// 适配器特定字段。
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    #[ts(type = "Record<string, any>")]
    pub extra: HashMap<String, Value>,
}

/// 配额窗口类型 — 覆盖主流供应商的重置周期。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum QuotaWindow {
    /// 滑动窗口（按秒计，例如 umans 的 5h = 18000 秒）。
    SlidingSeconds(u64),
    /// 滑动窗口（按分钟计，便于可读性）。
    SlidingMinutes(u64),
    /// 按自然日重置。
    CalendarDaily,
    /// 按自然周重置。
    CalendarWeekly,
    /// 按自然月重置。
    CalendarMonthly,
    /// 未知 / 供应商未明确窗口语义。
    Unknown,
}
