# LLM-Bridge 项目现状报告

> 生成日期：2026-05-18
> 以代码实现为准。

---

## 1. 项目概况

| 项目 | 详情 |
|------|------|
| 名称 | `llm-bridge` |
| 定位 | LLM 网关服务 — 统一路由多个 LLM 提供者的 API 请求 |
| 语言 | Rust (后端) + TypeScript / Svelte 5 (前端) |
| 版本 | `0.1.0` |
| Rust Edition | 2024 |
| 运行时 | `tokio` (multi-thread) |
| Actor 框架 | `ractor` 0.15 |
| Web 框架 | `axum` 0.8 |
| HTTP 客户端 | `reqwest` 0.13 (rustls, stream) |
| 可观测性 | OpenTelemetry (traces + logs → OTLP) |
| 前端 | Svelte 5 + Vite 8 + TailwindCSS 4 + bits-ui |
| 类型生成 | `ts-rs` + `axfetchum` (自动生成 TS 客户端) |

---

## 2. 已实现功能

### 2.1 OpenAI 兼容 API

**文件：** `src/server/openai_api.rs`

| 端点 | 方法 | 说明 |
|------|------|------|
| `/v1/models` | GET | 返回可用模型列表 |
| `/v1/chat/completions` | POST | 聊天补全，支持流式 (SSE) 与非流式 |

**详情：**
- 接受标准 OpenAI Chat Completions 格式请求
- 内部将 OpenAI 消息格式转换为通用 `LanguageModelChatMessage` 类型
- 支持 `stream: true` — 返回 SSE 事件流
- 支持 `reasoning_content`（thinking/reasoning 内容透传）
- 当前支持 `role: user / assistant`（`system` 被映射为 user）
- 多部分内容（array content）目前仅提取文本部分

### 2.2 Admin REST API

**文件：** `src/server/admin.rs`（使用 `axfetchum` 声明式路由）

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/v1/models` | GET | 浏览全部模型（从 models.dev 同步） |
| `/api/v1/models/available` | GET | 仅返回已绑定启用提供者的模型 |
| `/api/v1/providers` | GET | 列出所有提供者及其配置 |
| `/api/v1/providers/{provider_name}` | PUT | 更新提供者配置（不存在时自动创建） |
| `/api/v1/providers/{provider_name}` | DELETE | 删除提供者 |

**认证：** 支持 Bearer Token，通过 `LLM_BRIDGE_AUTH_TOKEN` 环境变量配置。未配置时接口对所有调用方开放。

### 2.3 提供者适配器

**文件：** `src/actors/provider/adapters/`

| 适配器 | 协议 | 状态 |
|--------|------|------|
| `openai_chat_completions` | OpenAI `/v1/chat/completions` | ✅ 完整实现 |
| `openai_responses` | OpenAI `/v1/responses` | ✅ 完整实现 |
| `anthropic_messages` | Anthropic `/v1/messages` | ✅ 完整实现 |

**每个适配器支持：**
- 流式 SSE 响应解析
- 自定义 base URL + path suffix
- 自定义 HTTP headers（通过 `compat_settings`）
- 错误消息提取与转发
- Thinking/Reasoning 内容传输
- 工具调用结果传递

### 2.4 模型目录同步

**文件：** `src/config/models_dev_catalog.rs`, `src/models_dev.rs`

- 数据源：**models.dev**（`https://models.dev/api.json`）
- 启动时优先从本地缓存（`catalog_cache.json`）加载，若无则从 models.dev 拉取
- 支持 ETag 条件请求（304 Not Modified）
- 定期后台刷新（默认 900 秒，最小 30 秒）
- `strict_bootstrap` 模式：首次启动时若拉取失败且本地无缓存，则拒绝启动

### 2.5 存储层

**文件：** `src/store/`（`mod.rs`, `catalog.rs`, `providers.rs`, `error.rs`）

- 持久化方式：JSON 文件（`catalog_cache.json` + `providers.json`）
- 内存中使用 `RwLock` 包裹 `Arc<ModelsDevRoot>` 和 `HashMap<String, ProviderConfig>`
- 目录刷新时自动注册新发现的提供者（默认 disabled，需手动启用）
- 加权轮询 (weighted round-robin) API Key 选择，支持多 Key 负载均衡

### 2.6 Actor 模型

**文件：** `src/actors/`

```
GatewayManagerActor（单例）
    └── ProviderActor（每次请求临时创建，请求结束即销毁）
```

- **GatewayManagerActor**：管理模型目录初始化与定时刷新，处理模型查询与路由解析
- **ProviderActor**：由 HTTP handler 直接创建，接收 ChatRequest，分发到对应协议适配器，返回流式响应后销毁
- 当前无 WebSocket 连接管理 Actor

### 2.7 前端 — 管理界面

**技术栈：** Svelte 5 + Vite 8 + TailwindCSS 4 + bits-ui + svelte-spa-router

| 页面 | 路由 | 功能 |
|------|------|------|
| 模型目录 | `/` 或 `/models` | 表格展示，支持搜索、排序、全部/可用筛选 |
| 提供者管理 | `/providers` | 卡片列表，支持编辑对话框和删除 |

**特点：**
- 侧边栏导航（可折叠）
- TypeScript 类型自动生成（`ts-rs` + `axfetchum`）
- 前端 API 客户端完全自动生成（`frontend/src/bindings/client.ts`）

### 2.8 可观测性

**文件：** `src/observability/mod.rs`

- OpenTelemetry traces + logs → OTLP HTTP 导出
- `tracing-subscriber` 集成，支持 `EnvFilter`
- 所有 Actor 消息处理均带有 `#[instrument]` 追踪

---

## 3. 代码结构

```
src/
├── main.rs                     # 入口：初始化可观测性 → 加载配置 → 启动 HTTP 服务
├── lib.rs                      # 模块声明
├── types.rs                    # 通用 LM 类型（消息、角色、响应、工具调用等）
├── models_dev.rs               # models.dev API 数据结构
├── config/
│   ├── mod.rs
│   ├── models.rs               # RuntimeSettings, OidcConfig, 环境变量解析
│   └── models_dev_catalog.rs   # models.dev HTTP 客户端（拉取 + 缓存）
├── db/                         # 🆕 Phase 1 — toasty ORM 数据库层
│   ├── mod.rs                  # 连接管理、init 函数、all_models()、集成测试
│   └── models.rs               # User, Token, UsageRecord, Provider, ProviderModel
├── middleware/                  # 🆕 Phase 2 — 认证中间件
│   ├── mod.rs                  # 模块入口
│   ├── session_auth.rs         # Session 认证提取器（SessionAuth / AdminAuth）
│   └── token_auth.rs           # Bearer Token 认证提取器（TokenAuth）
├── auth/                       # 🆕 Phase 1-2 — 认证模块
│   ├── mod.rs                  # 模块入口
│   ├── oidc.rs                 # OIDC Service（discover / login_url / callback）
│   ├── session.rs              # Session 数据类型（OidcContext / SessionUser）
│   ├── token.rs                # 🆕 Token Service（创建/验证/CRUD/模型权限检查）
│   └── quota.rs                # 🆕 Quota Service（周期计数/配额检查/扣减/重置）
├── store/                      # 🔄 Phase 3 重写 — 基于 toasty + SQLite
│   ├── mod.rs                  # Store 核心（db + KeySelector + CRUD）
│   ├── catalog.rs              # models.dev 磁盘缓存读写（发现用）
│   ├── compat.rs               # 🆕 npm → 兼容协议推导 + 数据映射
│   ├── router.rs               # 🆕 路由解析（DB 查询）
│   └── error.rs                # StoreError
├── store/
│   ├── mod.rs                  # Store — 核心数据层（模型查询、路由解析、轮询选 Key）
│   ├── catalog.rs              # catalog_cache.json 读写
│   ├── providers.rs            # providers.json 读写
│   └── error.rs                # StoreError
├── actors/
│   ├── mod.rs
│   ├── gateway_manager.rs      # GatewayManagerActor（目录初始化、定时刷新、路由解析）
│   └── provider/
│       ├── mod.rs              # ProviderActor（请求入口，分发到适配器）
│       ├── adapters.rs         # 适配器分发（按 ProviderCompatibility 枚举）
│       └── adapters/
│           ├── openai_chat_completions.rs
│           ├── openai_responses.rs
│           └── anthropic_messages.rs
├── server/
│   ├── mod.rs
│   ├── admin.rs                # 🔄 Admin REST API（Session + Admin 认证）
│   ├── auth.rs                 # 🆕 Auth API（/auth/login, callback, me, logout）
│   ├── tokens.rs               # 🆕 Token 管理 API（CRUD）
│   └── openai_api.rs           # OpenAI 兼容 API + 服务器启动
└── observability/
    └── mod.rs                  # OpenTelemetry 初始化
```

---

## 4. 环境变量配置

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `LLM_BRIDGE_GATEWAY_ID` | `llm-bridge-v1` | 网关标识 |
| `LLM_BRIDGE_HOST` | `127.0.0.1` | 监听地址 |
| `LLM_BRIDGE_PORT` | `3000` | 监听端口 |
| `LLM_BRIDGE_AUTH_TOKEN` | 无 | Bearer Token（不设置则无认证） |
| `LLM_BRIDGE_STORE_PATH` | `./data/llm-bridge` | 数据存储目录 |
| `LLM_BRIDGE_CATALOG_BASE_URL` | `https://models.dev` | 模型目录 API 地址 |
| `LLM_BRIDGE_CATALOG_REFRESH_INTERVAL_SECS` | `900` | 目录刷新间隔（秒） |
| `LLM_BRIDGE_CATALOG_REQUEST_TIMEOUT_SECS` | `30` | 目录请求超时（秒） |
| `LLM_BRIDGE_CATALOG_STRICT_BOOTSTRAP` | `true` | 首次启动时必须成功拉取目录 |
| `RUST_LOG` | `info` | 日志级别（tracing-subscriber env-filter） |

---

## 5. Phase 1-5 实施进度

> 详见 [`PLAN.md`](PLAN.md) 第 6 章。

### Phase 1：基础设施（数据库 + OIDC）

| 任务 | 状态 | 日期 |
|------|------|------|
| 1.1 引入 toasty 依赖，配置 SQLite | ✅ | 2026-05-19 |
| 1.2 定义数据模型（User, Token, UsageRecord, Provider, ProviderModel） | ✅ | 2026-05-19 |
| 1.3 实现数据库初始化与迁移 | ✅ | 2026-05-19 |
| 1.4 实现 OIDC Service | ✅ | 2026-05-19 |
| 1.5 实现 Session 管理 | ✅ | 2026-05-19 |
| 1.6 实现 Auth API 端点 | ✅ | 2026-05-19 |

### Phase 2：Token 体系 + 配额管理

| 任务 | 状态 | 日期 |
|------|------|------|
| 2.1 实现 Token Service（创建/验证/CRUD） | ✅ | 2026-05-19 |
| 2.2 实现 Quota Service（计数+重置+检查） | ✅ | 2026-05-19 |
| 2.3 实现 Token 管理 API（CRUD） | ✅ | 2026-05-19 |
| 2.4 实现配额后台重置任务 | ✅ | 2026-05-19 |
| 2.5 实现 Token 认证中间件 | ✅ | 2026-05-19 |
| 2.6 实现 Session 认证中间件 | ✅ | 2026-05-19 |
| 2.7 管理员自动提升逻辑 | ✅ | 2026-05-19（Phase 1 已实现） |

### Phase 3：提供者与模型存储 + API 改造

| 任务 | 状态 | 日期 |
|------|------|------|
| 3.1 重写 Store 层：用 toasty 操作 providers / provider_models 表 | ✅ | 2026-05-19 |
| 3.2 实现兼容协议推导（npm → compatibility）和 models.dev 数据映射 | ✅ | 2026-05-19 |
| 3.3 实现 models.dev 代理与导入端点（浏览 + 搜索 + 导入） | ✅ | 2026-05-20 |
| 3.4 重写路由解析：从 provider_models 表读取 | ✅ | 2026-05-19 |
| 3.5 增强 /v1/models：完整能力+定价 | ✅ | 2026-05-19 |
| 3.6 改造 /v1/chat/completions 接入 Token 认证和配额 | ✅ | 2026-05-19（已在 Phase 2 完成） |
| 3.7 移除旧的 Bearer Token Admin 认证 + 清理废弃代码 | ✅ | 2026-05-19 |

### Phase 4-5

| Phase | 内容 | 状态 |
|-------|------|------|
| Phase 4 | Admin API + 前端 | 🔄 前端重写中 |
| Phase 5 | 测试与文档 | ⬜ |

---

## 6. 代码质量

**开发规范：**
- **每次修改后必须运行 `cargo clippy`** — 所有 lint 警告必须在提交前清零（✅ 当前 `cargo clippy --lib` — 0 warnings）

**优点：**
- **类型安全**：核心类型与 TypeScript 自动同步（`ts-rs`），前端 API 客户端自动生成（`axfetchum`）；数据库枚举字段（`UserRole`、`ProviderCompatibility`）用 `toasty::Embed` 实现 CHECK 约束
- **架构清晰**：Actor 模型 + 适配器模式，职责分离良好；新增 `db` 模块独立管理 ORM 和模型
- **可观测性**：完善的 tracing 集成，每个 Actor 消息处理均有 span
- **流式处理**：SSE 流解析器自实现，正确处理分帧
- **错误处理**：各层均有 `thiserror` 定义的错误类型，HTTP 层统一转换为 JSON 错误响应
- **前端现代化**：Svelte 5 runes、TailwindCSS 4、声明式表格组件
- **数据库层已测试**：`src/db/mod.rs` 中有 2 个集成测试（建表 + CRUD 往返）
- **双通道 TS 生成**：`ts-rs` 负责 `.ts` 类型文件（`cargo test export_bindings`），`axfetchum` 负责 API 客户端（`cargo test generate_ts_client`），Token 相关类型（`CreateTokenRequest`、`CreateTokenResponse`、`UpdateTokenRequest`、`TokenListItem`）已全部导出

**已知问题：**
- OpenTelemetry 版本冲突（`opentelemetry 0.31 vs 0.32`），导致 `observability/mod.rs` 编译错误（`cargo check --lib` 可绕过，全量编译受阻），需统一依赖版本
- **适配器重复代码**：三个适配器中 SSE 解码和流处理逻辑重复较多，可提取公共 SSE 工具模块
- **API Key 存储**：当前 `providers.json` 明文，重构后 SQLite 中也需考虑加密

---

## 7. 构建与运行

```bash
# 构建
cargo build --release

# 运行
cargo run

# 生成 ts-rs 类型文件 (.ts 文件)
cargo test export_bindings

# 生成 axfetchum API 客户端 (client.ts)
cargo test generate_ts_client

# 前端开发
cd frontend && bun install && bun run dev

# 前端构建
cd frontend && bun run build
```

**示例脚本：** `examples/openai_stream_cli.rs` 和 `examples/anthropic_stream_cli.rs` 提供了命令行端到端测试入口。

---

## 8. 重构计划

项目正在进行大规模重构，目标是成为一个**自托管的 OpenRouter**。详细计划见 [`PLAN.md`](PLAN.md)。

### 8.1 计划概览

| Phase | 内容 | 预估 |
|-------|------|------|
| Phase 1 | 基础设施（toasty + SQLite + OIDC） | 18h |
| Phase 2 | Token 体系 + 配额管理 | 23h |
| Phase 3 | 提供者与模型存储 + API 改造 | 24h |
| Phase 4 | Admin API + 前端 | 26h |
| Phase 5 | 测试与文档 | 13h |
| **总计** | | **~96h** |

### 8.2 当前进度

| 任务 | 状态 | 完成日期 |
|------|------|---------|
| 需求分析与计划文档 | ✅ 已完成 | 2026-05-18 |
| Phase 1：基础设施 | ✅ 已完成 (6/6) | 2026-05-19 |
| Phase 2：Token 体系 | ✅ 已完成 (7/7) | 2026-05-19 |
| Phase 3：存储与 API | ✅ 已完成 (6/7) | 2026-05-19 |
| Phase 4：Admin + 前端 | ⬜ 未开始 | — |
| Phase 5：测试与文档 | ⬜ 未开始 | — |

### 8.3 核心变更

| 方面 | 重构前（当前代码） | 重构后（PLAN 目标） |
|------|-----------------|------------------|
| 存储 | JSON 文件 + RwLock | toasty + SQLite（部分迁移） |
| 认证 | Bearer Token（环境变量） | OIDC + Session + API Token |
| 模型目录 | models.dev 直接驱动路由 | models.dev 仅作发现（计划中），运行时暂用 Store |
| Admin API | Bearer Token 认证 | Session + RBAC（计划中） |
| Token 管理 | 无 | 用户创建多 Token，每 Token 独立配额 + 模型范围 |
| API 认证 | Bearer Token（环境变量） | TokenAuth 提取器（bcrypt 验证 Token） |
| 前端 | 无认证 | OIDC 登录 + Token 管理页面（API 已就绪） |

---

## 9. 总结

LLM-Bridge 当前处于**重构 Phase 3 核心完成、Phase 4 待开始**。核心路径已完整实现。

**Phase 1 已完成：**
- 引入 `toasty` ORM + `jiff` 时间库，新增 `openidconnect` + `tower-sessions` + `bcrypt` + `rand` 依赖
- SQLite（默认）+ PostgreSQL（可选 feature）双数据库支持
- 5 张核心表数据模型定义完毕（`User`, `Token`, `UsageRecord`, `Provider`, `ProviderModel`）
- 数据库初始化与自动建表（`db::init` / `db::init_sqlite`）
- OIDC Service（`OidcService::discover` / `login_url` / `callback`）
- Session 管理（`tower-sessions` + `MemoryStore`）
- Auth API 端点（`/auth/login`, `/auth/callback`, `/auth/me`, `/auth/logout`）
- 管理员首任机制（首个 OIDC 登录用户自动 Admin）
- `UserRole`、`ProviderCompatibility` 枚举类型安全
- `created_at` / `updated_at` 使用 `jiff::Timestamp` + `#[auto]` 自动管理
- 2 个集成测试（建表 + 插入查询往返）
- `cargo clippy --lib` — 0 warnings

**Phase 2 已完成：**
- 实现 `src/auth/token.rs` — Token Service（随机生成 `lb_` 前缀 + base62 + bcrypt 哈希，CRUD，模型权限精确匹配）
- 实现 `src/auth/quota.rs` — Quota Service（daily/monthly/unlimited 周期，配额检查 + 原子扣减，后台过期周期清理）
- 实现 `src/server/tokens.rs` — Token 管理 API（GET/POST/PATCH/DELETE `/api/v1/tokens`，Session 认证 + 所有权验证）
- 实现 `src/middleware/session_auth.rs` — SessionAuth / AdminAuth 提取器（`FromRequestParts`）
- 实现 `src/middleware/token_auth.rs` — TokenAuth 提取器（Bearer Token bcrypt 验证）
- 更新 GatewayManagerActor — 新增 `ResetQuota` 消息 + 每小时配额重置后台循环
- 更新 `openai_api.rs` — `/v1/models` 和 `/v1/chat/completions` 迁移到 TokenAuth，增加配额前置检查
- `AppState` 新增 `db: db::Db` 字段，auth API 与 token API 共享数据库句柄
- 36 个测试全部通过，`cargo clippy` — 0 warnings

**Phase 3 核心已完成（6/7 任务）：**
- 重写 `src/store/mod.rs` — 废弃 JSON 文件 + RwLock，改用 toasty Db 直接操作 SQLite
- 删除 `src/store/providers.rs` — JSON 文件读写全部移除
- 新增 `src/store/router.rs` — 路由解析从 `provider_models` + `providers` 表查询，含加权轮询 KeySelector
- 新增 `src/store/compat.rs` — npm → ProviderCompatibility 推导 + models.dev 数据映射
- 重写 `src/server/admin.rs` — 认证从 Bearer Token 改为 Session（SessionAuth / AdminAuth）；提供者列表返回完整 ProviderResponse
- 更新 `src/server/openai_api.rs` — `list_models` / `resolve_model` → async；返回完整定价+能力
- 更新 `src/main.rs` — Store 由 `Store::new(db, path)` 创建，移除 JSON 文件依赖
- 更新 `src/actors/gateway_manager.rs` — 适配新 Store API
- ⬜ models.dev 代理导入端点 延后至 Phase 4（前端 Admin UI 配套）

**Phase 4 待开始：** Admin API + 前端
