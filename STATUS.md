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
│   ├── models.rs               # RuntimeSettings, ProviderConfig, 环境变量解析
│   └── models_dev_catalog.rs   # models.dev HTTP 客户端（拉取 + 缓存）
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
│   ├── admin.rs                # Admin REST API（axfetchum 声明式路由）
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

## 5. 待实现功能（已被 PLAN.md 取代）

> 以下为旧计划的待实现项。新重构计划见 [`PLAN.md`](PLAN.md) 第 6 章 Phase 1-5。

| 功能 | 优先级 | 状态 |
|------|--------|------|
| OIDC 单点登录 + Session 管理 | Phase 1 | ⬜ |
| toasty + SQLite 数据库 | Phase 1 | ⬜ |
| 用户 API Token 体系（创建/验证/CRUD） | Phase 2 | ⬜ |
| 配额管理（请求次数 + Token 消耗 + 周期重置） | Phase 2 | ⬜ |
| 提供者与模型数据库存储 | Phase 3 | ⬜ |
| models.dev 导入辅助 | Phase 3 | ⬜ |
| 增强 `/v1/models`（能力 + 定价 + Token 过滤） | Phase 3 | ⬜ |
| Admin API（Session + RBAC） | Phase 4 | ⬜ |
| 前端 OIDC 登录 + Token 管理页面 | Phase 4 | ⬜ |
| 集成测试 | Phase 5 | ⬜ |

---

## 6. 代码质量

**开发规范：**
- **每次修改后必须运行 `cargo clippy`** — 所有 lint 警告必须在提交前清零

**优点：**
- **类型安全**：核心类型与 TypeScript 自动同步（`ts-rs`），前端 API 客户端自动生成（`axfetchum`）
- **架构清晰**：Actor 模型 + 适配器模式，职责分离良好
- **可观测性**：完善的 tracing 集成，每个 Actor 消息处理均有 span
- **流式处理**：SSE 流解析器自实现，正确处理分帧
- **错误处理**：各层均有 `thiserror` 定义的错误类型，HTTP 层统一转换为 JSON 错误响应
- **前端现代化**：Svelte 5 runes、TailwindCSS 4、声明式表格组件

**可改进点：**
- **无自动化测试**：`tests/` 目录仅有 TypeScript 客户端生成测试（且被注释），无业务逻辑测试
- **适配器重复代码**：三个适配器中 SSE 解码和流处理逻辑重复较多，可提取公共 SSE 工具模块
- **`unwrap()` 使用**：Store 中大量 `read().unwrap()` / `write().unwrap()`（RwLock），可考虑使用 `expect` 提供上下文
- **API Key 存储**：`providers.json` 中明文存储，后续可考虑加密

---

## 7. 构建与运行

```bash
# 构建
cargo build --release

# 运行
cargo run

# 生成 TypeScript 客户端类型
cargo test --test generate_ts_client

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
| Phase 1：基础设施 | ⬜ 未开始 | — |
| Phase 2：Token 体系 | ⬜ 未开始 | — |
| Phase 3：存储与 API | ⬜ 未开始 | — |
| Phase 4：Admin + 前端 | ⬜ 未开始 | — |
| Phase 5：测试与文档 | ⬜ 未开始 | — |

### 8.3 核心变更

| 方面 | 重构前（当前代码） | 重构后（PLAN 目标） |
|------|-----------------|------------------|
| 存储 | JSON 文件 + RwLock | toasty + SQLite |
| 认证 | Bearer Token（环境变量） | OIDC + Session + API Token |
| 模型目录 | models.dev 直接驱动路由 | models.dev 仅作发现，路由依赖本地 DB |
| Admin API | Bearer Token 认证 | Session + RBAC（Admin / Member） |
| Token 管理 | 无 | 用户创建多 Token，每 Token 独立配额 + 模型范围 |
| 前端 | 无认证 | OIDC 登录 + Token 管理页面 |

---

## 9. 总结

LLM-Bridge 当前处于**早期可用阶段**。核心路径（OpenAI 兼容 API → 模型路由 → 多提供者适配 → 流式响应）已完整实现。系统采用 JSON 文件持久化、自动从 models.dev 同步模型目录、通过加权轮询在多个 API Key 间负载均衡。前端管理界面覆盖模型浏览和提供者配置两大功能。

目前已制定完整的重构计划（[`PLAN.md`](PLAN.md)），后续将按 Phase 1→5 逐步实施，目标是将项目改造为面向团队内部使用的自托管 LLM 网关，支持 OIDC 单点登录、API Token 管理、配额控制等企业级功能。
