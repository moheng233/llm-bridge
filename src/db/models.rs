//! 数据模型定义 — toasty ORM 模型。
//!
//! 包含用户、Token、用量记录、模型（规范定义）、提供者、提供者协议、
//! 模型-提供者关联七张核心表，以及请求追踪（可观察性）两张表。
//! 所有模型通过 `toasty::Model` derive 宏生成数据库操作代码。
//!
//! ## 核心设计（OpenRouter 风格）
//!
//! - **LLM 模型（LLMModel）** 是主要索引，拥有标称能力和描述
//! - **提供者（Provider）** 是上游 LLM 服务配置，持有跨协议共享的 API Keys
//! - **提供者协议（ProviderProtocol）** 声明提供者支持的协议及其端点 URL
//! - **模型-提供者关联（ModelProvider）** 记录哪个提供者以何种协议提供哪个模型，
//!   以及提供者侧的实际参数（可覆盖标称值）和独立定价
//!
//! ```text
//! LLMModel --< ModelProvider >-- ProviderProtocol --< Provider
//! ```
//!
//! Token 的 `allowed_models` 引用 `models.model_name`。

use jiff::Timestamp;
use toasty::schema::Deferred;

use crate::config::models::{ApiKeyEntry, ProviderCompatibility, ProviderQuotaAdapter};

/// 用户角色。
#[derive(Debug, Clone, PartialEq, Eq, toasty::Embed)]
pub enum UserRole {
    Admin,
    Member,
}

// ── 用户表 ──

/// 用户 — 通过 OIDC 登录自动创建。
///
/// 首次登录的用户自动成为管理员（admin），后续用户为普通成员（member）。
#[derive(Debug, toasty::Model)]
#[table = "users"]
pub struct User {
    #[key]
    #[auto]
    pub id: u64,

    /// OIDC subject — 在 IdP 中唯一标识用户
    #[unique]
    pub oidc_sub: String,

    /// 显示名
    pub name: String,
    /// 邮箱
    pub email: Option<String>,
    /// 头像 URL
    pub avatar_url: Option<String>,

    /// 角色
    pub role: UserRole,

    /// 账户是否启用
    pub active: bool,

    #[auto]
    pub created_at: Timestamp,
    #[auto]
    pub updated_at: Timestamp,

    #[has_many]
    pub tokens: Deferred<Vec<Token>>,
}

// ── API Token 表 ──

/// API Token — 用户创建的 Bearer Token，用于 API 认证。
///
/// Token 明文仅在创建时返回一次，数据库中存储 bcrypt 哈希。
/// `allowed_models` 存储 JSON 数组字符串，引用 `models.model_name`。
/// 空数组表示允许全部模型。
#[derive(Debug, toasty::Model)]
#[table = "tokens"]
pub struct Token {
    #[key]
    #[auto]
    pub id: u64,

    #[index]
    pub user_id: u64,
    #[belongs_to(key = user_id, references = id)]
    pub user: Deferred<User>,

    /// Token 名称（用户自定义，如 "dev-machine"）
    pub name: String,

    /// bcrypt 哈希值
    pub token_hash: String,
    /// Token 前缀（UI 识别用，如 "lb_ab3x..."）
    pub token_prefix: String,

    /// 允许使用的模型列表（JSON 数组字符串，引用 `models.model_name`）
    pub allowed_models: String,

    /// 周期内最大请求数（0 表示不限制）
    pub request_quota: i64,
    /// 周期内最大 Token 消耗量（0 表示不限制）
    pub token_quota: i64,
    /// 配额周期：`daily` / `monthly` / `unlimited`
    pub quota_period: String,

    /// 是否启用
    pub active: bool,

    #[auto]
    pub created_at: Timestamp,
    pub last_used_at: Option<i64>,

    #[has_many]
    pub usage_records: Deferred<Vec<UsageRecord>>,
}

// ── 用量记录表 ──

/// 用量记录 — 每个 Token 每个计费周期一条。
#[derive(Debug, toasty::Model)]
#[table = "usage_records"]
pub struct UsageRecord {
    #[key]
    #[auto]
    pub id: u64,

    #[index]
    pub token_id: u64,
    #[belongs_to(key = token_id, references = id)]
    pub token: Deferred<Token>,

    /// 周期标识（如 `"2026-05"`）
    pub period_key: String,

    /// 当前周期已使用请求数
    pub request_count: i64,
    /// 当前周期已使用 Token 数
    pub token_count: i64,

    #[auto]
    pub updated_at: Timestamp,
}

// ── 模型表（规范模型定义，主要索引）──

/// LLM 模型 — 规范模型定义，是网关的主要索引单位。
///
/// 对标 OpenRouter 的模型概念：`model_name` 是模型的唯一标识
/// （如 `"openai/gpt-4o"`、`"anthropic/claude-sonnet-4"`），
/// 其中前缀是品牌标识而非提供者。
///
/// 标称能力与描述存储在此表。不同提供者提供的同一模型
/// 可能具有不同的实际能力和定价，这些差异记录在 [`ModelProvider`] 中。
#[derive(Debug, toasty::Model)]
#[table = "models"]
pub struct LLMModel {
    #[key]
    #[auto]
    pub id: u64,

    /// 模型唯一标识（如 `"openai/gpt-4o"`），用于路由与 Token allowed_models 匹配
    #[unique]
    pub model_name: String,

    /// 显示名称
    pub display_name: String,

    /// 描述
    pub description: Option<String>,

    // ── 标称能力 ──
    pub max_input_tokens: i64,
    pub max_output_tokens: i64,
    pub tool_calling: bool,
    pub vision: bool,
    pub thinking: bool,
    pub adaptive_thinking: bool,

    /// 模型状态（如 `"stable"`, `"beta"`, `"deprecated"`）
    pub status: Option<String>,

    #[auto]
    pub created_at: Timestamp,
    #[auto]
    pub updated_at: Timestamp,

    #[has_many]
    pub providers: Deferred<Vec<ModelProvider>>,
}

// ── 提供者配置表 ──

/// 提供者 — 上游 LLM 提供者配置。
///
/// 管理员通过 Admin API 手动创建。
/// `api_keys` 使用 toasty::Json 原生 JSON 列存储，支持多 Key 轮询（跨协议共享）。
/// 协议和 URL 配置拆分到 [`ProviderProtocol`] 表。
#[derive(Debug, toasty::Model)]
#[table = "providers"]
pub struct Provider {
    #[key]
    #[auto]
    pub id: u64,

    /// 提供者唯一标识（如 `"openai"`, `"anthropic"`）
    #[unique]
    pub provider_id: String,
    /// 显示名称
    pub display_name: String,

    /// API Keys（JSON 列，跨协议共享，加权轮询选择）
    pub api_keys: toasty::Json<Vec<ApiKeyEntry>>,

    /// 是否启用
    pub enabled: bool,
    /// 优先级（数字越小优先级越高）
    pub priority: i64,

    /// 额度适配器类型（NULL = 不查询上游额度）。
    ///
    /// 声明该 Provider 使用哪种上游额度查询协议（如 umans）。
    /// 启用后可通过 Admin API `GET /api/v1/admin/providers/{id}/quota`
    /// 实时查询每个 API Key 的剩余额度。
    pub quota_adapter: Option<ProviderQuotaAdapter>,
    /// 适配器配置（JSON 字符串，对应 [`crate::config::models::QuotaAdapterConfig`]）。
    pub quota_adapter_config: Option<String>,

    #[auto]
    pub created_at: Timestamp,
    #[auto]
    pub updated_at: Timestamp,

    #[has_many]
    pub model_links: Deferred<Vec<ModelProvider>>,

    #[has_many]
    pub protocols: Deferred<Vec<ProviderProtocol>>,
}

// ── 提供者协议配置表 ──

/// 提供者协议配置 — 记录提供者支持的每种协议及其端点 URL。
///
/// 一个 Provider 可以有多个 ProviderProtocol，例如 OpenAI 同时支持
/// Chat Completions 和 Responses 两种协议，分别配置不同 URL。
///
/// `compat_settings` 从 Provider 下沉至此，不同协议可有不同 HTTP 定制。
#[derive(Debug, toasty::Model)]
#[table = "provider_protocols"]
pub struct ProviderProtocol {
    #[key]
    #[auto]
    pub id: u64,

    /// 关联的提供者
    #[index]
    pub provider_id: u64,
    #[belongs_to(key = provider_id, references = id)]
    pub provider: Deferred<Provider>,

    /// 协议枚举
    pub protocol: ProviderCompatibility,

    /// 协议端点 URL（必填）
    pub base_url: String,

    /// 自定义 HTTP 设置（JSON 对象字符串，对应 CompatibilitySettings）
    pub compat_settings: Option<String>,

    /// 是否启用
    pub enabled: bool,
    /// 多协议间的优先级
    pub priority: i64,

    #[auto]
    pub created_at: Timestamp,
    #[auto]
    pub updated_at: Timestamp,
}

// ── 模型-提供者关联表 ──

/// 模型-提供者关联 — 记录哪个提供者提供哪个模型。
///
/// 这是 LLM 模型（LLMModel）与提供者（Provider）之间的多对多关联表。
/// 包含提供者侧的实际能力覆盖（nullable = 使用模型标称值）、
/// 提供者特定的定价，以及协议引用。
///
/// 同一模型可有多个提供者，按 `priority` 排序作为 fallback 链。
#[derive(Debug, toasty::Model)]
#[table = "model_providers"]
pub struct ModelProvider {
    #[key]
    #[auto]
    pub id: u64,

    /// 关联的 LLM 模型
    #[index]
    pub model_id: u64,
    #[belongs_to(key = model_id, references = id)]
    pub llm_model: Deferred<LLMModel>,

    /// 关联的提供者
    #[index]
    pub provider_id: u64,
    #[belongs_to(key = provider_id, references = id)]
    pub provider: Deferred<Provider>,

    /// 提供者侧的模型 ID（如 `"gpt-4o"`）
    pub provider_model_id: String,

    /// 关联的协议配置（FK → provider_protocols）
    #[index]
    pub protocol_id: u64,
    #[belongs_to(key = protocol_id, references = id)]
    pub protocol: Deferred<ProviderProtocol>,

    /// 显示名称
    pub display_name: String,

    // ── 能力覆盖（nullable = 使用 LLMModel 标称值）──
    pub max_input_tokens: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub tool_calling: Option<bool>,
    pub vision: Option<bool>,
    pub thinking: Option<bool>,
    pub adaptive_thinking: Option<bool>,

    // ── 提供者特定定价（每 1M tokens，美元）──
    pub input_price_per_1m: Option<f64>,
    pub output_price_per_1m: Option<f64>,
    pub cache_read_price_per_1m: Option<f64>,

    /// 是否启用
    pub enabled: bool,
    /// 优先级（数字越小优先级越高，同模型多提供者间 fallback 排序）
    pub priority: i64,

    #[auto]
    pub created_at: Timestamp,
    #[auto]
    pub updated_at: Timestamp,
}

// ── 请求追踪（可观察性）──

/// 请求来源接口。
///
/// 对应 PLAN.md §5 的 `interface` 字段：区分 OpenAI 兼容 HTTP 与 WS RPC 两种传输绑定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed)]
pub enum TraceInterface {
    /// `POST /v1/chat/completions`（OpenAI 兼容 HTTP）
    OpenAiHttp,
    /// `GET /v1/ws`（协议无关 WebSocket RPC，§4）
    WsRpc,
}

/// 请求生命周期状态机：`pending → streaming → finalized`。
///
/// 请求开始时 INSERT（pending），首个 chunk 到达后转 streaming，
/// 结束时 upsert 为终态（success / error / cancelled）。
/// 中途崩溃时记录停留在 pending/streaming，可见「卡住」的请求而非丢记录
///（Langfuse observation upsert 模式）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed)]
pub enum TraceStatus {
    /// 请求已受理，尚未产生任何 chunk
    Pending,
    /// 已收到首个上游 chunk，流式进行中
    Streaming,
    /// 正常完成
    Success,
    /// 失败（含上游错误与内部错误）
    Error,
    /// 被客户端取消（WS cancel / 断连）
    Cancelled,
}

impl TraceStatus {
    /// 是否为终态（success / error / cancelled）。
    pub fn is_final(self) -> bool {
        matches!(self, Self::Success | Self::Error | Self::Cancelled)
    }
}

// ── 请求追踪表 ──

/// LLM 请求追踪 — 一次请求生命周期的全部结构化事实落为一行记录（单一事实源）。
///
/// metrics 是其流式投影、计费查询是其 SQL 聚合、内容快照是其可空大字段——
/// 不建三套采集管线。语义遵循 OpenTelemetry GenAI 语义约定。
///
/// 写入路径：handler 热路径只发 mpsc 事件，专用后台任务批量落盘；
/// mpsc 满则丢弃并计数（观察性数据可丢，业务请求不可阻塞）。
#[derive(Debug, toasty::Model)]
#[table = "llm_request_traces"]
pub struct LlmRequestTrace {
    #[key]
    #[auto]
    pub id: u64,

    /// 网关生成的请求 ID（UUID），响应头 `x-request-id` 回传，
    /// 贯穿 stdout 日志 / OTel / DB 三方互查。
    #[unique]
    pub request_id: String,

    /// OTel trace id（otel 启用时双写互查）。
    pub trace_id: Option<String>,

    /// 请求来源接口（`openai_http` / `ws_rpc`）。
    pub interface: TraceInterface,

    // ── 归属 ──
    /// 发起请求的 Token ID（`token_hash` 永不进表）。
    #[index]
    pub token_id: u64,
    /// Token 归属的用户 ID。
    #[index]
    pub user_id: u64,
    /// Token 前缀（UI 识别用，冗余存储避免 join）。
    pub token_prefix: String,

    // ── 路由结果 ──
    /// 规范模型名（如 `"openai/gpt-4o"`）。
    #[index]
    pub model: String,
    /// 路由命中的提供者 ID（`providers.provider_id`）。
    pub provider_id: String,
    /// 提供者侧的实际模型 ID（`model_providers.provider_model_id`）。
    pub provider_model_id: String,
    /// 路由命中的协议（`openai` / `anthropic` / …）。
    pub protocol: String,

    // ── 生命周期 ──
    /// 生命周期状态机。
    pub status: TraceStatus,
    /// 错误类型（`provider_error` / `quota_exceeded` / …）。
    pub error_type: Option<String>,
    /// 错误消息（截断存储）。
    pub error_message: Option<String>,
    /// 上游 HTTP 状态码。
    pub upstream_status: Option<u16>,
    /// 上游真实 finish_reason（stop / length / tool_calls / …）。
    pub finish_reason: Option<String>,

    // ── 用量与计费 ──
    /// 预扣量（解释配额结算 delta）。
    pub estimated_tokens: i64,
    /// 输入 tokens（`LanguageModelUsagePart` 持久化形态）。
    pub input_tokens: Option<u64>,
    /// 输出 tokens。
    pub output_tokens: Option<u64>,
    /// 推理 tokens（thinking）。
    pub reasoning_tokens: Option<u64>,
    /// 缓存命中 tokens。
    pub cached_tokens: Option<u64>,
    /// 总 tokens。
    pub total_tokens: Option<u64>,
    /// 成本（`model_providers` 定价 × usage，派生）。
    pub cost_usd: Option<f64>,

    // ── 上游元数据 ──
    /// 上游请求 ID（`ProviderResponseMetadata.id`）。
    pub upstream_request_id: Option<String>,

    // ── 时间线 ──
    /// 请求开始时间。
    #[auto]
    pub created_at: Timestamp,
    /// 首个 chunk 到达时间。
    pub first_chunk_at: Option<Timestamp>,
    /// 请求完成时间。
    pub completed_at: Option<Timestamp>,
    /// 首 chunk 延迟（毫秒，派生存储）。
    pub ttft_ms: Option<i64>,
    /// 总延迟（毫秒，派生存储）。
    pub latency_ms: Option<i64>,

    // ── 内容快照（PII 敏感，Opt-In）──
    /// 请求消息快照，仅当 `LLM_BRIDGE_OBS_CAPTURE_CONTENT=true` 时写入。
    pub request_messages: Option<toasty::Json<Vec<crate::types::LanguageModelChatMessage>>>,
    /// 聚合后响应 parts 快照，同上 Opt-In。
    pub response_parts: Option<toasty::Json<Vec<crate::types::LMResponsePart>>>,
}

// ── 日度用量预聚合表 ──

/// 日度用量聚合 — `day` × `token_id` × `model` 的 rollup。
///
/// 由 finalize 事件同事务更新，仪表盘聚合查询不全表扫 trace 表。
/// 已聚合无 PII，永久保留（不受 trace retention 影响）。
#[derive(Debug, toasty::Model)]
#[table = "usage_daily"]
pub struct UsageDaily {
    #[key]
    #[auto]
    pub id: u64,

    /// 日期（`"YYYY-MM-DD"` 格式，UTC）。
    #[index]
    pub day: String,
    /// 发起请求的 Token ID。
    #[index]
    pub token_id: u64,
    /// 规范模型名。
    #[index]
    pub model: String,

    /// 当日请求总数。
    pub request_count: i64,
    /// 当日输入 tokens 合计。
    pub input_tokens: i64,
    /// 当日输出 tokens 合计。
    pub output_tokens: i64,
    /// 当日推理 tokens 合计。
    pub reasoning_tokens: i64,
    /// 当日缓存命中 tokens 合计。
    pub cached_tokens: i64,
    /// 当日总 tokens 合计。
    pub total_tokens: i64,
    /// 当日成本合计（美元）。
    pub cost_usd: f64,

    #[auto]
    pub updated_at: Timestamp,
}
