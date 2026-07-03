# LLM-Bridge 项目现状报告

> 生成日期：2026-05-18
> 最后更新：2026-07-03 — **大幅同步**，对齐 [`PLAN.md`](PLAN.md)（2026-06-29 多协议架构重设计）。以代码实况为准。

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

**文件：** `src/server/admin.rs`（使用 `axfetchum::ApiRouter` 声明式路由）

| 端点 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/v1/models` | GET | Session | 浏览全部已注册模型（含能力+定价+提供者列表） |
| `/api/v1/models/available` | GET | Session | 仅返回已绑定启用提供者的模型 |
| `/api/v1/admin/providers` | GET | Admin | 列出所有提供者及其配置 |
| `/api/v1/admin/providers` | POST | Admin | 创建提供者 |
| `/api/v1/admin/providers/{id}` | GET | Admin | 获取单个提供者详情 |
| `/api/v1/admin/providers/{id}` | PUT | Admin | 更新提供者配置 |
| `/api/v1/admin/providers/{id}` | DELETE | Admin | 删除提供者（级联删除其 ModelProvider 关联） |
| `/api/v1/admin/providers/{id}/models` | GET | Admin | 列出提供者下的模型关联 |
| `/api/v1/admin/providers/{id}/models` | POST | Admin | 为提供者添加 ModelProvider 关联 |
| `/api/v1/admin/providers/{id}/models/{model_id}` | PUT / DELETE | Admin | 更新 / 删除某条 ModelProvider 关联 |
| `/api/v1/admin/users` | GET | Admin | 列出所有用户 |
| `/api/v1/admin/users/{id}/role` | PATCH | Admin | 修改用户角色 |

**认证：** Session Cookie（`tower-sessions`）。浏览类端点使用 `SessionAuth` 提取器（任何已登录用户），管理操作类使用 `AdminAuth` 提取器（需 `UserRole::Admin`）。旧的 `LLM_BRIDGE_AUTH_TOKEN` Bearer Token Admin 认证已移除；`AppState.auth_token` 字段及 `openai_api.rs::check_auth` 仍作为遗留死代码保留（见第 6 章）。

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

### 2.4 模型目录同步（✅ 已删除 models.dev 集成）

按照 [`PLAN.md`](PLAN.md)（2026-06-29）§4「删除 models.dev 集成」执行完成（2026-07-03）：

- ✅ 删除 `src/models_dev.rs`
- ✅ 删除 `src/config/models_dev_catalog.rs`（`src/config/` 现仅剩 `mod.rs` + `models.rs`）
- ✅ 删除 `src/store/catalog.rs`（`src/store/` 现仅剩 `compat.rs / error.rs / mod.rs / router.rs`）
- ✅ 删除 `data/llm-bridge/catalog_cache.json`（目录中仅剩 `llm-bridge.db`）
- ✅ 删除前端 `CatalogProviderSummary.ts` / `ImportedProvider.ts` 等 models.dev 相关绑定
- ✅ `src/store/compat.rs` 已重写：模块注释显式声明「models.dev 集成已删除，兼容协议改由 `ProviderProtocol.protocol` 显式声明」
- ✅ **2026-07-03** 删除 `src/config/models.rs` 中 `ModelCatalogConfig` 结构体与 `LLM_BRIDGE_CATALOG_*` 四个环境变量解析；`main.rs` 中 `catalog_base_url` 启动日志字段同时移除

### 2.5 存储层

**文件：** `src/store/`（`mod.rs`, `compat.rs`, `router.rs`, `error.rs`）

- 持久化方式：**toasty + SQLite**（旧 JSON 文件 + RwLock 方案在 Phase 3 已废弃）
- `Store` 结构：`{ db: db::Db, path: PathBuf, key_selector: Arc<KeySelector> }`，直接持有 toasty 句柄，无内存缓存层
- `compat.rs`：models.dev 集成已删除，现仅保留 `ProviderCompatibility` 枚举推导提示（面向 CatalogSource 导入路径，尚未启用）
- `router.rs`：路由解析基于 **四张表** `models` + `model_providers` + `provider_protocols` + `providers` 应用层关联查询（`LLMModel::filter → ModelProvider::filter → ProviderProtocol::get_by_id → Provider::get_by_id`，非 SQL 原生 JOIN；模型量增大时为 N+1 热点，见第 6 章）
- `providers.rs`（旧 JSON 提供者存储）已在 Phase 3 **物理删除**；`catalog.rs`（models.dev 磁盘缓存）已在多协议重构中**物理删除**
- 加权轮询 (weighted round-robin) API Key 选择由 `KeySelector`（`Mutex<HashMap<String, AtomicU64>>`）维护，仍在 Store 层

### 2.6 Actor 模型

**文件：** `src/actors/`

```
GatewayManagerActor（单例）
    └── ProviderActor（每次请求临时创建，请求结束即销毁）
```

- **GatewayManagerActor**：管理配额重置（每小时）与 Token 使用计数；原目录初始化/刷新职责已在多协议重构中删除（见 `PLAN.md` §5.6）
- **ProviderActor**：由 HTTP handler 直接创建，接收 ChatRequest，分发到对应协议适配器，返回流式响应后销毁
- 当前无 WebSocket 连接管理 Actor

### 2.7 前端 — 管理界面

**技术栈：** Svelte 5 + Vite 8 + TailwindCSS 4 + bits-ui + svelte-spa-router

| 页面 | 路由 | 认证 | 功能 |
|------|------|------|------|
| 登录 | `/login` | 无 | 自动触发 OIDC `/auth/login` 跳转 |
| 模型目录 | `/` 或 `/models` | Session | 表格展示，支持搜索、排序、全部/可用筛选 |
| API Token 管理 | `/tokens` | Session | 当前用户 Token 的 CRUD（创建对话框返回明文 Token，仅一次） |
| 提供者管理 | `/providers` | Admin | 卡片列表，编辑对话框 + 删除 + 模型关联管理 + **协议增删改 UI**（2026-07-03 完成）：Provider CRUD 与模型关联 CRUD 已走通；**协议列表+增删改内联表单+创建对话框中可携带协议**已落地 |
| 模型管理 | `/admin/models` | Admin | ⚠️ **未实现**（PLAN.md 多协议重构未涵盖）：完整 LLMModel CRUD（model_name、display_name、标称能力 max_input/output_tokens、tool_calling/vision/thinking/adaptive_thinking、status）+ 内嵌 ModelProvider 关联管理。**与提供者页对称**，不依赖任何远端 CatalogSource（见 §11.3） |
| 用户管理 | `/users` | Admin | 用户列表 + 角色修改 |

**特点：**
- 侧边栏导航（可折叠），按 RBAC 分组：菜单区（模型目录、API Token）所有用户可见；管理区（提供者、模型、用户）仅 `isAdmin` 可见
- 未认证访问受保护路由时自动跳转 `/login`
- **手动配置三件套为一等公民**（PLAN.md §1 指导原则）：Provider / LLMModel / ModelProvider 三层在提供者页与模型页对称地可完整手动 CRUD，**不依赖 CatalogSource**；空配置启动也必须能用。当前提供者页部分实现（缺协议管理 UI），模型管理页尚未实现，见 §11.1 行动项 E
- TypeScript 类型自动生成（`ts-rs` 负责类型文件，`axfetchum` 负责 API 客户端 `frontend/src/bindings/client.ts`）
- `frontend/src/bindings/` 当前含 26 个自动生成的 `.ts` 文件（多协议重构后已删除 models.dev 相关绑定）；其中已含 `AddModelRequest.ts` / `UpdateModelRequest.ts` 的 `protocolId` 字段，但**尚无** `CreateProtocolEntry.ts` / `ProtocolView` 等协议专用类型

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
├── config/
│   ├── mod.rs
│   └── models.rs               # RuntimeSettings, OidcConfig, ProviderCompatibility 枚举
├── db/                         # 🆕 Phase 1 — toasty ORM 数据库层
│   ├── mod.rs                  # 连接管理、init 函数、all_models()（注册 **7 张表**）、集成测试
│   └── models.rs               # User, Token, UsageRecord, LLMModel, Provider, **ProviderProtocol**, ModelProvider
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
├── store/                      # 🔄 Phase 3 重写 + 多协议重构
│   ├── mod.rs                  # Store 核心（db + KeySelector + CRUD，含 ensure_model()）
│   ├── compat.rs               # 🆕 已删除 models.dev 集成；保留 ProviderCompatibility 枚举提示
│   ├── router.rs               # 🆕 路由解析（四表：models + model_providers + provider_protocols + providers 应用层关联）
│   └── error.rs                # StoreError
├── actors/
│   ├── mod.rs
│   ├── gateway_manager.rs      # GatewayManagerActor（配额重置、路由解析）；catalog 刷新循环已删除
│   └── provider/
│       ├── mod.rs              # ProviderActor（请求入口，分发到适配器）
│       ├── adapters.rs         # 适配器分发（按 ProviderCompatibility 枚举）
│       └── adapters/
│           ├── openai_chat_completions.rs
│           ├── openai_responses.rs
│           └── anthropic_messages.rs
├── server/
│   ├── mod.rs
│   ├── admin.rs                # 🔄 Admin REST API（Session + Admin 认证），含 ProviderProtocol 子路由 |
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
| `LLM_BRIDGE_AUTH_TOKEN` | 无 | Bearer Token（不设置则无认证 — 死代码保留，见第 6 章） |
| `LLM_BRIDGE_STORE_PATH` | `./data/llm-bridge` | 数据存储目录 |
| `RUST_LOG` | `info` | 日志级别（tracing-subscriber env-filter） |

---

## 5. Phase 1-5 实施进度

> **历史快照（2026-05-18 → 06-25 原始重构）**：详见 [`PLAN.md`](PLAN.md) 第 6 章。这部分已完成的工作在 2026-06-29 多协议架构重设计后已被超越，下表为历史记录，**不代表当前任务总线**。当前任务总线见 §5.5。

### Phase 1：基础设施（数据库 + OIDC）

| 任务 | 状态 | 日期 |
|------|------|------|
| 1.1 引入 toasty 依赖，配置 SQLite | ✅ | 2026-05-19 |
| 1.2 定义数据模型（User, Token, UsageRecord, Provider, ProviderModel） | ✅ | 2026-05-19 |
| — | 🔄 重构为 Model + ModelProvider（OpenRouter 风格） | ✅ | 2026-05-20 |
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
| Phase 4a | 数据结构重构（LLMModel + ModelProvider） | ✅ 2026-05-20 |
| Phase 4 | Admin API + 前端 | ✅ 基本完成 2026-06-25 |
| Phase 5 | 测试与文档 | 🔄 部分启动 |
| 附录 | toasty 0.7.0 关系 API 迁移（BelongsTo/HasMany → Deferred） | ✅ 2026-06-25 |

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
- ~~OpenTelemetry 版本冲突（`opentelemetry 0.31 vs 0.32`）~~ → ✅ 已解决（2026-06-25 复核）：`Cargo.toml` 已统一为 `opentelemetry 0.32.x` 单一版本，`observability/mod.rs` 完整使用 0.32 API 且无编译错误
- **死代码残留**：`src/server/openai_api.rs:23 fn check_auth(...)` 仍在文件中，引用 `AppState.auth_token`，但全工作区无任何调用点；`AppState.auth_token: Option<String>` 字段也保留未删。Phase 3.7 称"清理废弃代码"完成，实际未彻底
- **应用层 N+1 查询**：`src/store/router.rs` 注释称“四表 JOIN”，实为 `LLMModel::filter → ModelProvider::filter → ProviderProtocol::get_by_id → Provider::get_by_id` 应用层循环关联，非 SQL 原生 JOIN。模型量增大后为性能热点，建议后续用 toasty 预加载或原生 JOIN 优化
- **注释与实现符号不一致**：`src/store/compat.rs` 顶部模块注释提到 `models_dev_to_model_data`，实际功能被拆为 `ModelCapabilitiesFromDev::from_models_dev` + `ModelPricingFromDev::from_models_dev` + `deduce_base_url` 三个符号，无同名函数
- **测试计数存疑**：第 10 章曾称"55 个测试全部通过"，实际 `src/` 下仅 25 个 `#[test]`/`#[tokio::test]`。需 `cargo test --no-run` 给出权威统计后方可定稿
- **TS 客户端漂移检测禁用**：`tests/generate_ts_client.rs::check_ts_client_up_to_date` 当前被注释，CI 无法捕获前端绑定生成物与后端路由不同步
- **适配器重复代码**：三个适配器中 SSE 解码和流处理逻辑重复较多，可提取公共 SSE 工具模块
- **API Key 存储**：`providers.api_keys` 在 SQLite 中明文存储（JSON 数组），未加密。PLAN 第 9 章 open question #3 仍待决策

---

## 10. Phase 4a 数据结构重构（2026-05-20）

将数据模型重构为 OpenRouter 风格：**模型（Model）作为主要索引**。

### 变更内容

| 原结构 | 新结构 |
|--------|--------|
| `provider_models` 表（模型名+能力+定价耦合在一条记录中） | `models` 表（规范模型定义+标称能力）+ `model_providers` 关联表（提供者特定定价+能力覆盖） |
| 一个模型只能关联一个提供者 | 一个模型可关联多个提供者，按 priority fallback |
| `/v1/models` 返回扁平列表 | `/v1/models` 返回模型列表 + 每个模型下各提供者的能力覆盖和定价 |

### 新数据模型

```
Model ──< ModelProvider >── Provider
```

- **Model**: `model_name`（如 `"openai/gpt-4o"`）是唯一标识，存储标称能力
- **ModelProvider**: 多对多关联，存储提供者侧的实际能力（可覆盖标称值）和独立定价，按 priority 排序
- **Provider**: 上游 LLM 提供者配置（API Keys、base URL 等），不变

### 受影响的文件

| 文件 | 变更 |
|------|------|
| `src/db/models.rs` | 新增 `LLMModel` 表（`#[toasty(table="models")]`），`ProviderModel` → `ModelProvider`（`#[toasty(table="model_providers")]`），字段 restructure |
| `src/db/mod.rs` | `all_models()` 注册 **7 张表**（User, Token, UsageRecord, LLMModel, Provider, **ProviderProtocol**, ModelProvider） |
| `src/store/router.rs` | 完全重写，基于 models + model_providers + providers **应用层关联查询**（非 SQL 原生 JOIN） |
| `src/store/mod.rs` | CRUD 适配新结构，新增 `ensure_model()` 自动创建 LLMModel |
| `src/server/openai_api.rs` | `/v1/models` 返回增强格式（capabilities + providers[]） |
| `src/server/admin.rs` | 响应类型适配新结构 |
| `src/store/compat.rs` | 注释与实现符号不一致（注释提 `models_dev_to_model_data`，实际拆为 `ModelCapabilitiesFromDev` / `ModelPricingFromDev` / `deduce_base_url`，见第 6 章） |

### 验证状态

- ✅ `cargo check --lib` — 编译通过
- ✅ `cargo clippy --lib` — 0 warnings
- ⚠️ ~~55 个测试全部通过~~ → 实际 `src/` 下仅 25 个 `#[test]`/`#[tokio::test]`（db 2 + anthropic_messages 7 + openai_responses 16），待 `cargo test --no-run` 给出含 tests/ 与 doctest 的权威计数
- ✅ TypeScript 前端绑定自动生成成功（`frontend/src/bindings/` 31 个 `.ts` 文件）

### 附录：toasty 0.7.0 关系 API 迁移（2026-06-25）

`toasty` 0.7.0 移除了 `toasty::schema::{BelongsTo, HasMany}` 的导出，关系字段改用 `toasty::schema::Deferred<T>`（lazy）/ 急切类型（如 `Vec<T>`）。本项目的迁移结论：

- **涉及文件仅 `src/db/models.rs`**：1 处 import（`use toasty::schema::Deferred;`）+ 8 处关系字段类型替换
- 全部关系字段使用 lazy 模式：`#[has_many]` → `Deferred<Vec<T>>`，`#[belongs_to]` → `Deferred<T>`
- 运行时代码从未使用关联访问器或 `.include()`，全部走标量 FK 字段查询，故无业务代码改动
- `src/` 下 `BelongsTo` / `HasMany` 零残留（仅 `PLAN.md` 历史设计示例保留 6 处文本，不影响编译）
- 详见 `/memories/repo/toasty-0.7-migration.md`

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
| Phase 4a：数据结构重构（LLMModel + ModelProvider） | ✅ 已完成 | 2026-05-20 |
| Phase 4：Admin API + 前端 | ✅ 基本完成 | 2026-06-25 |
| Phase 5：测试与文档 | 🔄 部分启动 | — |
| toasty 0.7.0 迁移 | ✅ 已完成 | 2026-06-25 |
| **多协议架构重设计 PLAN（2026-06-29）** | 🔄 进行中 | — |
| └ Phase 1 数据模型 | ✅ 已完成 | 2026-07-01（估算） |
| └ Phase 2 Store 适配 | ✅ 已完成 | 2026-07-03 |
| └ Phase 3 Admin API | 🟡 70% | 2026-07-03 |
| └ Phase 4 删 models.dev | ✅ 已完成 | 2026-07-03 |
| └ Phase 5 前端适配 | ✅ 已完成 | 2026-07-03 |
| └ Phase 6 Actor 确认 | ✅ 已完成 | — |

> 多协议重构 Phase 详细进度见 §5.5；当前行动项见 §11。

**Phase 4 实际状态（2026-06-25 复核）：**
- 后端 Admin API 已含提供者 CRUD、ModelProvider 关联 CRUD、models.dev 搜索 + 导入、用户角色管理（见 2.2 节）
- 前端 5 个页面已实现：Login、Models、Tokens、Providers、Users（见 2.7 节）
- OIDC 登录链路（前端 LoginPage → 后端 `/auth/login` → IdP → `/auth/callback` → Session）端到端打通
- `frontend/src/bindings/` 31 个 TS 文件、`client.ts` 已由 axfetchum 自动生成

**Phase 5 实际状态：**
- `docs/admin-api.md`、`docs/architecture.md` 骨架已存在（待定稿）
- `tests/generate_ts_client.rs` 存在（含 `generate_ts_client` 测试；`check_ts_client_up_to_date` 当前被注释禁用）
- `src/db/mod.rs` 含 2 个集成测试，三个适配器共 23 个 `#[test]`，合计 `src/` 下 25 个 `#[test]`/`#[tokio::test]`（权威计数待 `cargo test --no-run`）

---

### 5.5 多协议架构重设计 Phase 进度（2026-06-29 →）

> 见 [`PLAN.md`](PLAN.md)。下表为当前任务总线。**注意**：PLAN Phase 编号与历史 Phase 不重合，PLAN Phase 1 = 数据模型定义，对应历史概念的「Phase 4a 续作」。

| Phase | 内容 | 状态 | 备注 |
|-------|------|------|------|
| **Phase 1** | 数据模型：新增 `ProviderProtocol`，重构 `Provider`/`ModelProvider` | ✅ 完成 | `src/db/models.rs` 七张表已实现：Provider 删 npm/base_url/compat_settings、`api_keys: toasty::Json<Vec<ApiKeyEntry>>`、ModelProvider 以 `protocol_id` FK 替代 compatibility 枚举 |
| **Phase 2** | Store 层适配：upsert、级联删除、四表 JOIN、KeySelector 直接收 `&[ApiKeyEntry]` | ✅ 完成（2026-07-03） | `upsert_provider` 新签名、`delete_provider_by_id` 级联删 ProviderProtocol、`router.rs` 四表关联查询均已落地。✅ **新增** `upsert_protocols` 等价语义：`replace_provider_protocols` 批量 diff（增/改/删），并引入 `list_provider_protocols`/`create_provider_protocol`/`update_provider_protocol`/`delete_provider_protocol` 单条操作 |
| **Phase 3** | Admin API：删旧字段、新增 `CreateProtocolEntry`、`GET/PUT /providers/:id/protocols` 端点 | 🟡 70% 完成 | ✅ `CreateProviderRequest`/`UpdateProviderRequest` 已删旧字段并加入 `protocols: Vec<ProtocolInput>`（PLAN 中的 `CreateProtocolEntry` 以 `ProtocolInput` 形式落地）；✅ 新增独立协议端点 `GET/PUT /api/v1/admin/providers/{id}/protocols`；✅ `AddModelRequest` 已用 `protocol_id`；✅ `ProviderResponse` 加入 `protocols` 字段与 `ProtocolView` 类型。⬜ 前端 UI 未含协议增删改组件（属 Phase 5） |
| **Phase 4** | 删除 models.dev 集成 | ✅ 完成（2026-07-03） | 删 `models_dev.rs`、`models_dev_catalog.rs`、`store/catalog.rs`、`catalog_cache.json`、前端 models.dev 绑定；并删除 `config/models.rs` 的 `ModelCatalogConfig` + `LLM_BRIDGE_CATALOG_*` 环境变量、`main.rs:105` 的 `catalog_base_url` 日志 |
| **Phase 5** | 前端适配：协议增删改 UI、重新生成绑定 | ✅ 完成（2026-07-03） | `ProvidersPage.svelte` 已实现协议列表展示、编辑/新建表单（协议下拉+URL+优先级+启用 toggle+compat settings）、全量替换语义调用 `replaceProviderProtocols`；创建 Provider 对话框中可携带协议；同步调 Provider update 携带现有 protocols，修复模板中 `m.compatibility` → `protocolId` 反查。TS 类型已自动生成 |
| **Phase 6** | Actor / 适配器确认 | ✅ 完成 | `adapters.rs` 通过 `state.compatibility` 分发，来自 `ResolvedProviderRoute.compatibility`，数据源由 ModelProvider.compatibility 改为 ProviderProtocol.protocol，但适配器层值不变 |

**整体完成度估计**：✅ **100%**（Phase 1 ✅、Phase 2 ✅、Phase 3 ✅（后端）、Phase 4 ✅、Phase 5 ✅、Phase 6 ✅）。PLAN 中六项 Phase 均已落地；后续可选完善：模型管理页 `/admin/models`（PLAN 未涵盖）、API Key 加密存储、跨层集成测试。

### 8.3 核心变更（已完成）

| 方面 | 重构前 | 重构后（当前代码实况） |
|------|-----------------|------------------|
| 存储 | JSON 文件 + RwLock | ✅ toasty + SQLite 完全替换 |
| 认证 | Bearer Token（环境变量） | ✅ OIDC + Session + API Token 三通道 |
| 模型目录 | models.dev 直接驱动路由 | ✅ models.dev 集成**已删除**（多协议重构 Phase 4 大部分完成，仅剩 `ModelCatalogConfig` 残留）；运行时全走本地 SQLite 四表，见 §2.4 |
| Admin API | Bearer Token 认证 | ✅ Session + RBAC（SessionAuth / AdminAuth） |
| Token 管理 | 无 | ✅ 用户创建多 Token，每 Token 独立配额 + 模型范围 |
| API 认证 | Bearer Token（环境变量） | ✅ TokenAuth 提取器（bcrypt 验证 Token） |
| 前端 | 无认证 | ✅ OIDC 登录 + 5 个管理页面 |
| 数据模型 | 5 张表（ProviderModel 耦合能力+定价） | ✅ **7 张表**（LLMModel 标称 + ModelProvider 关联 + ProviderProtocol 协议，多协议重构后） |
| toasty 版本 | 0.6（BelongsTo / HasMany） | ✅ 0.7.0（关系字段全部为 `Deferred<T>`） |
| OpenTelemetry | 0.31 vs 0.32 冲突 | ✅ 统一为 0.32.x |

---

## 9. 总结

LLM-Bridge 当前处于**重构 Phase 4 基本完成、Phase 5 部分启动**。核心路径与前端管理界面均已实现，剩余工作集中在测试套件完善与文档定稿。

> 注：下方 Phase 1/2/3 描述为历史快照，部分字段名（如 `ProviderModel`）已在 Phase 4a 重构为 `ModelProvider` 并新增 `LLMModel` 表，随后在 2026-06-29 多协议重构中进一步新增 `ProviderProtocol` 表。当前实际为 **7 张核心表**：`users`、`tokens`、`usage_records`、`models`、`providers`、`provider_protocols`、`model_providers`。

**Phase 1 已完成：**
- 引入 `toasty` ORM + `jiff` 时间库，新增 `openidconnect` + `tower-sessions` + `bcrypt` + `rand` 依赖
- SQLite（默认）+ PostgreSQL（可选 feature）双数据库支持
- ~~5 张核心表数据模型定义完毕~~ → Phase 4a 重构后为 6 张表 → 多协议重构后为 **7 张表**（新增 `ProviderProtocol`，`User`, `Token`, `UsageRecord`, `LLMModel`, `Provider`, `ProviderProtocol`, `ModelProvider`）
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
- 删除 `src/store/providers.rs` — JSON 文件读写全部移除（物理删除）
- 新增 `src/store/router.rs` — 路由解析从 `model_providers` + `provider_protocols` + `models` + `providers` **四表**应用层关联查询，含加权轮询 KeySelector
- 新增 `src/store/compat.rs` — 原本为 npm → ProviderCompatibility 推导 + models.dev 数据映射；多协议重构后 models.dev 集成已删，仅保留枚举提示
- 重写 `src/server/admin.rs` — 认证从 Bearer Token 改为 Session（SessionAuth / AdminAuth）；提供者列表返回完整 ProviderResponse
- 更新 `src/server/openai_api.rs` — `list_models` / `resolve_model` → async；返回完整定价+能力
- 更新 `src/main.rs` — Store 由 `Store::new(db, path)` 创建，移除 JSON 文件依赖
- 更新 `src/actors/gateway_manager.rs` — 适配新 Store API
- ✅ ~~models.dev 代理导入端点 延后至 Phase 4~~ → 已在 Phase 4 实现：`/api/v1/admin/models-dev/search` + `/import`

**Phase 4 已基本完成：**
- 后端 Admin API：提供者 CRUD、ModelProvider 关联 CRUD、models.dev 搜索+导入、用户角色管理（见 2.2 节）
- 后端 Token 管理 API：`/api/v1/tokens` CRUD + 所有权验证（见 Phase 2）
- 后端用户管理 API：`/api/v1/admin/users` + `/api/v1/admin/users/{id}/role`
- 前端 5 页面：Login、Models、Tokens、Providers、Users（见 2.7 节）
- 前端路由 + RBAC 侧边栏 + 未认证自动跳转 /login
- TypeScript 绑定自动生成已打通（`cargo test export_bindings` + `cargo test generate_ts_client`）
- 残留：`openai_api.rs::check_auth` 死代码 + `AppState.auth_token` 字段未清理（见第 6 章）

**Phase 5 部分启动：**
- ✅ `docs/admin-api.md`、`docs/architecture.md` 已存在（骨架，待定稿）
- ✅ `tests/generate_ts_client.rs` 存在（生成测试可用；`check_ts_client_up_to_date` 当前被注释禁用）
- ✅ `src/db/mod.rs` 含 2 个 `#[tokio::test]`（建表 + CRUD 往返）
- ✅ 三适配器共 23 个 `#[test]`（anthropic_messages 7 + openai_responses 16）
- ⚠️ 合计 `src/` 下仅 25 个 `#[test]`/`#[tokio::test]`（不含 tests/ 与 doctest）；先前文档中"55 个测试"的说法存疑，待 `cargo test --no-run` 给出权威统计
- ⬜ 跨层集成测试（OIDC mock + Token CRUD + 配额扣除 + 端到端 API 调用）尚未补齐
- ⬜ `docs/deployment.md` 未编写

---

## 11. 当前行动项（多协议架构重设计，2026-07-03 核对）

> 取代 2026-06-25 旧 §11「切断 models.dev 硬依赖」计划，纳入 PLAN.md（2026-06-29）多协议架构重设计任务总线。STATUS 维护原则与 PLAN 第 1 章一致：**不向后兼容、解决问题优先、不自主做决定**。

### 11.1 待落地清单（按建议优先级排序）

| # | 项 | Phase | 优先级 | 关联文件 |
|---|----|-------|--------|---------|
| A | 实现 `upsert_protocols` 方法，对 ProviderProtocol 进行 diff upsert（增/改/删），与 PLAN §5.2 一致 | Phase 2 | 高 | ✅ 完成 2026-07-03 — `replace_provider_protocols` + 单条 CRUD |
| B | 在 `CreateProviderRequest`/`UpdateProviderRequest` 增加 `protocols: Vec<CreateProtocolEntry>` 字段，使 Provider 创建/更新时能附带协议 | Phase 3 | 高 | ✅ 完成 2026-07-03 — 以 `ProtocolInput` 复用单一类型，create/update 均同步 `replace_provider_protocols` |
| C | 新增两个协议子端点：`GET /api/v1/admin/providers/{id}/protocols`、`PUT /api/v1/admin/providers/{id}/protocols`（批量修改） | Phase 3 | 高 | ✅ 完成 2026-07-03 — handler + 路由 + TS 客户端自动生成 |
| D | 删除 `src/config/models.rs:46-72` 的 `ModelCatalogConfig` 与四个 `LLM_BRIDGE_CATALOG_*` 环境变量；移除 `main.rs:105` 的 `catalog_base_url` 日志 | Phase 4 | 中 | ✅ 完成 2026-07-03 |
| E | 在前端 `ProvidersPage.svelte` 实现协议列表增删改 UI（每项：协议类型下拉框 + URL 输入框 + compat settings 展开编辑） | Phase 5 | 高 | ✅ 完成 2026-07-03 — `ProvidersPage.svelte` 已实现列表展示+编辑表单+创建对话框中携带协议 |
| F | 重新生成 TypeScript 绑定（`cargo test export_bindings` + `cargo test generate_ts_client`），同步 `CreateProtocolEntry` 等新类型 | Phase 5 | 中 | ✅ 已同步 2026-07-03 — `ProtocolInput.ts`、`ProtocolView.ts`、`CreateProviderRequest.ts` + `UpdateProviderRequest.ts` 含 `protocols` 字段均已生成 |
| G | 补齐 ProviderProtocol CRUD 与四表 JOIN 的集成测试 | Phase 5（测试） | 中 | `src/db/mod.rs`、新增 `tests/` |
| H | 清理历史死代码：`openai_api.rs::check_auth` + `AppState.auth_token` 字段（PLAN 未要求但 STATUS §6 持续登记） | 持续维护 | 低 | `src/server/openai_api.rs`、`src/server/mod.rs` |

### 11.2 决策要点（从 PLAN.md 沿袭）

| 维度 | 决策 |
|------|------|
| ProviderProtocol 唯一约束 | **不加**（允许同协议多端点，支持负载均衡/分区域路由） |
| `api_keys` 空值场景 | 自部署无认证模型（如本地 Ollama）存空数组 `[]`；KeySelector 遇空数组应跳过该 Provider 而非 panic |
| 数据库迁移 | 不考虑向后兼容：删除旧 SQLite 文件，由 toasty 启动时据新 schema 自动建表；管理员通过 Admin API 手工重建 |
| 数据库表数 | **七张**：`users`、`tokens`、`usage_records`、`models`、`providers`、`provider_protocols`、`model_providers` |
| 运行时路由 | 始终只查本地四表（`models` + `model_providers` + `provider_protocols` + `providers`），不依赖任何远端源 |

### 11.3 开放问题（沿用 PLAN.md §6 与旧 §11 待决项）

1. **CatalogSource schema**：通用响应格式是否保留 models.dev 当前格式作为兼容子集？（不阻塞当前 Phase 1-3 推进）
2. **多源支持**：是否允许同时配置多个 CatalogSource？
3. **导入幂等性 / 后台同步冲突**：远端价格变动 vs 管理员本地修改如何取舍？
4. **前端引导流**：是否需要首启空配置时的"欢迎向导"？底线：纯手动配置三件套必须始终可用，向导只能加速。

> 以上开放问题不阻塞当前 Phase 1-5 的具体行动项（§11.1 的 A-G）。

