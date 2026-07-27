# LLM-Bridge 开发计划

> 最后更新：2026-07-24
> 项目定位：homelab / 小型工作室的私有化 OpenRouter

---

## 1. 当前状态

### 后端（Rust）

Provider 多协议架构已落地：Provider → ProviderProtocol（协议 + base_url）→ ModelProvider（protocol_id FK）→ LLMModel。models.dev 集成已完全删除。

### 前端（Vue 3）

已于 2026-07 从 Svelte 迁移至 Vue 3。

| 维度 | 选型 |
|------|------|
| 框架 | Vue 3 + Vite 8 + Vue Router 5 + Pinia 3 |
| 样式 | Tailwind CSS 4 + reka-ui + shadcn 风格组件 |
| 图标 | `@lucide/vue` |
| 字体 | Noto Sans SC + JetBrains Mono |
| 类型 | `ts-rs` 自动生成 bindings |
| 自动导入 | `unplugin-auto-import` + `unplugin-vue-components` |

### 前端已完成优化（2026-07-20）

| 优化项 | 交付物 |
|--------|--------|
| API 调用统一封装 | `composables/useApiCall.ts` — loading/error/execute 模式 |
| 响应式集合封装 | `composables/useReactiveCollections.ts` — `useReactiveMap` / `useReactiveSet` |
| 等宽字体修复 | `main.css` — `font-mono` 改用 JetBrains Mono |
| 品牌色语义化 | `main.css` — `--color-cta` / `--color-cta-hover` 注册到 Tailwind |
| 硬编码颜色消除 | 全部页面 `text-[#22C55E]` → `text-cta` |
| 页面过渡动画 | `App.vue` + `main.css` — 150ms fade + slide |
| 首页空白修复 | `index.vue` — 添加空 template 修复 Transition 渲染 |

### 通用组件（已建待用）

`components/common/` 已有 `PageShell`、`SectionHeader`、`EmptyState`、`ErrorState`、`ConfirmDialog`、`UnauthorizedPage`，但各页面尚未接入使用。

### 已有 composable（已建待用）

- `useApiCall` — 统一 API 调用 loading/error
- `useReactiveMap` / `useReactiveSet` — 响应式集合
- `useConfirm` — 删除确认对话框
- `formatApiError` — 错误格式化（`lib/utils/error.ts`）

---

## 2. 待优化项

### P0 — 高优先级（一致性与可维护性）

| # | 问题 | 说明 |
|---|------|------|
| 3 | 主内容区大面积留白 | 数据少时页面下 2/3 全空，需限制最大宽度或添加引导内容 |

### P1 — 中优先级（体验提升）

| # | 问题 | 说明 |
|---|------|------|
| 4 | 品牌感弱 | Logo 仅为绿色方块 "LB"，无 favicon 定制，登录页无品牌展示 |
| 5 | 表格行交互反馈不足 | 模型目录表格行可点击但无视觉反馈，缺 hover 过渡动画 |
| 6 | 表单体验粗糙 | Checkbox 无分组标签，数字输入无单位提示，缺表单验证 |

### P2 — 低优先级（打磨）

| # | 问题 | 说明 |
|---|------|------|
| 7 | 缺少快捷键 | 无 `Ctrl+K` 全局搜索等效率快捷键 |
| 8 | badge 语义不统一 | 启用/禁用有的用 default/secondary，有的用 default/destructive |
| 9 | 缺少用量统计仪表盘 | 无请求数、Token 消耗、费用等核心差异化功能（需后端配合） |

---

## 3. 后端：`/v1/chat/completions` OpenRouter 兼容性差距（剩余）

> 2026-07-21 全面核对：P0（#1–#4）与 P1（#5–#9）、P2（#10–#14）已全部落地并验证（采样参数贯通、usage 五元组解析与真实结算、finish_reason 透传、system/developer 角色、多模态 image_url、seed/penalty 等参数、上游状态码透传、增量 tool_call 累积发射），相应内容已从本节删除。图例：南向 = 客户端 → llm-bridge；北向 = llm-bridge → 上游 provider。

### P3 — 按需（低频 / OpenRouter 特有）

| # | 方向 | 问题 |
|---|------|------|
| 15 | 南+北 | `n`（多候选）、`logprobs` / `top_logprobs`、`parallel_tool_calls`、`user`、`metadata` |
| 16 | 南+北 | OpenRouter 特有：`provider`（路由偏好）、`models`（fallback 列表）、`route`、`transforms`、`plugins`、`session_id`、`trace`、`service_tier`、`modalities`、`prediction`、`image_config`、`min_p` / `top_k` / `top_a` / `repetition_penalty` |
| 17 | 南 | assistant 消息 `refusal` 字段；tool 消息 content 为 part 数组（含图片）时仅提取 text |
| 18 | 南 | `reasoning_details`（OpenRouter 结构化格式）未支持，当前仅以 DeepSeek 风格 `reasoning_content` 字符串透传 |
| 19 | 北 | 响应中 `images` / `audio` 多模态输出未解析 |

---

## 4. 协议无关 WebSocket RPC 接口 `/v1/ws`

> 设计定稿：2026-07-21。目标：提供与 OpenAI 兼容 HTTP 接口并列的第二种传输绑定，直接以 vscode `LanguageModelChatResponse` 的同构类型（`LMResponsePart`）收发，便于 vscode 插件接入。`docs/architecture.md` §5–6.1 的旧 `/ws` + `GatewayEnvelope` 草稿作废，以本节为准。

### 语义基准（最高原则）

`LMResponsePart` 的流式语义对齐 vscode `LanguageModelChatResponse`，冲突时优先修复实现而非文档化残缺：

- Text / Thinking：增量 append。
- ToolCall：参数增量累积完整后**一次性发完整 part**（`input` 必须是完整 JSON 对象），无增量 tool call 帧。三适配器均已对齐（`openai_chat_completions` 的修复已于 2026-07-21 完成，即原阶段 A2）。

### 协议要点

- **端点**：`GET /v1/ws`（Upgrade），握手阶段 `TokenAuth`（Bearer）鉴权，失败 401 不升级；subprotocol `lm-bridge.v1` 可选。
- **帧格式**：全 JSON text frame；信封含客户端生成的 `id`，服务端以 `result` / `chunk` / `done` / `error` 四者之一回显同一 `id`。
- **方法**：`chat`（流式，`chunk: LMResponsePart` 原样透传 × N → `done{finishReason}`）、`listModels`（→ `LMModelInfo[]`）、`cancel`（按 `targetId` 精确终止，补发 `done{cancelled}` 并按已产生 usage 结算）。
- **错误码**：`invalid_request` / `model_not_found` / `model_not_allowed` / `quota_exceeded` / `provider_error` / `request_not_found` / `internal_error`。
- **会话**：单连接多路复用（并发上限 8）；断连自动终止全部进行中请求并按实际 usage 多退少补；服务端 30s Ping 保活；出站 mpsc(64) 背压，慢消费关连接（1008）。

### 实施阶段

| # | 阶段 | 内容 | 依赖 |
|---|------|------|------|
| A | 协议类型 + TS 导出 | `src/types.rs`：为 `LMResponsePart` / `LanguageModelChatMessage` 等 15 个既有类型补 `#[derive(TS)]`（现状仅 3 个有）；新增 `WsChatParams` / `WsClientMessage`（`#[serde(tag="method")]`）/ `WsServerMessage`（result\|chunk\|done\|error）/ `WsErrorBody` / `WsErrorCode` / `WsChatDone` / `WsListModelsResult`；`cargo test generate_ts_client` 再生成绑定（含 untagged union 与 serde 判别行为的一致性断言测试） | — |
| B | 配额/usage 逻辑抽取 | 新建 `src/server/chat_common.rs`：从 `openai_api.rs` 抽出 `UsageAccumulator` / `settle_quota_with_actual_usage` / `estimate_token_count`，新增 `prepare_chat_request`（resolve_model → 白名单 → 预扣 → spawn ProviderActor → 取 stream，错误用结构化 `ChatPrepareError` 由两 handler 各自映射）；纯移动无行为变更 | A |
| C | WS handler | 新建 `src/server/ws.rs`：`ws_handler(State, TokenAuth, WebSocketUpgrade)` → 读写任务分离；chat 消费任务把 `ProviderStream` map 为信封帧，连接级 `HashMap<request_id, AbortHandle>` 支撑 cancel 与断连清理（含 provider 层 channel 关闭时中止上游请求的验证）；`server/mod.rs` 注册路由（已核实 axfetchum 0.1.4 `get()` 兼容 WS upgrade handler）。**契约测试** `tests/ws_chat_contract.rs`：mock provider 覆盖 chunk 序列 / 并发归属 / cancel 结算 / 401 / 坏帧 / 慢消费 | A + B |
| D | 文档与示例 | 重写 `docs/architecture.md` §6.1（删旧草稿与 ConnectionActor 时序）；新增面向插件开发者的 `docs/ws-api.md`；`examples/ws_chat_client.rs` 最小客户端（dev-dependencies 加 `tokio-tungstenite`） | C |

### 复用（不改动）

- `ProviderStream = Stream<Item = Result<LMResponsePart, String>>`（`src/actors/provider/mod.rs`）——与传输协议解耦，WS 只是新的消费端。
- `TokenAuth`（`src/middleware/token_auth.rs`）、`check_and_deduct` / `TokenQuotaContext::from_token` / `adjust_usage`（`src/auth/quota.rs`）。

---

## 4.1 vscode 插件登录鉴权（设备码流程，RFC 8628 风格）

> 设计定稿：2026-07-21。目标：vscode 插件无需手动复制粘贴即可获取 API token。与 §4 的 WS 接口配套：插件先经本节流程拿到 token，再以 Bearer 握手 `/v1/ws`。

### 流程（四端点）

| # | 端点 | 认证 | 说明 |
|---|------|------|------|
| 1 | `POST /api/v1/auth/cli-sessions` | 无 | 插件创建 CLI 会话，返回 `{ sessionId, userCode, verificationUrl, expiresIn, interval }`。userCode 为 6 位数字（不含 0/1/O/I 混淆字符），session 10 分钟过期，轮询间隔 5s |
| 2 | `GET /auth/cli-verify?code=` | Session | 浏览器验证页（前端路由或后端渲染）。未登录先走 `GET /auth/login?next=/auth/cli-verify?code=...`（复用现有 OIDC + `login_next` 机制；no-auth 模式中间件自动注入 admin session，两种模式行为统一） |
| 3 | `POST /api/v1/auth/cli-sessions/confirm` | Session | 验证页提交用户码 + 确认授权：服务端为当前 SessionUser 签发 token（见「token 属性」），标记会话 `approved`。防钓鱼：必须用户主动输入码 + 点确认，码不出现在 URL 时不授权 |
| 4 | `GET /api/v1/auth/cli-sessions/{sessionId}` | 无（sessionId 即凭证） | 插件轮询。`pending` → 202 + `{ status }`；`approved` → 200 + `{ status, token, tokenPrefix }`（**token 明文仅此一次**，响应后会话标记 `consumed`，再查返回 410）；`expired` → 410 |

### token 属性（自动默认 + 自动吊销）

- 名称 `vscode:<8位随机后缀>`；`allowed_models = []`（全模型）；配额 unlimited；归属当前 SessionUser。
- **自动吊销**：签发前列出该用户全部 token，`name` 以 `vscode:` 前缀匹配的旧 token 置 `active = false`（不物理删除，保留审计痕迹），保证同一用户同一时刻最多一个有效 vscode token，避免重复登录导致列表堆积。

### 数据模型

新增 `CliSession` 表（`toasty::models!` 注册 + `push_schema` 自动建表，无迁移负担）：`id`(key) / `user_code`(unique) / `status`(pending|approved|consumed|expired) / `user_id`(nullable，授权前未知) / `token_plaintext`(nullable，仅 approved 后暂存至 consumed) / `created_at` / `expires_at`。服务端惰性过期（查询时判 `expires_at`），无需后台清理任务。

### 实施步骤

| # | 内容 | 位置 |
|---|------|------|
| 1 | `CliSession` 模型 + 注册 `all_models()` | `src/db/models.rs`、`src/db/mod.rs` |
| 2 | 会话创建/查询/确认服务函数 + userCode 生成 + 自动吊销逻辑 | `src/auth/cli_session.rs`（新） |
| 3 | 四个路由 handler（`POST cli-sessions` / `GET cli-sessions/{id}` / `POST cli-sessions/confirm` / `GET cli-verify` 重定向页） | `src/server/auth.rs` 或新 `src/server/cli_auth.rs`，注册进 `auth_routes()`（ApiRouter，confirm 标 `.auth()`） |
| 4 | 前端验证页路由（输码 + 确认按钮，复用 `useApiCall`） | `frontend/src/pages/` + router |
| 5 | 文档：`docs/ws-api.md` 增加「插件登录」一节（完整流程时序 + curl 示例） | docs |

### 复用（不改动）

- `token::create_token` / `list_user_tokens` / `update_token`（`src/auth/token.rs`）；`SessionAuth` 提取器；`GET /auth/login` + `login_next` 重定向机制；no-auth 自动注入 admin session 的中间件。

### 明确否决

- 回环回调模式（`127.0.0.1` 临时服务器）——远程 SSH 场景不可用，已评估否决。
- 免确认直接签发（码在 URL 中拿到链接即授权）——防钓鱼底线保留「输码 + 确认按钮」。
- 为 CLI 单独发明 token 类型——复用现有 `Token` 表与 bcrypt 验证链路，`vscode:` 前缀仅是命名约定。


- 不落旧稿的 ConnectionActor / GatewayManager 转发层——WS handler 与 `openai_api` 同构，直接编排 store + ProviderActor，避免两套调用链漂移。
- 一请求一连接（无多路复用）方案；session cookie 双模式鉴权（session 用户无配额体系）；服务端非流式 chat（WS 上一切皆流，聚合是客户端的事）。

> §4.1 的登录鉴权为插件获取 token 的前置环节，与 WS 接口独立交付（WS 接口假设 token 已存在）。

### 记录待办（不阻塞本节）

- Thinking part 目前三适配器均增量 append，vscode 侧为完整块语义；append 对渲染等价且 OpenAI/Anthropic 官方客户端均如此消费，保持现状，未来 vscode 侧有要求再按语义基准对齐。
- `LanguageModelDataPart.data` 数字数组冗余；连接存续期 token 吊销不踢人（首版接受，`ws-api.md` 注明）。

---

## 5. AI 可观察性（请求追踪 + GenAI 遥测）

> 设计定稿：2026-07-21。范围：运营可观察性（metrics/traces）+ 用量计费归因 + 内容追踪三层；质量评估（eval）明确划出（网关是透传层，无 ground truth）。语义遵循 OpenTelemetry GenAI 语义约定（development 稳定性）。

### 核心模型：`LlmRequestTrace`（单一事实源）

一次请求生命周期产生的全部结构化事实落为**一行记录**，metrics 是其流式投影、计费查询是其 SQL 聚合、内容快照是其可空大字段——不建三套采集管线。

**生命周期状态机**：`pending → streaming → finalized`（success / error / cancelled）。请求开始时 INSERT（pending），结束时 upsert 终态——中途崩溃可见「卡住」的请求而非丢记录（Langfuse observation upsert 模式）。

**写入路径**：handler 热路径只发 mpsc 事件，专用后台任务批量落盘；mpsc 满则**丢弃并计数**（`dropped_traces_total` metric）——观察性数据可丢，业务请求不可阻塞。

### 表结构（`llm_request_traces`，toasty model 注册 `all_models()`）

> ✅ **表结构已落地（2026-07-22）**：`src/db/models.rs` 新增 `LlmRequestTrace` / `UsageDaily` 两个 toasty model（含 `TraceInterface` / `TraceStatus` Embed 枚举、`is_final()` 辅助方法），已注册 `all_models()`（7 → 9 张表），附 2 个集成测试验证 JSON 列、唯一/普通索引与枚举持久化。`interface` / `status` 字段实现为 `toasty::Embed` 枚举而非裸 String（类型安全，对齐 `ProviderCompatibility` 既有模式）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | u64 PK auto | |
| `request_id` | String unique idx | UUID，网关生成；响应头 `x-request-id` 回传，贯穿 stdout 日志 / OTel / DB |
| `trace_id` | Option\<String\> | OTel trace id（otel 启用时双写互查） |
| `interface` | String | `openai_http` \| `ws_rpc`（为 §4 WS 接口预留） |
| `token_id` / `user_id` / `token_prefix` | u64 / u64 / String | 归属（`token_hash` 永不进表） |
| `model` | String idx | 规范模型名 |
| `provider_id` / `provider_model_id` / `protocol` | String | 路由结果 |
| `status` | String | pending \| streaming \| success \| error \| cancelled |
| `error_type` / `error_message` / `upstream_status` | Option | 错误三元组 |
| `finish_reason` | Option\<String\> | 上游真实值 |
| `estimated_tokens` | i64 | 预扣量（解释配额结算 delta） |
| `input/output/reasoning/cached/total_tokens` | Option\<u64\> | 五元组（`LanguageModelUsagePart` 持久化形态） |
| `cost_usd` | Option\<f64\> | `model_providers` 定价 × usage（派生） |
| `upstream_request_id` | Option\<String\> | `ProviderResponseMetadata.id` |
| `created_at` / `first_chunk_at` / `completed_at` | Timestamp / Option×2 | 时间线；`ttft_ms`、`latency_ms` 派生存储 |
| `request_messages` / `response_parts` | Option\<Json\> | **内容快照（决策：包含）**，`Vec<LanguageModelChatMessage>` / 聚合后 `LMResponsePart` 序列；Opt-In 写入（见隐私分级） |

**预聚合表 `usage_daily`** ✅ 已落地（2026-07-22，同上）：`day` × `token_id` × `model` 三索引的 rollup（request_count、五元组 tokens、cost_usd 合计），由 finalize 事件同事务更新——仪表盘聚合查询不全表扫。

### 隐私分级与保留

- **运营数据**（默认可存）：token_id、模型、延迟、用量五元组、成本。
- **PII 敏感**（Opt-In）：`request_messages` / `response_parts` 仅当 `LLM_BRIDGE_OBS_CAPTURE_CONTENT=true` 时写入（对齐 OTel `gen_ai.input.messages` 的 Opt-In 约定）；建议配置独立更短 retention。
- **保留策略**：trace 表定期 `DELETE WHERE created_at < ?`（后台任务，`LLM_BRIDGE_OBS_TRACE_RETENTION_DAYS`，默认 30 天）；`usage_daily` 永久保留（已聚合无 PII）。

### OTel GenAI 遥测（仅 OTLP，决策：不接 Prometheus /metrics）

- **Span**：现有骨架 `chat_completions → provider_adapter_stream → stream_chat` 补齐 GenAI 属性——span 名改 `chat {model}`；属性 `gen_ai.operation.name=chat` / `gen_ai.provider.name`（protocol 映射：`openai`/`anthropic`/…）/ `gen_ai.request.model` / `gen_ai.request.stream` / `gen_ai.response.model` / `gen_ai.response.finish_reasons` / `gen_ai.usage.input_tokens` / `gen_ai.usage.output_tokens` / `error.type`；流式请求附 `gen_ai.response.time_to_first_chunk`。
- **Metrics**（新增 meter，opentelemetry crate 已依赖）：`gen_ai.client.token.usage`（histogram，by `gen_ai.token.type`=input/output）、`gen_ai.client.operation.duration`、`gen_ai.client.operation.time_to_first_chunk`——全部由 finalize 事件投影。
- **stdout 关联**：fmt 层输出 `trace_id` 字段（`tracing_opentelemetry::OpenTelemetrySpanExt`），stdout 日志 ↔ OTLP trace ↔ DB request_id 三方互查。

### request_id 贯穿（决策：手写 axum 中间件，不引 tower-http）

`from_fn` 中间件（与 `no_auth_middleware` 同款风格）：生成 UUID → `request.extensions_mut().insert(RequestId)` → handler → 响应头回写 `x-request-id`。WS handler 用 `Extension<RequestId>` 提取器在 upgrade 前读取（axum 的 Extensions 在 upgrade 前已就位，天然覆盖 §4）。tower-http `SetRequestId` 在 WS 路径拿不到 extensions，否决。

### 实施阶段

| # | 阶段 | 内容 | 依赖 |
|---|------|------|------|
| O1 | 请求标识 | ✅ **已完成（2026-07-22）**：手写 `RequestId` 中间件（`src/middleware/request_id.rs`，from_fn 风格对齐 `no_auth_middleware`）+ `x-request-id` 回传 + span 字段记录（中间件记录连接级 span、`chat_completions` handler 记录请求级 span）+ otel feature 下 stdout fmt 层 trace_id 输出（`OpenTelemetrySpanExt`）+ **span context 断点修复**（`openai_api.rs` SSE 结算 `tokio::spawn` 以 `.instrument(Span::current())` 挂回请求 span）。中间件置于路由最外层覆盖 WS upgrade 路径；含 3 个测试（UUID 合法性 / Display / 端到端 extension 注入与响应头一致性） | — |
| O2 | GenAI span/metrics | ✅ **已完成（2026-07-22）**：`src/observability/genai.rs` 新增——GenAI 属性键常量（无条件编译，stdout 日志同可携带）+ `provider_name()` 协议映射（OpenAiChatCompletions/OpenAiResponses→`openai`、AnthropicMessages→`anthropic`）+ `GenAiFinalize` 投影数据集 + `record_finalize()`（otel 下记录三个 metrics，无 otel 零开销空实现）。**span 属性补齐**：`chat_completions` instrument 加 `gen_ai.operation.name=chat`/`provider.name`/`request.model`/`request.stream`/`response.model`/`response.finish_reasons`/`usage.input_tokens`/`usage.output_tokens`/`response.time_to_first_chunk`/`error.type`，路由选定后 record provider/response.model，finalize 时 record usage/finish_reason/TTFT。**meter 接线**：`observability/mod.rs` 新增 `SdkMeterProvider`（`PeriodicReader` 异步运行时 Tokio + OTLP HTTP exporter），注册为全局 provider 并纳入 `ObservabilityGuard::shutdown`。**三个 GenAI metrics** 由 finalize 事件投影（流式于 SSE 结算 spawn、非流式于 handler 内，两路径均汇于 `settle_quota_with_actual_usage` 之后）：`gen_ai.client.token.usage`（u64 histogram，`{token}`，by `gen_ai.token.type`=input/output）/ `gen_ai.client.operation.duration`（f64，`s`）/ `gen_ai.client.operation.time_to_first_chunk`（f64，`s`，仅流式）——边界均对齐语义约定建议值。**TTFT 捕获**：`stream_to_sse` 注入 `TtftSlot`，上游首个 item 落槽计时。**注意**：`gen_ai.response.model` 以 `provider_model_name`（上游请求模型）近似（`ProviderResponseMetadata` 暂无上游响应 model 字段）；`error.type` 仅覆盖非流式流内错误路径（`stream_error`），其余错误路径（上游非 2xx/502）未挂钩——两处为有意的最小覆盖，留待 O3 finalize 汇集点统一。`Cargo.toml` otel 加 `rt-tokio` + `experimental_metrics_periodicreader_with_async_runtime`。91/92 测试通过（双 feature），clippy 无新警告 | O1 |
| O3 | trace 持久化 | ✅ **已完成（2026-07-22）**：异步写入器（`src/observability/trace_writer.rs`：有界 mpsc 1024 + 逐条落盘 + 满则丢弃计数 `dropped_traces_total`）+ finalize 挂钩（OpenAI HTTP 双路径：流式 SSE 结算 spawn 内、非流式 handler 内均发 `Finalize`；上游非 2xx/502/流内错误三路径发 error finalize，避免 pending 行卡住）+ 内容快照 Opt-In（`LLM_BRIDGE_OBS_CAPTURE_CONTENT`，pending 写 `request_messages`、finalize 写 `response_parts`；当前 response_parts 留空——流式热路径不逐条聚合，留待 O5 详情页增强）。`UsageDaily` 加复合唯一约束 `#[unique(day, token_id, model)]`（toasty 结构级，生成 `filter_by_day_and_token_id_and_model`），事务内 read-then-write upsert 保证 trace 与 rollup 原子。状态机 `pending → finalized`（pending INSERT 含 Opt-In 入参快照，崩溃可见「卡住」请求）。`RuntimeSettings.observability` 配置段落地（`capture_content` / `trace_retention_days`）。`interface` 固定 `OpenAiHttp`，WS handler 复用同一挂钩点传 `WsRpc` 即可（零成本扩展）。**注**：cost_usd / upstream_request_id 流式路径留空（O4 成本计算后回填 / SSE metadata 已消费）；trace_id 双写留待 otel 集成。94/95 测试通过（双 feature），clippy 无新警告 | O1，与 §4.B/C 并行 |
| O4 | 成本与查询 | 成本计算（读 `model_providers` 定价）+ `GET /api/v1/usage/summary` / `/api/v1/usage/traces`（分页/按 token/模型/时间筛选）+ 前端仪表盘页（对应 §2 P2#9） | O3 |
| O5 | 内容追踪 UI | trace 详情页（messages/parts 快照展示）+ 保留策略后台任务 | O3 |

### 配置（`RuntimeSettings.observability` 新增段，环境变量）

- `LLM_BRIDGE_OBS_CAPTURE_CONTENT`（默认 false）——内容快照开关
- `LLM_BRIDGE_OBS_TRACE_RETENTION_DAYS`（默认 30）
- OTel endpoint 维持标准 `OTEL_EXPORTER_OTLP_*` 环境变量（现状，不进 RuntimeSettings）

### 明确否决

- Prometheus `/metrics` 端点（决策：仅 OTLP）
- tower-http `SetRequestId`（WS 路径不可用，手写中间件替代）
- 质量评估 / eval 管线（网关无 ground truth）
- 分别为 metrics/计费/内容建三套采集管线（单一事实源原则）

---

## 6. 不在本计划范围

- 既有 REST Admin API 与 `/v1/chat/completions` 的请求/响应形态变更（`/v1/ws` 为新增接口，不在此限）
- `allowedModels` 多选 UI
- 一次性 Token 下载/二次确认
- 国际化（i18n）
- E2E 测试（§4 的 WS 契约测试为协议级集成测试，不在此限）

---

## 7. 从 models.dev 派生 JSON 的三层导入管道（2026-07-24 决策定稿）

> 目标：在管理界面增加导入功能，把 models.dev 的模型/提供者/模型链接三层数据导入 llm-bridge。**不消费 models.dev 官方 models.json/api.json**（api.json 的 provider 侧裸 id 无法无歧义映射回规范模型——`github-models` 源 `xai/grok-3` 会错误拼成 `github-models/xai/grok-3` 而非 `xai/grok-3`，属 models.dev 缺陷）；改为基于 `anomalyco/models.dev` `dev` 分支的三表 TOML 源（官方 `base_model` 外键）**自建派生 JSON 管道**，脚本与 Actions 全部落在**本仓库**，GitHub Pages 托管，程序以单一可覆盖 URL 为数据源。

### 7.1 总体架构

```
anomalyco/models.dev (dev 分支, TOML 三层)
   models/{family}/{model}.toml          → LLMModel（provider 无关标称数据）
   providers/{id}/provider.toml          → Provider + ProviderProtocol
   providers/{id}/models/{model}.toml    → ModelProvider（价格 + provider 侧 id + 覆盖）
        │  base_model = "{family}/{model}"（官方外键，消除 id 歧义）
        ▼
本仓库 scripts/models-dev-catalog/（Node 20+ TypeScript，Bun 运行）
   扫描源 TOML → 三层合并（provider model 继承 base_model 元数据，本地字段优先）
        ▼
GitHub Actions（.github/workflows/models-dev-catalog.yml）
   定时 cron（每日）+ workflow_dispatch + 源仓库 push 触发
        ▼
GitHub Pages（本仓库 gh-pages 分支）
   https://moheng233.github.io/llm-bridge/catalog.json  ← 默认数据源 URL
   https://moheng233.github.io/llm-bridge/contract.json ← schema 契约
        ▼
llm-bridge 运行时：modelsImport.sourceUrl（配置文件 + LLM_BRIDGE_MODELS_IMPORT_URL 覆盖）
   admin 手动触发预览/导入 → 幂等 upsert LLMModel / Provider / ProviderProtocol / ModelProvider
```

### 7.2 已验证的源 schema（anomalyco/models.dev dev 分支，2026-07-24 第一手核实）

| 源 | 关键字段 |
|----|---------|
| `models/{family}/{model}.toml` | `name, family, release_date, last_updated, knowledge?, attachment, reasoning, tool_call, structured_output?, temperature?, open_weights, license?, [limit]{context?, input?, output?}, [modalities]{input[], output[]}, links?/weights?/benchmarks?` |
| `providers/{id}/provider.toml` | `name, npm, env[], doc, api?`（`api` 仅 `@ai-sdk/openai-compatible` 时必有） |
| `providers/{id}/models/{model}.toml` | `base_model="{family}/{model}"`, `[cost]{input, output, reasoning?, cache_read?, cache_write?, input_audio?, output_audio?}`（USD/1M tokens）, 可选 `[limit]`/`[modalities]` 覆盖, `status?`(alpha/beta/deprecated), `base_model_omit?` |

### 7.3 派生 JSON 契约（直接对齐 llm-bridge 导入需求）

`catalog.json`：

```jsonc
{
  "generatedAt": "RFC3339",
  "sourceRev": "anomalyco/models.dev commit sha",
  "schemaVersion": 1,
  "models":    [{ "modelName": "openai/gpt-5.4", "displayName": "GPT-5.4", "description?": "...",
                  "maxInputTokens": 272000, "maxOutputTokens": 128000,
                  "toolCalling": true, "vision": true, "thinking": true, "adaptiveThinking": false }],
  "providers": [{ "providerId": "openai", "displayName": "OpenAI",
                  "baseUrl": "https://api.openai.com/v1", "compat": "openAiChatCompletions" }],
  "links":     [{ "providerId": "openai", "protocolKey": "openAiChatCompletions|https://api.openai.com/v1",
                  "modelName": "openai/gpt-5.4", "providerModelId": "gpt-5.4",
                  "inputPricePer1m?": 1.25, "outputPricePer1m?": 10.0, "cacheReadPricePer1m?": 0.125,
                  "enabled": true }]
}
```

映射规则（决策已定）：
- `modelName = {family}/{id}`（源 `models/` 路径）；`providerModelId` = provider 侧原始 id。
- `maxInputTokens = limit.input ?? limit.context ?? 4096`；`maxOutputTokens = limit.output ?? 4096`。
- `vision = modalities.input 含 "image"`；`thinking = reasoning`；`adaptiveThinking` 恒 `false`。
- `compat`：`npm` 含 `anthropic` → `anthropicMessages`，否则 → `openAiChatCompletions`（保守默认）。
- `links.enabled`：`status == "deprecated"` → `false`，其余 → `true`。
- **缺口显式记录**：`cache_write` 在 `ModelProvider` 无对应列，丢弃；`reasoning/input_audio/output_audio` 同理丢弃。

`contract.json`：`{ "schemaVersion": 1, "fields": {...} }`，供 llm-bridge 拉取时校验兼容性，未来升版不破坏旧消费方。

### 7.4 本仓库内落地（派生管道）

| 组件 | 说明 |
|------|------|
| `scripts/models-dev-catalog/` | 独立 Node/TS 子包（Bun 运行）：`fetch`（git sparse checkout 或 GitHub tarball 拉源 dev 分支）→ `parse`（TOML）→ `merge`（base_model 继承合并）→ `emit`（写 `dist/catalog.json` + `dist/contract.json`）→ `validate`（schema 校验 + 引用完整性：每条 link 的 modelName 必须存在于 models） |
| `.github/workflows/models-dev-catalog.yml` | `on: schedule(cron: "17 3 * * *")`（UTC 每日）+ `workflow_dispatch`；检出本仓库 → Bun 安装依赖 → 跑生成脚本 → 校验 → 将 `dist/` 发布到 `gh-pages` 分支（`peaceiris/actions-gh-pages@v4` 或 `actions/deploy-pages`） |
| Pages 开启 | 仓库 Settings → Pages → Source = `gh-pages` 分支根目录（一次性手工设置） |

幂等性：同一 sourceRev 重复运行产出字节级一致 JSON（键排序固定、时间戳唯一变动字段），Pages diff 仅含真实数据变化。

### 7.5 llm-bridge 侧导入

**配置**：`RuntimeSettings` 新增 `modelsImport.sourceUrl`（默认 `https://moheng233.github.io/llm-bridge/catalog.json`），环境变量 `LLM_BRIDGE_MODELS_IMPORT_URL` 覆盖。

**Store 新增**（`src/store/mod.rs`，均为按业务键 upsert，幂等）：
- `get_model_by_name` / `upsert_model_by_name(ModelInput)`（model_name unique）
- `upsert_protocol_by_key(provider_id, protocol, base_url)`（ProviderProtocol 无 unique 约束，先查后插；同 protocol+base_url 复用）
- `upsert_model_provider(model_id, protocol_id, link fields)`（按 `(model_id, protocol_id)` 唯一键查；存在→更新价格/启用/覆盖字段，不存在→create）
- **不直接复用** `add_provider_model`（无脑 create，重导入必撞 `(model_id, protocol_id)` 唯一约束）与 `ensure_model`（存在即返回、不更新标称字段——导入场景需要覆盖式 upsert）。

**端点**（`src/server/models_dev.rs` 新文件，挂 `admin_crud_routes()`，`AdminAuth`）：
- `GET /api/v1/admin/models-import/preview`：拉 catalog.json（reqwest，参考 `src/quota/adapters/umans.rs:54` 模板；`If-None-Match`/`If-Modified-Since` 条件拉取）→ 与 DB diff → 返回三层预览项（各标 `exists: bool` → 新建/更新）。
- `POST /api/v1/admin/models-import`：请求体 `{ models: Vec<modelName>, providers: Vec<providerId>, links: Vec<linkKey> }`；单事务逐层 upsert（providers → protocols → models → links），响应 `{ created, updated, skipped, errors }`。
- **api_keys 保留策略**：`upsert_provider` 会整列覆盖 `api_keys`（已核实 `src/store/mod.rs:91-135`）——导入 provider 时**永不携带 api_keys**（派生 JSON 本无密钥），新建 provider 时置空数组，由管理员在 UI 手工补 key；不得因导入清空已有 key → 实现时 upsert 路径对已有 provider **不传 api_keys 字段**（只更新 display_name/enabled/priority），新建才给空。

**前端**：
- 共享组件 `components/models/CatalogImportDialog.vue`（预览 + 搜索 + 前缀筛选 + 三层分组 checkbox + 全选/清空 + 导入进度 toast）。
- 入口 1：`pages/admin/models.vue` `SectionHeader #actions` 加「从 models.dev 目录导入」（模型视角：勾选 models 时联动带出可选 links）。
- 入口 2：`pages/providers.vue` `SectionHeader #actions` 加同按钮（提供者视角：勾选 providers/links 时联动校验依赖 models）。
- 绑定：`cargo test export_bindings` + `cargo test generate_ts_client` 重新生成。

### 7.6 实施步骤

- [ ] **Phase 1 — 派生管道**：`scripts/models-dev-catalog/`（含单测：合并规则、引用完整性、idempotency 快照）+ workflow + 开启 Pages → 手动触发一次产出首批 catalog.json/contract.json 并 curl 验证。
- [ ] **Phase 2 — 后端**：`RuntimeSettings.modelsImport`、`src/server/models_dev.rs`（fetch + diff + 两个端点）、store 四个 upsert 方法（含 api_keys 保留逻辑）+ 单测（upsert 幂等、api_keys 不被清空、link 冲突转更新）。
- [ ] **Phase 3 — 前端**：`CatalogImportDialog.vue` + 两个入口接线 + 绑定重生成 → 端到端手动验证（导入 → 重导入显示全「更新」、DB 无重复行、已有 provider 的 api_keys 保留）。

### 7.7 排除项与遗留

- 不做定时自动同步到 DB（仅 admin 手动触发）；不消费 models.dev 官方 models.json/api.json；不导入 benchmarks/links/weights/license/open_weights（无目标列，超范围）。
- 遗留（需 migration 时才做）：`ModelProvider.cache_write_price_per_1m` 列；`status`（deprecated 当前仅映射到 `enabled=false`，不入库 status 文本）。
- GitHub Actions/Pages 的可用性以 `origin = moheng233/llm-bridge` 私有/公开属性为准：若仓库为 private 且未启用 Pages，Phase 1 验收时需先确认 Pages 可公开访问 catalog.json（private 仓库的 Pages 默认仍公开）。
