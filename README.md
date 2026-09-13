# LLM-Bridge

> 一个面向 **Homelab 与小型工作室** 的自托管 LLM 网关。
> 统一路由多个上游 LLM 提供者的 API 请求，兼容 OpenAI 客户端协议，
> 支持 OpenAI / Anthropic 等多种上游协议，内置 OIDC 登录、Token 配额、管理 UI。
>
> 网关侧 WS 与设备码协议面向插件开发者；本仓库不包含可安装的 VS Code 插件。

[![Rust](https://img.shields.io/badge/Rust-nightly-orange.svg)](./rust-toolchain.toml)
[![Edition 2024](https://img.shields.io/badge/Edition-2024-blue.svg)](https://blog.rust-lang.org/2025/02/20/Rust-2024.html)
[![License: BSD 3-Clause](https://img.shields.io/badge/License-BSD%203--Clause-blue.svg)](./LICENSE)

---

## 目录

- [为什么选择 LLM-Bridge](#为什么选择-llm-bridge)
- [简介](#简介)
- [核心特性](#核心特性)
- [系统架构](#系统架构)
- [技术栈](#技术栈)
- [快速开始](#快速开始)
- [配置](#配置)
- [数据模型](#数据模型)
- [API 参考](#api-参考)
- [前端管理界面](#前端管理界面)
- [支持的上游协议](#支持的上游协议)
- [项目结构](#项目结构)
- [开发指南](#开发指南)
- [示例](#示例)
- [可观测性](#可观测性)
- [VS Code Copilot 插件](#vs-code-copilot-插件规划中)
- [项目状态](#项目状态)

---

## 为什么选择 LLM-Bridge

适用于希望集中配置上游 API Key、统一模型入口并查看自身用量的 Homelab 或小型工作室。管理员维护模型和 Provider；成员使用个人 Bearer Token 访问模型。

**治理边界：Token 配额和模型白名单是用户可修改的个人自限，不是管理员强制执行的团队预算。** 用户可以创建无限额、全模型 Token。需要强制预算的部署应另行定义用户级治理策略，不能把当前 Token 配额作为安全边界。

本仓库未提供可安装的 VS Code 插件，也没有经当前版本测量的内存、二进制大小、ARM 性能或第三方网关横向基准。

---

## 简介

LLM-Bridge 是一个用 Rust 编写的 **LLM 网关**，它的目标类似于一个自托管的 OpenRouter——但更轻、更小、更专注于 Homelab 场景。

项目采用 Rust 后端与 Vue 管理界面，聚焦自托管模型路由、个人凭据、用量查询和协议接入，不提供完整企业治理或模型评测管线。

**适用场景：**
- **Homelab** — 集中维护提供者与模型，用户通过个人 Token 调用。
- **小型工作室** — 集中保管上游 Key，成员用量隔离；Token 自限不等于团队强制预算。
- **插件开发者** — 使用网关接入协议自行实现模型注册、连接管理和凭据保存；插件本体另行交付。

核心能力：

- 对客户端暴露 **OpenAI 兼容接口**（`/v1/models`、`/v1/chat/completions`）；支持范围见下文，不宣称完整兼容全部 OpenAI/OpenRouter 字段。
- 在网关内部，将请求**路由到多个上游 LLM 提供者**（OpenAI、Anthropic、其它兼容服务），并支持按优先级回退
- 每个提供者可声明**多种协议端点**（如一个 Provider 既能走 `OpenAIChatCompletions` 又能走 `AnthropicMessages`），多组 API Key 跨协议共享并加权轮询
- 提供 **OIDC 单点登录、API Token、配额管理**与一套**可视化管理界面**，让你像运维一个内部 API 平台一样管理 LLM 访问

资源占用、二进制体积和 ARM 交付尚无当前版本的测量结论。实际验收命令、范围及限制以 [STATUS.md](STATUS.md) 为准，不能由代码存在推导生产可用性。

---

## 核心特性

### 🎯 统一入口

| 能力 | 说明 |
|------|------|
| **OpenAI 兼容 API** | `GET /v1/models`、`POST /v1/chat/completions`，流式 SSE + 非流式，透传 `reasoning_content` |
| **多协议上游适配** | 内置 OpenAI Chat / Responses、Anthropic Messages 三种适配器，支持自定义 base URL 与 headers |
| **多协议架构** | `ProviderProtocol` 表让一个 Provider 承载多个协议端点，路由解析走四表关联 |
| **优先级路由与回退** | LLMModel → ModelProvider → ProviderProtocol → Provider 四级解析，失败自动回退 |
| **加权 API Key 轮询** | 每个 Provider 持有多把 Key，按 `weight` 加权轮询，跨协议共享 |

### 🔐 多租户

| 能力 | 说明 |
|------|------|
| **OIDC 单点登录** | 任何兼容 OIDC 的 IdP（Keycloak、Authentik、Google…）均可作为登录后端 |
| **API Token 体系** | 用户可创建多个 `lb_` 前缀 Token，每 Token 独立配额与模型权限范围 |
| **配额管理** | daily / monthly / unlimited 周期的个人 Token 自限；周期账本保留历史，不是管理员强制预算 |
| **RBAC 管理 UI** | Vue 3 单页应用：模型目录、Token、Provider/协议、用户角色和用量追踪 |

### 🚀 性能与运维

| 能力 | 说明 |
|------|------|
| **原生后端** | Rust 原生二进制；资源指标需在实际部署环境测量 |
| **单二进制部署** | `embed-frontend` feature 把前端嵌入二进制，一条命令即可在 NAS、VPS 上运行 |
| **可观测性** | OpenTelemetry traces + logs → OTLP，每个 Actor 消息 `#[instrument]` 追踪 |
| **类型契约** | `ts-rs` 导出 DTO，`axfetchum` 生成客户端；类型漂移检查独立于生成操作 |
| **插件接入** | 网关提供协议基础，不代表可安装 VS Code 扩展已交付 |

---

## 系统架构

```mermaid
flowchart TB
    subgraph Client[客户端]
        SDK[OpenAI SDK / 任意兼容客户端]
        UI[Web 管理界面]
    end

    subgraph Bridge[LLM-Bridge 网关]
        direction TB
        HTTP[HTTP Server<br/>axum]

        subgraph Auth[认证层]
            OIDC[OIDC SSO]
            SESSION[Session Cookie<br/>SessionAuth / AdminAuth]
            TOKEN[API Token<br/>TokenAuth + bcrypt]
        end

        subgraph Actors[Actor 层 - ractor]
            GW[GatewayManagerActor<br/>配额重置 / 路由解析]
            PA[ProviderActor<br/>请求分发到适配器]
        end

        subgraph Adapters[协议适配器]
            A1[OpenAI Chat Completions]
            A2[OpenAI Responses]
            A3[Anthropic Messages]
        end

        STORE[(Store<br/>四表关联路由)]
        DB[(SQLite / PostgreSQL<br/>toasty ORM)]
    end

    subgraph Upstream[上游 LLM 提供者]
        OAI[OpenAI]
        ANT[Anthropic]
        OTH[其它兼容服务]
    end

    SDK -->|/v1/chat/completions<br/>Bearer Token| HTTP
    UI -->|/api/v1/admin/*<br/>Session Cookie| HTTP
    HTTP --> Auth
    Auth --> GW
    GW --> STORE
    STORE --> DB
    GW --> PA
    PA --> Adapters
    Adapters -->|SSE 流式| OAI
    Adapters -->|SSE 流式| ANT
    Adapters -->|SSE 流式| OTH
```

---

## 技术栈

### 后端（Rust，Edition 2024 / Nightly）

| 类别 | 依赖 |
|------|------|
| Web 框架 | `axum` 0.8 |
| 异步运行时 | `tokio`（multi-thread） |
| Actor 框架 | `ractor` 0.16 |
| HTTP 客户端 | `reqwest` 0.13（rustls + stream） |
| 数据库 ORM | `toasty` 0.10；SQLite 默认，PostgreSQL 需要对应 feature 与数据库 URL |
| 时间 | `jiff` 0.2 |
| 认证 | `openidconnect` 4.0、`tower-sessions` 0.15、`bcrypt` 0.19 |
| 可观测性 | `opentelemetry` 0.32、`tracing`、`tracing-subscriber` |
| 类型生成 | `ts-rs` 12、`axfetchum` 0.1 |
| 序列化 | `serde` / `serde_json` |
| 错误处理 | `thiserror` 2.0 |

### 前端（TypeScript）

| 类别 | 依赖 |
|------|------|
| 框架 | Vue 3 + TypeScript |
| 构建 | Vite 8 + vue-tsc |
| 样式 | Tailwind CSS 4 + reka-ui |
| 路由 | Vue Router 5 |
| 状态 | Pinia 3 |
| 包管理 | pnpm |

---

## 快速开始

### 前置要求

- **Rust nightly**（项目附带 `rust-toolchain.toml`，首次进入目录会自动安装）
- **Node.js 与 pnpm**（仅前端开发或构建完整应用需要；纯后端检查、测试与运行不需要）
- **OIDC IdP** 可选。未配置 OIDC，或 Discovery 失败时，按既定设计进入免登录管理员模式；必须通过可信网络或反向代理限制管理入口。

### 1. 克隆并构建后端

```bash
git clone <repo-url> llm-bridge
cd llm-bridge

# 调试构建
cargo build

# 默认仅构建后端；完整生产构建使用 cargo xtask build。
```

### 2. 启动全栈开发环境

```bash
pnpm --dir frontend install --frozen-lockfile
cargo xtask dev
# → 浏览器访问 http://127.0.0.1:5173
```

`xtask` 独立管理 Vite 和后端：Rust 输入变化后自动重建并重启后端，前端变化使用原生 HMR，不重启 Vite。编译失败时保留监控，修复后自动重试；服务意外退出时清理其他进程并报错。启动器不隐式安装前端依赖。

开发端口可通过 `LLM_BRIDGE_PORT=3000`、`LLM_BRIDGE_UI_PORT=5173` 设置；后端监听地址沿用 `LLM_BRIDGE_HOST`，Vite 只监听 `127.0.0.1`。端口冲突会报错，不自动改用其他端口。Vite 代理 `/api`、`/auth`、`/v1`，包括业务 WebSocket 和 SSE。

开发时未显式设置的 `LLM_BRIDGE_BASE_URL` 自动设为浏览器入口（默认 `http://127.0.0.1:5173`）；自定义反代地址会被保留。OIDC IdP 的回调地址应为该公开地址加 `/auth/callback`，不要混用 `localhost` 与 `127.0.0.1`。Unix 上 Ctrl-C/SIGTERM 会停止进程组，后端最多有 10 秒优雅退出时间，随后强制清理；Windows 使用 Job Object 清理进程树。

### 3. 启动网关

```bash
# 仅启动后端 API（不启动 Vite、不提供管理页面）
cargo run --bin llm-bridge

# 带 OIDC 与前端嵌入的单二进制启动
pnpm --dir frontend install --frozen-lockfile
cargo xtask build
LLM_BRIDGE_OIDC_ISSUER_URL=https://idp.example.com \
LLM_BRIDGE_OIDC_CLIENT_ID=llm-bridge \
LLM_BRIDGE_OIDC_CLIENT_SECRET=... \
./target/release/llm-bridge
```

后端默认监听 `http://127.0.0.1:3000`。默认 Cargo features 为空；完整应用使用 `cargo xtask build --features otel`，需要 PostgreSQL 时使用 `--features otel,postgresql`。启动器先执行 `pnpm run build`，成功后才编译带 `embed-frontend` 的后端。`cargo build --features embed-frontend` 仍可消费预先构建的 `frontend/dist/`；构建脚本仅检查入口并追踪目录变化，不启动 Node 或下载依赖。

---

## 配置

所有配置通过环境变量传递，无额外配置文件。

### 通用配置

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `LLM_BRIDGE_GATEWAY_ID` | `llm-bridge-v1` | 网关标识，用于日志追踪 |
| `LLM_BRIDGE_HOST` | `127.0.0.1` | 监听地址 |
| `LLM_BRIDGE_PORT` | `3000` | 监听端口 |
| `LLM_BRIDGE_STORE_PATH` | `./data/` | SQLite 目录，自动创建；数据库文件为该目录下的 `sqlite.db` |
| `LLM_BRIDGE_DATABASE_URL` | 未设置 | 非空时优先于 STORE_PATH；支持 `sqlite:/path/db`、`sqlite::memory:` 与 `postgresql://user:pass@host/db`（需 postgresql feature） |
| `LLM_BRIDGE_MODELS_IMPORT_URL` | `https://moheng233.github.io/llm-bridge/catalog.json` | 管理员手动预览/导入使用的目录来源 |
| `RUST_LOG` | `info` | 日志级别（`tracing-subscriber` env-filter） |

### OIDC 配置

配置 `LLM_BRIDGE_OIDC_ISSUER_URL` 时尝试启用 OIDC。未配置或 Discovery 失败会继续进入免登录管理员模式，这是有意设计，不是拒绝启动策略。管理功能对可访问服务的用户开放；`/v1/models`、`/v1/chat/completions` 与 `/v1/ws` 始终需要 Bearer Token。部署者须提供与该信任模型相符的访问边界。

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `LLM_BRIDGE_OIDC_ISSUER_URL` | 无（禁用 OIDC） | IdP 的 Issuer URL |
| `LLM_BRIDGE_OIDC_CLIENT_ID` | 空 | OIDC Client ID |
| `LLM_BRIDGE_OIDC_CLIENT_SECRET` | 空 | OIDC Client Secret |
| `LLM_BRIDGE_OIDC_SCOPES` | `openid profile email` | 申请的 scopes |
| `LLM_BRIDGE_BASE_URL` | 后端独立运行：`http://localhost:3000`；`xtask dev`：Vite 入口 | 对外可访问地址，用于 OIDC 回调；显式设置优先 |

> 首个通过 OIDC 登录的用户会自动获得 `Admin` 角色，后续用户为 `Member`。

### Cargo Features

| Feature | 说明 |
|---------|------|
| `embed-frontend` | 启用 `rust-embed`，把 `frontend/dist` 嵌入后端二进制，适合单文件部署 |
| `otel` | 启用 OpenTelemetry traces、logs 和 GenAI metrics 的 OTLP HTTP 导出 |
| `postgresql` | 编译 PostgreSQL 驱动；仍需配置 PostgreSQL 连接 URL，feature 本身不会选择数据库 |

默认不启用任何 feature：普通 `cargo check/test/run` 与 Node、Vite 和 `frontend/dist` 解耦。

---

## 数据模型

路由核心由 `LLMModel`、`Provider`、`ProviderProtocol`、`ModelProvider` 四表组成。用户、Token、周期账本及可观测性表的完整注册以 `src/db/mod.rs` 为准；升级策略和验收见 [STATUS.md](STATUS.md)。

```mermaid
erDiagram
    LLMModel ||--o{ ModelProvider : "has"
    Provider ||--o{ ProviderProtocol : "supports"
    ProviderProtocol ||--o{ ModelProvider : "used by"
    User ||--o{ Token : "owns"
    User ||--o{ UsageRecord : "generates"
    Token ||--o{ UsageRecord : "tracks"

    LLMModel {
        u64 id PK
        string model_name UK "如 openai/gpt-4o"
        string display_name
        u32 max_input_tokens
        u32 max_output_tokens
        bool tool_calling
        bool vision
        bool thinking
    }
    Provider {
        u64 id PK
        string display_name
        json api_keys "ApiKeyEntry 数组"
        bool enabled
        i64 priority
    }
    ProviderProtocol {
        u64 id PK
        u64 provider_id FK
        enum protocol "OpenAI Chat / Responses / Anthropic"
        string base_url "必填"
        string compat_settings "JSON"
        bool enabled
        i64 priority
    }
    ModelProvider {
        u64 id PK
        u64 model_id FK
        u64 protocol_id FK
        string provider_model_name
        bool enabled
        i64 priority
    }
    Token {
        u64 id PK
        u64 user_id FK
        string name
        string key_hash "bcrypt"
        string prefix "lb_..."
        json quota
        json allowed_models
    }
```

**路由解析流程**：

```
resolve_model(model_name)
  → LLMModel（按 model_name 查询）
  → ModelProvider[]（按 model_id，enabled 过滤，priority 排序）
      → ProviderProtocol（按 protocol_id）→ base_url / compatibility
      → Provider（按 provider_id）→ api_keys（KeySelector 加权轮询）
  → 构建 ResolvedProviderRoute 列表，按 priority fallback
```

---

## API 参考

### OpenAI 兼容接口（客户端使用）

需要 `Authorization: Bearer <api_token>` 头部，Token 通过管理界面创建（`lb_` 前缀）。

| 端点 | 方法 | 说明 |
|------|------|------|
| `/v1/models` | GET | 返回所有可用模型（含能力、定价、各提供者信息） |
| `/v1/chat/completions` | POST | 聊天补全，支持 `stream: true`（SSE） |

支持 `system` / `developer` / `user` / `assistant` / `tool` 消息、文本与图片输入、流式和非流式回复、reasoning 与工具调用。多候选 `n`、logprobs、OpenRouter provider 路由偏好、扩展采样和音视频输出等低频字段不在基础兼容承诺中；完整差距见 [PLAN.md §3](PLAN.md#3-后端v1chatcompletions-openrouter-兼容性差距剩余)。

### Auth API

| 端点 | 方法 | 说明 |
|------|------|------|
| `/auth/login` | GET | 触发 OIDC 跳转 |
| `/auth/callback` | GET | OIDC 回调，建立 Session |
| `/auth/me` | GET | 返回当前登录用户信息 |
| `/auth/logout` | POST | 销毁 Session |

### Token 管理 API

| 端点 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/v1/tokens` | GET | Session | 列出当前用户的 Token |
| `/api/v1/tokens` | POST | Session | 创建 Token（明文仅返回一次） |
| `/api/v1/tokens/{id}` | PATCH / DELETE | Session | 更新 / 删除 Token |

### Admin API

`/api/v1/admin/*` 需要管理员 Session；模型浏览 `/api/v1/models*` 只需要普通 Session。免登录模式按上述可信网络设计注入管理员身份。

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/v1/models` | GET | 列出全部模型 |
| `/api/v1/models/available` | GET | 仅列出已绑定启用提供者的模型 |
| `/api/v1/admin/providers` | GET / POST | 列出 / 创建 Provider |
| `/api/v1/admin/providers/{id}` | GET / PUT / DELETE | Provider 单条 CRUD |
| `/api/v1/admin/providers/{id}/models` | GET / POST | Provider 下的模型关联 |
| `/api/v1/admin/providers/{id}/models/{mid}` | PUT / DELETE | 更新 / 删除关联 |
| `/api/v1/admin/providers/{id}/protocols` | GET / PUT | 查看 / 全量替换提供者协议列表 |
| `/api/v1/admin/model-connections` | POST | 原子保存本批新模型定义与连接；已存在连接原样复用 |
| `/api/v1/admin/models-import/preview` | GET | 只读目录预览与本地匹配；仅创建时预填 |
| `/api/v1/admin/users` | GET | 列出所有用户 |
| `/api/v1/admin/users/{id}/role` | PATCH | 修改用户角色 |

> 完整的字段格式请参考 [`docs/admin-api.md`](./docs/admin-api.md)。

---

## 前端管理界面

Vue 3 单页应用位于 `frontend/`，使用 Vue Router、Pinia、Tailwind CSS 与 reka-ui。开发服务器与生产嵌入机制见部署说明。

| 页面 | 路由 | 权限 | 功能 |
|------|------|------|------|
| 登录 | `/login` | 无 | 触发 OIDC 跳转 |
| 概览 | `/` `/dashboard` | Session | 管理员切换全站/个人指标，成员仅本人；零数据图表、趋势与同范围最近请求 |
| 使用模型 | `/models` | Session | 本地模型搜索、能力/可路由筛选、未知价格与客户端接入说明 |
| 访问令牌 | `/tokens` | Session | 个人 Token、模型范围与配额自限；一次性明文展示 |
| 请求记录 | `/traces` `/traces/:id` | Session | 按角色隔离的筛选、分页、生命周期与 Opt-In 内容快照 |
| 模型定义 | `/admin/models` `/admin/models/new` `/admin/models/:id` | Admin | 创建与维护共享标称定义、独立连接覆盖及手动上游检测 |
| 提供者 | `/providers` `/providers/:id` | Admin | 接入实例列表、连接与协议/凭据的单次保存 |
| 接入模型 | `/admin/setup` | Admin | 目录只读预填或手动配置；提供者与批量模型连接两个事务检查点 |
| 用户 | `/users` | Admin | 角色变更确认；修改自身角色后刷新权限 |

侧边栏按使用与管理任务分组；管理入口仅 Admin 可见。未认证访问受保护路由会跳转到 `/login`。`Ctrl+K` / `Cmd+K` 搜索页面与本地模型。目录只在首次创建时预填，保存后独立维护，不同步或覆盖；客户端使用网关模型 ID 和个人访问令牌，不使用上游 Key。手动检测仅表示本浏览会话最近结果，不是实时健康监控。

---

## 支持的上游协议

| 协议 | 上游示例 | 特性 |
|------|---------|------|
| `OpenAiChatCompletions` | OpenAI `/v1/chat/completions` | 流式 SSE、自定义 base URL、自定义 headers |
| `OpenAiResponses` | OpenAI `/v1/responses` | 流式 SSE、Responses API 格式 |
| `AnthropicMessages` | Anthropic `/v1/messages` | 流式 SSE、Thinking 内容、工具调用 |

每个适配器均支持：自定义 base URL + path suffix、自定义 HTTP headers（`compat_settings`）、错误消息提取与转发、Thinking/Reasoning 内容传输、工具调用结果传递。

新增协议只需在 `ProviderCompatibility` 枚举中追加变体并在 `src/actors/provider/adapters/` 下实现一个新适配器模块。

---

## 项目结构

```
llm-bridge/
├── Cargo.toml                  # 依赖与 features 定义
├── rust-toolchain.toml         # 锁定 nightly + clippy + rustfmt
├── PLAN.md                     # 多协议架构重设计计划
├── STATUS.md                   # 项目现状与进度报告
├── docs/                       # 架构与 API 文档
├── examples/                   # 命令行端到端示例
│   ├── openai_stream_cli.rs
│   └── anthropic_stream_cli.rs
├── frontend/                   # Vue 3 管理界面
│   └── src/
│       ├── pages/              # 路由页面
│       ├── components/         # 共享 UI 与业务组件
│       ├── lib/                # API 入口与工具函数
│       └── bindings/           # ts-rs / axfetchum 自动生成的 TS 类型与客户端
└── src/
    ├── main.rs                 # 入口：可观测性 → 配置 → HTTP 服务
    ├── lib.rs
    ├── types.rs                # 通用 LM 类型（消息、角色、响应、工具调用）
    ├── config/                 # RuntimeSettings、ProviderCompatibility 枚举
    ├── db/                     # toasty ORM 模型、schema 初始化与升级
    ├── auth/                   # OIDC / Session / Token / Quota 服务
    ├── middleware/             # SessionAuth / AdminAuth / TokenAuth 提取器
    ├── store/                  # Store 层：CRUD、四表路由解析、KeySelector
    ├── actors/                 # ractor Actor：GatewayManager、Provider + 适配器
    ├── server/                 # axum 路由：openai_api / admin / auth / tokens
    └── observability/          # OpenTelemetry 初始化
```

---

## 开发指南

### 后端

```bash
# 编译检查
cargo check

# Lint（项目要求 clippy 零警告）
cargo clippy --workspace --all-targets --locked -- -D warnings

# 行为测试不改写 TS 绑定
cargo test -p llm-bridge --all-targets --locked -- --skip export_bindings

# 运行示例（端到端连通性测试）
cargo run --example openai_stream_cli -- <url> <api_key> <model>
```

### 开发编排

```bash
cargo xtask --help                      # 全部命令与选项
cargo xtask help build                  # 子命令帮助（也可用 build --help）
cargo xtask dev                         # Vite + 自动重建后端
cargo xtask dev --features otel         # 启用额外后端能力
cargo xtask build                       # 前端 + release 嵌入式后端
cargo xtask build --debug               # 前端 + dev profile 后端
cargo xtask build --target <triple>     # 前端 + 指定目标后端（需预装工具链/链接器）
cargo test -p xtask --locked            # 启动器的边界回归
```

参数解析采用 [clap derive](https://docs.rs/clap/latest/clap/_derive/_tutorial/index.html)，自动生成帮助与参数错误信息。`--features` 支持逗号或引号内的空格分隔，也可重复传入；例如 `--features "otel postgresql"` 或 `--features=otel --features=postgresql`。开发模式仍禁止启用 `embed-frontend`。

监控范围是 `src/`、根 Cargo manifest/lock、`build.rs`、`.cargo/config.toml` 和 `rust-toolchain.toml`，不监控 `target/`、`frontend/` 或数据库目录。修改 `xtask` 本身后需重新运行开发命令。前后端类型生成是显式任务，不在文件监控中自动改写绑定。

### 前端

```bash
cd frontend
pnpm install --frozen-lockfile
pnpm run dev       # 单独启动 Vite；另开终端运行后端
pnpm run build     # 生产构建到 frontend/dist
pnpm run lint      # 检查手写前端代码
pnpm exec vue-tsc -b # 类型检查；干净环境请先执行完整 build 生成自动声明
```

### 同步 TypeScript 绑定

后端类型变更后，需要重新生成前端的 TypeScript 绑定（位于 `frontend/src/bindings/`）：

```bash
# 显式生成 ts-rs 类型与 axfetchum API client
cargo xtask bindings

# 检查类型和客户端漂移，不改写仓库绑定（需 python3）
cargo xtask bindings --check
```

> `ts-rs` 负责数据类型文件，`axfetchum` 负责根据后端路由声明生成 API 客户端。两者双通道保持前后端类型一致。

`src/bindings/` 是生成产物，不手工改写，也不套用手写代码的 oxlint 风格规则；仍参加 TypeScript 编译与上述漂移检查。CI 分别检查手写前端 lint、类型/客户端一致性及行为合约。

### 单二进制部署

```bash
# 显式安装依赖，然后统一构建前端与 release 二进制
pnpm --dir frontend install --frozen-lockfile
cargo xtask build --features otel

# 部署只需一个可执行文件 + SQLite 数据目录
./target/release/llm-bridge
```

### Docker 部署

```bash
docker build -f Dockerfile.base -t localhost/llm-bridge-base:latest .
docker build -t localhost/llm-bridge:latest .
docker run --rm -p 127.0.0.1:3000:3000 \
  -v llm-bridge-data:/data localhost/llm-bridge:latest
```

镜像包含 `embed-frontend,otel,postgresql`，以非 root 用户运行，默认数据库为 `/data/sqlite.db`。挂载整个 `/data`；bind mount 需预先授予容器用户写权限。未配置 OIDC 的管理接口按可信网络设计开放，不应直接暴露到公网。

### PostgreSQL 部署

数据库需预先创建。原生构建启用 `postgresql`；上述 Docker 镜像已包含驱动：

```bash
cargo xtask build --features otel,postgresql
LLM_BRIDGE_DATABASE_URL='postgresql://llm_bridge:PASSWORD@127.0.0.1/llm_bridge' \
  ./target/release/llm-bridge
```

非空 URL 优先于 SQLite 目录配置；连接或 schema 升级失败会报错退出，不静默回退到 SQLite。升级会创建缺少的表并合并历史重复周期账本，但不是任意历史 schema 的通用迁移工具；不支持的结构拒绝启动且事务回滚。升级前备份，详见[数据升级边界](docs/architecture.md#数据库与升级)。

---

## 示例

OpenAI/Anthropic 示例用于直接验证上游；WS 示例用于连接本项目网关：

```bash
# OpenAI 兼容服务
cargo run --example openai_stream_cli -- https://api.openai.com/v1 sk-xxx gpt-4o-mini

# Anthropic
cargo run --example anthropic_stream_cli -- https://api.anthropic.com sk-xxx claude-3-5-sonnet-latest

# 网关 WS（Token 从环境变量读取）
LLM_BRIDGE_API_KEY=lb_... cargo run --example ws_chat_client -- ws://127.0.0.1:3000/v1/ws MODEL hello
```

示例会先输出 `[THINK]`（若上游返回推理内容），再输出 `[TEXT]` 文本流。

---

## 可观测性

启用 `otel` 后导出 traces、logs 和 GenAI metrics。默认 OTLP blocking HTTP 客户端搭配 SDK 稳定线程式 `PeriodicReader`；shutdown 在 blocking 线程执行，避免 Tokio runtime 销毁冲突。共享请求终态日志包含 request_id、trace_id（启用 OTLP 时）和用量，不包含聊天内容。

```bash
cargo build --features otel
# 默认导出到 http://localhost:4318（标准 OTLP HTTP 端口）
```

- 每个 Actor 消息处理均带有 `#[instrument]` span
- `tracing-subscriber` 集成，支持 `RUST_LOG` env-filter
- GatewayManager 与 ProviderActor 的请求生命周期完整可追踪

协议及运维说明：[架构与数据升级](docs/architecture.md)、[WebSocket RPC](docs/ws-api.md)、[设备码登录](docs/device-login.md)、[目录导入](docs/catalog-import.md)。内容快照默认关闭，通过 `LLM_BRIDGE_OBS_CAPTURE_CONTENT=true` 开启；trace 默认保留 30 天，daily rollup 不随 trace 删除。

---

## VS Code Copilot 插件（规划中）🚧

LLM-Bridge 将提供一款 VS Code 扩展，打通 **编辑器 ↔ 网关** 的最后一公里：

```
┌──────────────────┐      自动同步模型列表      ┌──────────────────┐
│  VS Code         │ ◄──────────────────────► │  LLM-Bridge      │
│  + 插件          │    /v1/models +          │  网关            │
│                  │    参数配置               │                  │
└──────────────────┘                          └──────────────────┘
```

**核心能力（规划）：**

| 功能 | 说明 |
|------|------|
| **模型列表自动同步** | 插件定期拉取网关 `/v1/models`，自动填充 VS Code Copilot 的可用模型清单，无需手动编辑 JSON 配置 |
| **参数一键同步** | 网关中配置的 `max_tokens`、`temperature` 等参数可直接同步到编辑器，或在插件 UI 中按模型微调 |
| **多网关切换** | 支持配置多个 LLM-Bridge 实例，一键切换（如家庭网关 ↔ 工作室网关） |
| **Token 管理集成** | 在编辑器内直接查看 Token 用量、配额剩余，无需打开管理页面 |

> 插件将以 VS Code Extension 形式发布，兼容 VS Code Stable 与 Insiders，并计划支持 Cursor / Windsurf 等兼容编辑器。具体发布时间请关注本仓库 Release。

---

## 项目状态

LLM-Bridge 当前版本为 `0.1.0`。实现状态、已验证范围和仍需外部环境验收的项目统一维护在 STATUS，不再用静态勾选列表或“只剩测试文档”描述交付状态。

详细进度与已知问题见 [`STATUS.md`](./STATUS.md)，架构演进计划见 [`PLAN.md`](./PLAN.md)。

---

## License

本项目基于 [BSD 3-Clause License](./LICENSE) 开源 — © 2026 monetx。

- ✅ **可商用** — 允许商业使用与闭源衍生品。
- ✅ **可修改与再分发** — 但必须保留上述版权声明与许可文本。
- ✅ **强制署名** — 再分发源码或二进制时必须保留 `Copyright (c) 2026, monetx` 的版权声明。
- 🚫 **禁止冒名背书** — 未经 monetx 书面许可,不得使用其名义为衍生产品背书或推广。

完整条款见 [`LICENSE`](./LICENSE)。衍生项目建议在明显位置标注
"Based on LLM-Bridge by monetx, licensed under BSD-3-Clause" 以满足署名要求。
