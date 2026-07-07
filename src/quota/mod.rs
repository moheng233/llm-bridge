//! 供应商额度适配器框架。
//!
//! 为不同上游 LLM 供应商的「额度/余额查询」提供统一抽象。每个供应商的额度 API
//! 形态各异（按请求次数 / 按金额，5 小时滑动窗口 / 日 / 周 / 月），本模块将其
//! 归一化为 [`adapters::QuotaInfo`] 通用模型。
//!
//! ## 架构
//!
//! ```text
//! Provider表(quota_adapter字段) ──► adapter_for() ──► QuotaAdapter trait ──► QuotaInfo
//!                                       │                       ▲
//!                    UmansQuotaAdapter ─┘               其他适配器...
//! ```
//!
//! 挂载级别为 `Provider`（账户级）— 因为额度通常按 API Key/账户维度而非模型维度。
//! 当 `quota_adapter` 为 `None` 时表示不查询上游额度，直接使用本地 Token 配额。
//!
//! ## 扩展点
//!
//! 新增供应商只需：
//! 1. 在 [`crate::config::models::ProviderQuotaAdapter`] 枚举中追加变体；
//! 2. 在 `adapters/` 下新建文件实现 [`adapters::QuotaAdapter`]；
//! 3. 在 [`adapters::adapter_for`] 工厂函数中注册。
//!
//! 第一阶段仅提供 Admin API 实时查询，不拦截请求；后续可在路由层或请求前钩子
//! 复用同一个 [`adapters::QuotaAdapter`] 实例做 enforcement。

pub mod adapters;

pub use adapters::{
    CreditQuota, KeyQuota, ProviderQuotaResponse, QuotaAdapter, QuotaAdapterError, QuotaInfo,
    QuotaWindow, RequestQuota, adapter_for, fetch_provider_quota,
};
