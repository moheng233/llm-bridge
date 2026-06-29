//! 数据模型定义 — toasty ORM 模型。
//!
//! 包含用户、Token、用量记录、模型（规范定义）、提供者、模型-提供者关联六张核心表。
//! 所有模型通过 `toasty::Model` derive 宏生成数据库操作代码。
//!
//! ## 核心设计（OpenRouter 风格）
//!
//! - **LLM 模型（LLMModel）** 是主要索引，拥有标称能力和描述
//! - **提供者（Provider）** 是上游 LLM 服务配置
//! - **模型-提供者关联（ModelProvider）** 记录哪个提供者提供哪个模型，
//!   以及提供者侧的实际参数（可覆盖标称值）和独立定价
//!
//! ```
//! LLMModel ──< ModelProvider >── Provider
//! ```
//!
//! Token 的 `allowed_models` 引用 `models.model_name`。

use jiff::Timestamp;
use toasty::schema::Deferred;

use crate::config::models::ProviderCompatibility;

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
/// 管理员手动创建或从 models.dev 导入。
/// `api_keys` 存储 JSON 数组，支持多 Key 轮询。
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

    /// AI SDK npm 包名（从 models.dev 导入时填充）
    pub npm: Option<String>,

    /// 基础 URL
    pub base_url: Option<String>,

    /// API Keys（JSON 数组字符串）
    pub api_keys: String,

    /// 自定义 HTTP 设置（JSON 对象字符串）
    pub compat_settings: Option<String>,

    /// 是否启用
    pub enabled: bool,
    /// 优先级（数字越小优先级越高）
    pub priority: i64,

    #[auto]
    pub created_at: Timestamp,
    #[auto]
    pub updated_at: Timestamp,

    #[has_many]
    pub model_links: Deferred<Vec<ModelProvider>>,
}

// ── 模型-提供者关联表 ──

/// 模型-提供者关联 — 记录哪个提供者提供哪个模型。
///
/// 这是 LLM 模型（LLMModel）与提供者（Provider）之间的多对多关联表。
/// 包含提供者侧的实际能力覆盖（nullable = 使用模型标称值）、
/// 提供者特定的定价，以及兼容协议信息。
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

    /// 兼容协议
    pub compatibility: ProviderCompatibility,

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
