//! 数据模型定义 — toasty ORM 模型。
//!
//! 包含用户、Token、用量记录、提供者、提供者模型五张核心表。
//! 所有模型通过 `toasty::Model` derive 宏生成数据库操作代码。

use jiff::Timestamp;
use toasty::schema::{BelongsTo, HasMany};

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
    pub tokens: HasMany<Token>,
}

// ── API Token 表 ──

/// API Token — 用户创建的 Bearer Token，用于 API 认证。
///
/// Token 明文仅在创建时返回一次，数据库中存储 bcrypt 哈希。
/// `allowed_models` 存储 JSON 数组字符串，空数组表示允许全部模型。
#[derive(Debug, toasty::Model)]
#[table = "tokens"]
pub struct Token {
    #[key]
    #[auto]
    pub id: u64,

    #[index]
    pub user_id: u64,
    #[belongs_to(key = user_id, references = id)]
    pub user: BelongsTo<User>,

    /// Token 名称（用户自定义，如 "dev-machine"）
    pub name: String,

    /// bcrypt 哈希值
    pub token_hash: String,
    /// Token 前缀（UI 识别用，如 "lb_ab3x..."）
    pub token_prefix: String,

    /// 允许使用的模型列表（JSON 数组字符串，空数组 `[]` 表示全部）
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
    pub usage_records: HasMany<UsageRecord>,
}

// ── 用量记录表 ──

/// 用量记录 — 每个 Token 每个计费周期一条。
///
/// 记录当前周期已使用的请求数和 Token 数，
/// 用于配额检查与扣减。
#[derive(Debug, toasty::Model)]
#[table = "usage_records"]
pub struct UsageRecord {
    #[key]
    #[auto]
    pub id: u64,

    #[index]
    pub token_id: u64,
    #[belongs_to(key = token_id, references = id)]
    pub token: BelongsTo<Token>,

    /// 周期标识（如 `"2026-05"` 表示 2026 年 5 月）
    pub period_key: String,

    /// 当前周期已使用请求数
    pub request_count: i64,
    /// 当前周期已使用 Token 数
    pub token_count: i64,

    #[auto]
    pub updated_at: Timestamp,
}

// ── 提供者配置表 ──

/// 提供者 — 上游 LLM 提供者配置。
///
/// 管理员手动创建或从 models.dev 导入。
/// `api_keys` 存储 JSON 数组，支持多 Key 轮询。
/// `compat_settings` 存储 JSON 对象，对应自定义 HTTP 设置。
#[derive(Debug, toasty::Model)]
#[table = "providers"]
pub struct Provider {
    #[key]
    #[auto]
    pub id: u64,

    /// 提供者唯一标识，用于路由（如 `"openai"`, `"anthropic"`）
    #[unique]
    pub provider_id: String,
    /// 显示名称
    pub display_name: String,

    /// AI SDK npm 包名（从 models.dev 导入时填充）
    pub npm: Option<String>,

    /// 基础 URL（默认按 AI SDK npm 推导，可覆盖）
    pub base_url: Option<String>,

    /// API Keys（JSON 数组字符串）
    pub api_keys: String,

    /// 自定义 HTTP 设置（JSON 对象字符串，对应 CompatibilitySettings）
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
    pub models: HasMany<ProviderModel>,
}

// ── 提供者模型表 ──

/// 提供者模型 — 提供者支持的模型配置。
///
/// `model_name` 是用于 API 路由和 Token `allowed_models` 匹配的规范化名称
/// （如 `"openai/gpt-4o"`）。`provider_model_id` 是上游 API 中的模型 ID
/// （如 `"gpt-4o"`）。
///
/// 能力与定价信息用于 `/v1/models` 增强返回。
#[derive(Debug, toasty::Model)]
#[table = "provider_models"]
pub struct ProviderModel {
    #[key]
    #[auto]
    pub id: u64,

    #[index]
    pub provider_row_id: u64,
    #[belongs_to(key = provider_row_id, references = id)]
    pub provider: BelongsTo<Provider>,

    /// 规范化模型名（如 `"openai/gpt-4o"`）
    #[unique]
    pub model_name: String,
    /// 提供者侧的模型 ID（如 `"gpt-4o"`）
    pub provider_model_id: String,

    /// 兼容协议
    pub compatibility: ProviderCompatibility,

    /// 显示名称
    pub display_name: String,
    /// 描述
    pub description: Option<String>,

    // ── 能力指标 ──
    pub max_input_tokens: i64,
    pub max_output_tokens: i64,
    pub tool_calling: bool,
    pub vision: bool,
    pub thinking: bool,
    pub adaptive_thinking: bool,

    /// 输入价格（每 1M tokens，美元）
    pub input_price_per_1m: Option<f64>,
    /// 输出价格（每 1M tokens，美元）
    pub output_price_per_1m: Option<f64>,
    /// 缓存读取价格（每 1M tokens，美元）
    pub cache_read_price_per_1m: Option<f64>,

    /// 模型状态（如 `"stable"`, `"beta"`, `"deprecated"`）
    pub status: Option<String>,

    /// 是否启用
    pub enabled: bool,

    #[auto]
    pub created_at: Timestamp,
    #[auto]
    pub updated_at: Timestamp,
}
