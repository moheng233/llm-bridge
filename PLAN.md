# LLM-Bridge 重构计划：自托管 OpenRouter

> 日期：2026-05-18
> 目标：将 llm-bridge 重构为面向 homelab / 工作室 / 公司内部使用的自托管 LLM 网关
>
> **设计原则：不考虑向后兼容。现有数据结构大概率不满足新需求，将进行大幅重构。代码追求简洁、明确。**

---

## 1. 目标与定位

### 1.1 核心定位

一个**自托管的 OpenRouter**，特化于小规模团队内部使用的 LLM API 网关。

与公网 OpenRouter 的差异：
- 部署在用户自己的基础设施上（homelab、工作室服务器、公司内网）
- 面向受控的内部用户群体，而非公开注册
- 支持 OIDC 单点登录，与团队现有身份体系对接
- 提供细粒度的 API Token 权限控制与用量配额

### 1.2 指导原则

| 原则 | 说明 |
|------|------|
| **不方案回退** | 一旦确认方案就持续推进，不在开发中途因犹豫而退回旧方案 |
| **解决问题优先** | 遇到障碍先想办法解决，而不是绕过或降低需求 |
| **不自主做决定** | 任何需要权衡取舍的决策都需先讨论确认，不在代码中私自拍板 |
| **用 `cargo add` 加依赖** | 添加新 crate 时统一使用 `cargo add`，保持 `Cargo.toml` 格式一致 |

### 1.3 设计原则

本次重构遵循以下核心原则：

1. **不考虑向后兼容**：现有 JSON 文件存储、Bearer Token Admin 认证、基于 models.dev 的路由解析等将被彻底替换，不保留任何兼容层。
2. **大幅重构**：现有数据结构（`Store`、`ProviderConfig`、`ResolvedProviderRoute` 等）大概率不满足新需求，将按需重新设计。
3. **简洁明确**：避免过度抽象。能用 `String` 的地方不用泛型，能直接查 SQLite 的地方不加缓存层，能一个模块搞定的事情不拆三个。

### 1.4 设计决策

| 决策 | 选择 | 理由 |
|------|------|------|
| OIDC 数量 | **单 OIDC Provider** | 小团队通常只有一个 IdP（GitLab / Authentik / Keycloak），简化配置与实现 |
| Token 过期 | **暂不考虑** | 初期通过手动删除 Token 管理生命周期，后续按需加入 |
| 模型目录来源 | models.dev **仅作配置辅助** | 管理员从 models.dev 浏览发现提供者与模型，确认后写入本地数据库；运行时完全依赖数据库 |
| 兼容协议推导 | **导入时自动 + 手动可指定** | 从 models.dev 导入时根据 AI SDK npm 包名自动推导兼容协议；手动创建的提供者需显式指定 |
| 存储方案 | **toasty + SQLite 完全重写** | 废弃现有 JSON 文件存储，所有业务数据用 SQLite |
| Provider ID | **自增主键 + 唯一字符串标识** | 内部关联用数字 ID，路由和 API 中用字符串（如 "openai"） |
| Token 模型匹配 | **精确匹配列表** | allowed_models 中的模型名必须与 provider_models.model_name 完全一致；空数组表示全部允许 |
| Actor 模型 | **GatewayManagerActor 简化保留** | 保留后台任务（models.dev 刷新 + 配额重置），路由解析移到 Store 层 |
| VS Code 插件 | **暂不考虑** | 优先完成网关核心功能，后续验证 OpenAI 兼容格式的第三方插件兼容性 |

### 1.5 核心用户故事

1. **管理员**：部署后首次登录自动成为管理员，配置上游 LLM 提供者（可从 models.dev 浏览导入），管理用户权限
2. **团队成员**：通过团队 OIDC 登录，创建个人 API Token，配置可用模型与配额
3. **开发者**：使用 Token 调用 `/v1/chat/completions`，兼容标准 OpenAI SDK
4. **前端用户**：在 Web 管理界面中管理自己的 Token、查看用量

---

## 2. 架构设计

### 2.1 整体架构

```
┌──────────────────────────────────────────────────────────────────┐
│                          客户端层                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐ │
│  │  API 客户端   │  │  Web 管理界面 │  │ 其他 HTTP 客户端 / SDK   │ │
│  │(Bearer Token)│  │(Session Cookie)│  │(Bearer Token)            │ │
│  └──────┬───────┘  └──────┬───────┘  └────────────┬─────────────┘ │
└─────────┼──────────────────┼───────────────────────┼───────────────┘
          │                  │                       │
          ▼                  ▼                       ▼
┌──────────────────────────────────────────────────────────────────┐
│                       LLM-Bridge 网关                              │
│                                                                    │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │                     HTTP Server (axum)                      │  │
│  │                                                              │  │
│  │  ┌──────────────────┐ ┌──────────────┐ ┌─────────────────┐ │  │
│  │  │  OpenAI 兼容 API  │ │  Auth API    │ │  Admin API      │ │  │
│  │  /v1/models      │ │  /auth/*     │ │  /api/v1/admin/* │ │  │
│  │  /v1/chat/*      │ │              │ │  /api/v1/tokens  │ │  │
│  │  └────────┬─────────┘ └──────┬───────┘ └────────┬────────┘ │  │
│  │           │                  │                   │          │  │
│  │           ▼                  ▼                   ▼          │  │
│  │  ┌──────────────────────────────────────────────────────┐  │  │
│  │  │                   中间件层                             │  │  │
│  │  │  • TokenAuth (Bearer Token 认证 + 权限 + 配额检查)     │  │  │
│  │  │  • SessionAuth (Session Cookie 认证，用于前端 / Admin) │  │  │
│  │  └──────────────────────────────────────────────────────┘  │  │
│  └────────────────────────────────────────────────────────────┘  │
│                              │                                     │
│      ┌───────────────────────┼───────────────────────┐            │
│      ▼                       ▼                       ▼            │
│  ┌──────────────┐  ┌──────────────────┐  ┌────────────────────┐  │
│  │ OIDC Service │  │  Token Service   │  │  Quota Service     │  │
│  │ • 登录/回调  │  │ • Token CRUD     │  │ • 用量实时计数     │  │
│  │ • Session    │  │ • 哈希存储       │  │ • 周期重置         │  │
│  │ • 用户自动   │  │ • 权限校验       │  │ • 超额拦截         │  │
│  │   注册       │  │                  │  │                    │  │
│  └──────┬───────┘  └────────┬─────────┘  └─────────┬──────────┘  │
│         │                   │                      │              │
│         └───────────────────┼──────────────────────┘              │
│                             ▼                                      │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │                   数据层 (toasty + SQLite)                  │  │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐  │  │
│  │  │  users   │ │  tokens  │ │ providers│ │ models   │  │  │
│  │  │          │ │          │ │          │ │          │  │  │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────────┘  │  │
│  │  ┌──────────┐                                              │  │
│  │  │ usage_   │  (用量记录，用于配额追踪)                     │  │
│  │  │ records  │                                              │  │
│  │  └──────────┘                                              │  │
│  └────────────────────────────────────────────────────────────┘  │
│                              │                                     │
│                              ▼                                     │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │                   Actor 层 (ractor)                        │  │
│  │  ┌──────────────────┐  ┌──────────────────────────────┐   │  │
│  │  │GatewayManager    │  │ ProviderActor (每次请求创建)   │   │  │
│  │  │(目录/路由管理)    │  │ (适配器分发)                  │   │  │
│  │  └──────────────────┘  └──────────────────────────────┘   │  │
│  └────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

### 2.2 认证体系

采用**双认证通道**设计：

| 通道 | 认证方式 | 适用场景 | 状态管理 |
|------|---------|---------|---------|
| 用户通道 | OIDC + Session Cookie | Web 前端登录、Token 管理 | 服务端 Session |
| API 通道 | Bearer Token | API 调用（SDK、脚本等） | 无状态，数据库校验 |

**OIDC 流程**（后端实现，仅支持单个 OIDC Provider）：
```
用户 → GET /auth/login → 302 重定向到 OIDC Provider
OIDC Provider → 用户认证 → 302 重定向到 /auth/callback?code=xxx
后端 → 用 code 换 token → 验证 id_token → 查找/创建用户 → 签发 Session Cookie
```

**API Token 流程**：
```
用户（已登录） → POST /api/v1/tokens → 创建 Token → 返回 Token 明文（仅此一次）
客户端 → Authorization: Bearer <token> → /v1/chat/completions
后端 → 校验 Token 哈希 → 检查权限 → 检查配额 → 执行请求 → 扣减用量
```

### 2.3 权限模型

采用 **RBAC + Token Scope** 双层权限：

```
┌─────────────────────────────────────────────────────┐
│                    RBAC (用户层)                     │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────┐  │
│  │  Admin   │  │  Member  │  │ (未来可扩展更多)  │  │
│  │ 管理员    │  │ 普通成员  │  │                  │  │
│  └──────────┘  └──────────┘  └──────────────────┘  │
│       │              │                               │
│       ▼              ▼                               │
│  管理提供者     创建个人 Token                        │
│  查看所有用户   查看自己的 Token                       │
│  管理模型配置   查看自己的用量                         │
│  全局配额管理                                         │
└─────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────┐
│               Token Scope (Token 层)                  │
│  每个 Token 独立配置：                                │
│  • allowed_models: ["openai/gpt-4o", "anthropic/claude-sonnet-4"] │
│  • request_quota: 10000 (周期内最大请求数)            │
│  • token_quota: 5000000 (周期内最大 Token 消耗量)     │
│  • quota_period: "monthly" (重置周期)                 │
└─────────────────────────────────────────────────────┘
```

---

## 3. 数据库设计

### 3.1 使用 toasty（SQLite 驱动）

选择 SQLite 的理由：
- 零配置、嵌入式，符合 homelab 部署场景
- toasty 对 SQLite 有完整支持
- 小团队场景下单文件数据库完全够用

### 3.2 数据模型

**OIDC 配置不存储在数据库中**，而是通过环境变量 / 配置文件提供（见第 7 节）。

```rust
// ── 用户表 ──
#[derive(Debug, toasty::Model)]
#[toasty(table = "users")]
struct User {
    #[key]
    #[auto]
    id: u64,

    /// OIDC subject（唯一标识用户在 IdP 中的身份）
    #[unique]
    oidc_sub: String,

    /// 用户显示名
    name: String,
    /// 邮箱
    email: Option<String>,
    /// 头像 URL
    avatar_url: Option<String>,

    /// 角色：admin / member
    role: String,

    /// 账户是否启用
    active: bool,

    created_at: i64,
    updated_at: i64,

    #[has_many]
    tokens: toasty::HasMany<Token>,
}

// ── API Token 表 ──
#[derive(Debug, toasty::Model)]
#[toasty(table = "tokens")]
struct Token {
    #[key]
    #[auto]
    id: u64,

    #[index]
    user_id: u64,
    #[belongs_to(key = user_id, references = id)]
    user: toasty::BelongsTo<User>,

    /// Token 名称（用户自定义，如 "dev-machine"）
    name: String,

    /// Token 的 bcrypt 哈希值
    token_hash: String,
    /// Token 前缀（用于在 UI 中识别，如 "lb_ab3x..."）
    token_prefix: String,

    /// 允许使用的模型列表（JSON 数组，空表示全部）
    allowed_models: String,

    /// 周期内最大请求数（0 表示不限制）
    request_quota: i64,
    /// 周期内最大 Token 消耗量（0 表示不限制）
    token_quota: i64,
    /// 配额周期：daily / monthly / unlimited
    quota_period: String,

    /// 是否启用
    active: bool,

    created_at: i64,
    last_used_at: Option<i64>,

    #[has_many]
    usage_records: toasty::HasMany<UsageRecord>,
}

// ── 用量记录表（每个 Token 每个周期一条） ──
#[derive(Debug, toasty::Model)]
#[toasty(table = "usage_records")]
struct UsageRecord {
    #[key]
    #[auto]
    id: u64,

    #[index]
    token_id: u64,
    #[belongs_to(key = token_id, references = id)]
    token: toasty::BelongsTo<Token>,

    /// 周期标识（如 "2026-05" 表示 2026 年 5 月）
    period_key: String,

    /// 当前周期已使用请求数
    request_count: i64,
    /// 当前周期已使用 Token 数
    token_count: i64,

    updated_at: i64,
}

// ── 提供者配置表（管理员手动配置或从 models.dev 导入） ──
#[derive(Debug, toasty::Model)]
#[toasty(table = "providers")]
struct Provider {
    #[key]
    #[auto]
    id: u64,

    /// 提供者唯一标识，用于路由（如 "openai", "anthropic", "my-custom"）
    #[unique]
    provider_id: String,
    /// 显示名称
    display_name: String,

    /// AI SDK npm 包名（从 models.dev 导入时填充，用于推导兼容协议）
    npm: Option<String>,

    /// 基础 URL（默认按 AI SDK npm 推导，可覆盖）
    base_url: Option<String>,

    /// API Keys（JSON 数组，支持多 Key 轮询，格式同现有 ApiKeyEntry）
    api_keys: String,

    /// 自定义 HTTP 设置（JSON，对应现有 CompatibilitySettings）
    compat_settings: Option<String>,

    /// 是否启用
    enabled: bool,
    /// 优先级（数字越小优先级越高）
    priority: i64,

    created_at: i64,
    updated_at: i64,

    #[has_many]
    models: toasty::HasMany<ProviderModel>,
}

// ── 提供者模型表 ──
#[derive(Debug, toasty::Model)]
#[toasty(table = "provider_models")]
struct ProviderModel {
    #[key]
    #[auto]
    id: u64,

    /// 关联的提供者（通过 Provider 的自增 id）
    #[index]
    provider_row_id: u64,
    #[belongs_to(key = provider_row_id, references = id)]
    provider: toasty::BelongsTo<Provider>,

    /// 规范化模型名（如 "openai/gpt-4o"），用于 API 路由和 Token allowed_models 匹配
    #[unique]
    model_name: String,
    /// 提供者侧的模型 ID（如 "gpt-4o"）
    provider_model_id: String,

    /// 兼容协议：open_ai_chat_completions / open_ai_responses / anthropic_messages
    /// 从 models.dev 导入时由 npm 自动推导；手动创建时需指定
    compatibility: String,

    /// 显示名称
    display_name: String,
    /// 描述
    description: Option<String>,

    /// ── 能力指标（用于 /v1/models 返回） ──
    max_input_tokens: i64,
    max_output_tokens: i64,
    tool_calling: bool,
    vision: bool,
    thinking: bool,
    adaptive_thinking: bool,

    /// 输入价格（每 1M tokens，美元）
    input_price_per_1m: Option<f64>,
    /// 输出价格（每 1M tokens，美元）
    output_price_per_1m: Option<f64>,
    /// 缓存读取价格
    cache_read_price_per_1m: Option<f64>,

    /// 模型状态
    status: Option<String>,

    /// 是否启用
    enabled: bool,

    created_at: i64,
    updated_at: i64,
}
```

### 3.3 models.dev 的角色

models.dev 仅作为**配置辅助工具**，不直接参与运行时路由：

```
管理员在 Web UI 中：
  1. 浏览 models.dev 目录（通过后端代理请求）
  2. 查看某个提供者（如 openai）下有哪些模型
  3. 点击"导入" → 后端执行：
     a. 创建/更新 Provider 行（provider_id、npm、base_url 从 models.dev 数据推导）
     b. 为每个模型创建 ProviderModel 行（compatibility 由 npm 自动推导）
     c. 模型的能力、定价从 models.dev 数据填充
  4. 手动填写 API Key、调整优先级、启用状态
```

**兼容协议推导规则**（导入时自动，手动创建需显式指定）：

| AI SDK npm 包 | 推导协议 |
|---------------|---------|
| `@ai-sdk/openai` / `@ai-sdk/openai-compatible` | `open_ai_chat_completions` + `open_ai_responses` |
| `@ai-sdk/anthropic` | `anthropic_messages` |
| 其他 / 未知 | 默认 `open_ai_chat_completions` |

运行时路由**完全依赖 `providers` + `provider_models` 两张表**，不再依赖 models.dev。

### 3.4 现有代码处理策略

以下模块将被**直接重写或移除**，不考虑向后兼容：

| 模块 | 处理方式 | 说明 |
|------|---------|------|
| `src/store/` | **完全重写** | JSON 文件 + RwLock 方案废弃，用 toasty Db 直接操作 SQLite |
| `src/config/models.rs` | **大幅重构** | `RuntimeSettings` 保留但扩展 OIDC 配置；`ProviderConfig` / `ProviderCompatConfig` 废弃，改为数据库模型 |
| `src/server/admin.rs` | **完全重写** | 旧 Admin API（Bearer Token 认证）全部移除，新 API 基于 Session + RBAC |
| `src/server/openai_api.rs` | **部分重写** | `/v1/chat/completions` 流程改为 Token 认证 + 配额扣减；`/v1/models` 改为从 SQLite 读取 + Token 过滤 |
| `src/actors/gateway_manager.rs` | **大幅简化** | 移除路由解析、目录初始化、自动注册逻辑，仅保留后台刷新和配额重置 |
| `src/main.rs` | **重构** | 启动流程改为：初始化 toasty → OIDC discovery → 启动 HTTP 服务 |
| `data/llm-bridge/*.json` | **废弃** | 迁移脚本将旧数据导入 SQLite 后删除（或直接丢弃） |

**保留不变的模块**：

| 模块 | 原因 |
|------|------|
| `src/actors/provider/adapters/` | 三个适配器逻辑独立，与认证/存储层解耦，无需改动 |
| `src/actors/provider/mod.rs` | ProviderActor 消息模型保持，仅调用方改变 |
| `src/types.rs` | 通用 LM 类型（消息、角色、响应）不变 |
| `src/models_dev.rs` | models.dev 数据结构仍用于发现功能 |
| `src/observability/` | OpenTelemetry 集成无需改动 |
| `frontend/src/bindings/` | 类型生成机制保留（ts-rs + axfetchum），但生成的类型内容会变 |

### 3.5 Actor 模型简化

GatewayManagerActor **保留但简化**：

| 保留的功能 | 移除的功能 |
|-----------|-----------|
| 后台定时刷新 models.dev 目录（供 UI 发现用） | 模型路由解析（移到 Store 层直接查 SQLite） |
| 配额周期重置后台任务 | 启动时初始化目录（不再阻塞启动） |
| — | 自动注册新发现提供者（改为 UI 手动导入） |

---

## 4. API 设计

### 4.1 Auth API（OIDC — 单 Provider）

| 端点 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/auth/login` | GET | 无 | 发起 OIDC 登录，302 重定向到 IdP |
| `/auth/callback` | GET | 无 | OIDC 回调，验证后签发 Session |
| `/auth/me` | GET | Session | 返回当前登录用户信息 |
| `/auth/logout` | POST | Session | 销毁 Session |

### 4.2 Token 管理 API

| 端点 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/v1/tokens` | GET | Session | 列出当前用户的所有 Token |
| `/api/v1/tokens` | POST | Session | 创建新 Token（返回明文 Token，仅此一次） |
| `/api/v1/tokens/{id}` | DELETE | Session | 删除指定 Token |
| `/api/v1/tokens/{id}` | PATCH | Session | 更新 Token 配置（名称、模型范围、配额） |

**创建 Token 请求体**：
```json
{
    "name": "dev-machine",
    "allowed_models": ["openai/gpt-4o", "anthropic/claude-sonnet-4"],
    "request_quota": 10000,
    "token_quota": 5000000,
    "quota_period": "monthly"
}
```

`allowed_models` 使用**精确匹配**：数组中的每个字符串必须与 `provider_models.model_name` 完全一致。空数组 `[]` 表示允许全部模型。
```

**创建 Token 响应**：
```json
{
    "id": 42,
    "name": "dev-machine",
    "token": "lb_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
    "token_prefix": "lb_x7k2...",
    "allowed_models": ["openai/gpt-4o", "anthropic/claude-sonnet-4"],
    "request_quota": 10000,
    "token_quota": 5000000,
    "quota_period": "monthly",
    "created_at": 1716076800
}
```

### 4.3 OpenAI 兼容 API（Token 认证）

| 端点 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/v1/models` | GET | Bearer Token | **增强版** — 返回 Token 可用的模型 + 完整能力和定价 |
| `/v1/chat/completions` | POST | Bearer Token | 聊天补全，支持流式，自动扣除配额 |

**增强版 `/v1/models` 响应**：
```json
{
    "object": "list",
    "data": [
        {
            "id": "openai/gpt-4o",
            "object": "model",
            "created": 1716076800,
            "owned_by": "openai",
            "capabilities": {
                "max_input_tokens": 128000,
                "max_output_tokens": 16384,
                "tool_calling": true,
                "vision": true,
                "thinking": false,
                "adaptive_thinking": false
            },
            "pricing": {
                "input_per_1m": 2.5,
                "output_per_1m": 10.0,
                "cache_read_per_1m": 1.25
            },
            "description": "GPT-4o — OpenAI's most advanced multimodal model"
        }
    ]
}
```

### 4.4 Admin API（Session 认证，Admin 角色）

| 端点 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/v1/admin/providers` | GET | Session + Admin | 列出所有提供者 |
| `/api/v1/admin/providers` | POST | Session + Admin | 创建提供者 |
| `/api/v1/admin/providers/{id}` | PUT | Session + Admin | 更新提供者配置 |
| `/api/v1/admin/providers/{id}` | DELETE | Session + Admin | 删除提供者 |
| `/api/v1/admin/providers/{id}/models` | GET | Session + Admin | 列出提供者下的模型 |
| `/api/v1/admin/providers/{id}/models` | POST | Session + Admin | 添加模型到提供者 |
| `/api/v1/admin/providers/{id}/models/{model_id}` | DELETE | Session + Admin | 删除模型 |
| `/api/v1/admin/models-dev/search` | GET | Session + Admin | 搜索 models.dev 目录（发现新模型） |
| `/api/v1/admin/models-dev/import` | POST | Session + Admin | 从 models.dev 导入提供者与模型 |
| `/api/v1/admin/users` | GET | Session + Admin | 列出所有用户 |
| `/api/v1/admin/users/{id}/role` | PATCH | Session + Admin | 修改用户角色 |

### 4.5 前端路由规划

| 路由 | 页面 | 认证 |
|------|------|------|
| `/login` | 自动跳转到 OIDC 登录 | 无 |
| `/` | 重定向到 `/models` | Session |
| `/models` | 模型目录浏览（带搜索/筛选/能力展示） | Session |
| `/tokens` | Token 管理（创建/查看/删除/编辑） | Session |
| `/admin/providers` | 提供者管理 + 模型配置 + models.dev 发现导入 | Session + Admin |
| `/admin/users` | 用户管理 | Session + Admin |

---

## 5. 核心服务设计

### 5.1 Token Service

```
TokenService
├── create_token(user_id, config) → (Token, plaintext)
│   • 生成随机 Token（lb_ 前缀 + 256-bit 随机数 + base62 编码）
│   • bcrypt 哈希存储
│   • 返回明文 Token（仅此一次）
├── validate_token(plaintext) → Option<Token>
│   • 从数据库加载所有 Token
│   • bcrypt 验证
│   • 检查 Token 是否激活
├── check_model_access(token, model_name) → bool
│   • 检查 allowed_models JSON 数组中是否精确包含该 model_name
│   • 空数组表示允许全部模型
├── check_quota(token, usage_record) → bool
│   • 检查 request_count < request_quota
│   • 检查 token_count < token_quota
└── record_usage(token, request_count_delta, token_count_delta)
    • 更新当前周期的 UsageRecord
    • 更新 Token 的 last_used_at
```

### 5.2 Quota Service

```
QuotaService
├── get_current_period_key() → String
│   • "daily"   → "2026-05-18"
│   • "monthly" → "2026-05"
│   • "unlimited" → "unlimited"
├── get_or_create_usage_record(token_id, period_key) → UsageRecord
├── check_and_deduct(token, estimated_tokens) → Result
│   • 原子性检查 + 扣减（事务）
└── reset_cycle_task() (后台任务)
    • 每小时检查一次是否有需要重置的周期
    • 为新周期创建空的 UsageRecord
```

### 5.3 OIDC Service

```
OidcService
├── login() → Redirect
│   • 构造 OIDC authorization URL
│   • 生成 state（防 CSRF）
│   • 生成 nonce
│   • 存入 Session
├── callback(code, state) → User + Session
│   • 验证 state
│   • 用 code 交换 token
│   • 验证 id_token（签名、issuer、audience、nonce）
│   • 解析 claims（sub, email, name, picture）
│   • 查找或创建 User
│   • 发放 Session Cookie
└── discover() → OidcMetadata
    • 请求 .well-known/openid-configuration
    • 启动时执行一次，缓存结果
```

### 5.4 管理员首任机制

```
启动时检查:
  if users 表为空:
      第一个通过 OIDC 登录的用户 → 自动赋予 admin 角色
      日志记录: "first user {email} promoted to admin"

后续:
  仅 admin 可以通过 PATCH /api/v1/admin/users/{id}/role 修改其他用户的角色
```

---

## 6. 实施计划

### Phase 1：基础设施（数据库 + OIDC）

| 任务 | 预估工时 | 产出 |
|------|---------|------|
| 1.1 引入 toasty 依赖，配置 SQLite | 2h | `Cargo.toml` 更新，`db.rs` 模块 |
| 1.2 定义数据模型（User, Token, UsageRecord, Provider, ProviderModel） | 4h | `src/db/models.rs` |
| 1.3 实现数据库初始化与迁移 | 2h | `src/db/mod.rs` + 自动建表 |
| 1.4 实现 OIDC Service（discover + login + callback，单 Provider） | 5h | `src/auth/oidc.rs` |
| 1.5 实现 Session 管理（Cookie + 内存存储） | 2h | `src/auth/session.rs` |
| 1.6 实现 Auth API 端点（login/callback/me/logout） | 3h | `src/server/auth.rs` |

**预估总计：18h**

### Phase 2：Token 体系 + 配额

| 任务 | 预估工时 | 产出 |
|------|---------|------|
| 2.1 实现 Token Service（创建/验证/CRUD，无过期逻辑） | 5h | `src/auth/token.rs` |
| 2.2 实现 Quota Service（计数+重置+检查） | 4h | `src/auth/quota.rs` |
| 2.3 实现 Token 管理 API（CRUD） | 4h | `src/server/tokens.rs` |
| 2.4 实现配额后台重置任务 | 3h | `src/auth/quota_reset.rs` |
| 2.5 实现 Token 认证中间件 | 3h | `src/middleware/token_auth.rs` |
| 2.6 实现 Session 认证中间件 | 2h | `src/middleware/session_auth.rs` |
| 2.7 管理员自动提升逻辑 | 1h | 在 OIDC callback 中 |

**预估总计：23h**

### Phase 3：提供者与模型存储 + 改造 API

| 任务 | 预估工时 | 产出 |
|------|---------|------|
| 3.1 重写 Store 层：用 toasty 操作 providers / provider_models 表 | 5h | `src/store/mod.rs` 重写 |
| 3.2 实现兼容协议推导（npm → compatibility）和 models.dev 数据映射 | 3h | `src/store/compat.rs` |
| 3.3 实现 models.dev 代理与导入端点（浏览 + 搜索 + 导入） | 4h | `src/server/models_dev_proxy.rs` |
| 3.4 重写路由解析：从 provider_models 表读取，构造 ResolvedProviderRoute | 3h | `src/store/router.rs` |
| 3.5 增强 `/v1/models`：基于 Token 的 allowed_models 过滤 + 返回完整能力+定价 | 3h | `src/server/openai_api.rs` 改造 |
| 3.6 改造 `/v1/chat/completions` 接入 Token 认证和配额扣减 | 4h | 中间件集成 |
| 3.7 移除旧的 Bearer Token Admin 认证 + 清理废弃代码 | 2h | 全局清理 |

**预估总计：24h**

### Phase 4：Admin API + 前端

| 任务 | 预估工时 | 产出 |
|------|---------|------|
| 4.1 实现 Admin API（提供者/模型 CRUD + models.dev 代理） | 6h | `src/server/admin.rs` 扩展 |
| 4.2 前端：登录页面 + OIDC 回调处理 | 3h | `LoginPage.svelte` |
| 4.3 前端：Token 管理页面 | 6h | `TokensPage.svelte` |
| 4.4 前端：适配现有模型浏览页面到新数据源 | 3h | 更新 ModelsPage.svelte |
| 4.5 前端：Admin 页面（提供者管理 + 模型配置 + models.dev 发现导入 + 用户管理） | 8h | Admin 相关页面 |

**预估总计：26h**

### Phase 5：测试与文档

| 任务 | 预估工时 | 产出 |
|------|---------|------|
| 5.1 集成测试（OIDC mock + Token CRUD + API 调用） | 6h | `tests/` |
| 5.2 更新架构文档 | 3h | `docs/architecture.md` |
| 5.3 更新 Admin API 文档 | 2h | `docs/admin-api.md` |
| 5.4 编写部署文档 | 2h | `docs/deployment.md` |

**预估总计：13h**

**全部预估：~96h**

---

## 7. 依赖变更

### 新增依赖

```toml
# 数据库 ORM
toasty = { version = "0.6", features = ["sqlite"] }

# OIDC 客户端
openidconnect = "4"       # OIDC 协议实现

# Session 管理
tower-sessions = "0.15"   # axum session 中间件

# Token 哈希
bcrypt = "0.17"           # API Token 哈希

# 随机数生成
rand = "0.9"
```

### OIDC 环境变量配置

OIDC 通过环境变量配置（单 Provider，不需要数据库表）：

| 变量 | 说明 |
|------|------|
| `LLM_BRIDGE_OIDC_ISSUER_URL` | OIDC issuer URL（如 `https://gitlab.example.com`） |
| `LLM_BRIDGE_OIDC_CLIENT_ID` | OIDC client_id |
| `LLM_BRIDGE_OIDC_CLIENT_SECRET` | OIDC client_secret |
| `LLM_BRIDGE_OIDC_SCOPES` | 请求的 scopes（默认 `openid profile email`） |
| `LLM_BRIDGE_BASE_URL` | 网关自身的访问地址（用于构造回调 URL） |

### 保留的现有依赖

```toml
# 以下依赖在新架构中继续使用，无需变更
axum = { version = "0.8", features = ["ws"] }
ractor = "0.15"
reqwest = "0.13"
serde / serde_json
tokio
tracing / opentelemetry
ts-rs / axfetchum  # 前端类型生成，保留
```

### 移除的代码与依赖

```toml
# 移除：旧 Admin API Bearer Token 认证逻辑完全删除
# 移除：catalog_cache.json / providers.json 的读写逻辑
# 移除：RwLock 包裹的内存缓存层
# 移除：npm_to_compatibilities 推导逻辑（移到导入时处理）
# 移除：replace_catalog 自动注册提供者逻辑
```

---

## 8. 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| toasty 库不够成熟（v0.6） | API 变动频繁 | 封装一层 Repository trait，隔离 ORM 细节 |
| SQLite 并发写入瓶颈 | 高并发场景性能下降 | 小团队场景写入极少；后期可切换 PostgreSQL |
| OIDC Provider 兼容性问题 | 特定 IdP 无法对接 | 实现 OIDC Discovery 标准流程；测试主流 IdP（Authentik / Keycloak / GitLab） |
| bcrypt 验证延迟 | 每次 API 请求需验证 Token | Token 验证结果短时间缓存（内存 LRU） |
| 配额重置任务与请求并发 | 计数不准确 | 使用 SQLite 事务 + 乐观锁 |

---

## 9. 开放问题（待讨论）

1. **同邮箱多 OIDC 登录**：如果用户换了 IdP 中绑定的邮箱，系统按 `oidc_sub` 识别用户，不受邮箱变化影响。

2. **配额超额后的行为**：硬拦截还是软限制（允许一定超额）？建议：硬拦截，返回 429 `quota_exceeded`，包含重置时间信息。

3. **API Key 存储加密**：`providers` 表中的 `api_keys` 目前明文存储（JSON），是否需要加密？建议：Phase 1 先明文，Phase 5 加入可选的加密层。

4. **models.dev 数据更新**：管理员从 models.dev 导入的模型数据可能过时（如价格变动）。是否需要定期自动同步已导入的模型？建议：初期仅手动刷新，后续可加入后台定时同步。
