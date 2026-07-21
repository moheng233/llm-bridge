# LLM-Bridge 开发计划

> 最后更新：2026-07-21
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
| 1 | 通用组件未接入 | 各页面仍重复手写 header/empty/error 布局，未使用已有的 `PageShell`/`SectionHeader`/`EmptyState`/`ErrorState` |
| 2 | `formatApiError` 未集成 | `useApiCall` 中仍用 `e.message` 原始错误，未走中文友好格式化 |
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

## 3. 后端：`/v1/chat/completions` OpenRouter 兼容性差距

> 来源：2026-07-21 对照 [OpenRouter API 文档](https://openrouter.ai/docs/api/api-reference/chat/create-a-chat-completion) 逐项核对。
> 图例：南向 = 客户端 → llm-bridge；北向 = llm-bridge → 上游 provider。
> 已完成（2026-07-21）：`tools` / `tool_choice` 声明与透传、`assistant(tool_calls)` / `tool` 消息双向映射、流式/非流式 `tool_calls` 输出。

### P0 — 已声明未生效 / 影响计费正确性

| # | 方向 | 位置 | 问题 |
|---|------|------|------|
| 1 | 南+北 | `server/openai_api.rs` → `ProviderChatRequest` → 三个适配器 | `temperature` / `max_tokens` / `top_p` 已反序列化但从未传给适配器；`ProviderChatRequest` 无采样参数字段，`build_request_body` 全部不携带。Anthropic `max_tokens` 硬编码 4096 |
| 2 | 北+南 | 三个适配器 `map_event` / 聚合循环 | **`usage` 完全未解析**：非流式响应无 `usage` 字段；流式从不发 usage chunk（`stream_options.include_usage` 无效）。配额结算只能按字符估算 |
| 3 | 北+南 | 适配器 chunk 类型 / `stream_to_sse` / 非流式响应 | `finish_reason` 真实值被丢弃：非流式硬编码 `"stop"`，流式 chunk 的 `finish_reason` 字段标 `#[allow(dead_code)]`；`length` / `content_filter` 永远透传不出来 |
| 4 | 南 | `estimate_token_count` | 配额预估未计入 tools（已修复 ✅），但结算应改用真实 usage（依赖 #2） |

### P1 — 常用功能缺失

| # | 方向 | 位置 | 问题 |
|---|------|------|------|
| 5 | 南+北 | `ChatCompletionRequest` + 适配器 | ~~`response_format`（JSON mode / JSON Schema 结构化输出）未反序列化、无通路~~ ✅ 2026-07-21 |
| 6 | 南+北 | `ChatCompletionRequest` + 适配器 | ~~`reasoning` / `reasoning_effort` 推理强度配置未反序列化、无通路~~ ✅ 2026-07-21 |
| 7 | 南 | `stream_to_sse` | ~~流式协议细节：不发结束哨兵 `data: [DONE]`；首包无 `role: "assistant"`；chunk `id` / `created` 硬编码~~ ✅ 2026-07-21 |
| 8 | 南 | 非流式响应 | ~~`id` 硬编码 `"chatcmpl-llm-bridge"`、`created` 硬编码 `0`，未透传上游真实值~~ ✅ 2026-07-21 |
| 9 | 南+北 | `ChatCompletionRequest` + 适配器 | ~~`stop`（停止序列，string \| string[]）未反序列化、无通路~~ ✅ 2026-07-21 |

### P2 — 常规参数与角色

| # | 方向 | 位置 | 问题 |
|---|------|------|------|
| 10 | 南 | `convert_messages` | ~~`system` 角色被降级为 `user`；不支持 `developer` 角色~~ ✅ 2026-07-21（随 P1 批次） |
| 11 | 南 | `content_to_text` / `OpenAiContentPart` | ~~多模态 `image_url` part 被丢弃~~ ✅ 2026-07-21（`data:` URI 解码 + http(s) 抓取，10 MiB/图、8 图/消息上限）；`input_audio` / `video_url` / `file` 仍不支持 |
| 12 | 南+北 | `ChatCompletionRequest` + 适配器 | ~~`seed` / `frequency_penalty` / `presence_penalty` / `logit_bias` / `max_completion_tokens` 未反序列化、无通路~~ ✅ 2026-07-21（OpenAI 系原生透传；Anthropic/Responses 仅映射 `max_completion_tokens`，其余 warn 忽略） |
| 13 | 北+南 | `chat_completions` 错误处理 | ~~上游错误统一包装为 500 + `provider_error`，语义状态码不透传~~ ✅ 2026-07-21（`ProviderError` + 启动信号，429/402/400 等真实状态码透传；流式错误 chunk 补 `code` 字段） |
| 14 | 北 | `openai_chat_completions.rs` `map_event` | ~~增量 tool_call arguments 续传分支为空~~ ✅ 2026-07-21（按 index 缓冲累积，finish/EOF 一次性发射完整 ToolCall，对齐 §4 语义基准 A2） |

### P3 — 按需（低频 / OpenRouter 特有）

| # | 方向 | 问题 |
|---|------|------|
| 15 | 南+北 | `n`（多候选）、`logprobs` / `top_logprobs`、`parallel_tool_calls`、`user`、`metadata` |
| 16 | 南+北 | OpenRouter 特有：`provider`（路由偏好）、`models`（fallback 列表）、`route`、`transforms`、`plugins`、`session_id`、`trace`、`service_tier`、`modalities`、`prediction`、`image_config`、`min_p` / `top_k` / `top_a` / `repetition_penalty` |
| 17 | 南 | assistant 消息 `refusal` 字段；tool 消息 content 为 part 数组（含图片）时仅提取 text |
| 18 | 南 | 错误 chunk 格式无 `code` 字段；`reasoning_details`（OpenRouter 结构化格式）未支持，当前仅以 DeepSeek 风格 `reasoning_content` 字符串透传 |
| 19 | 北 | 响应中 `images` / `audio` 多模态输出未解析 |

### 建议实施顺序

1. **P0 一批**：#1 采样参数贯通 + #2 usage 解析与透传（含流式 `include_usage`）+ #3 `finish_reason` 透传 —— 改动集中在 `ProviderChatRequest`、`LMResponsePart`（需新增 Usage part）、三个适配器与 `openai_api.rs` 外壳。
2. **P1 一批**：#5 `response_format` + #6 `reasoning` + #7/#8 流式协议细节 + #9 `stop`。
3. P2 / P3 按实际需求排期。

---

## 4. 协议无关 WebSocket RPC 接口 `/v1/ws`

> 设计定稿：2026-07-21。目标：提供与 OpenAI 兼容 HTTP 接口并列的第二种传输绑定，直接以 vscode `LanguageModelChatResponse` 的同构类型（`LMResponsePart`）收发，便于 vscode 插件接入。`docs/architecture.md` §5–6.1 的旧 `/ws` + `GatewayEnvelope` 草稿作废，以本节为准。

### 语义基准（最高原则）

`LMResponsePart` 的流式语义对齐 vscode `LanguageModelChatResponse`，冲突时优先修复实现而非文档化残缺：

- Text / Thinking：增量 append。
- ToolCall：参数增量累积完整后**一次性发完整 part**（`input` 必须是完整 JSON 对象），无增量 tool call 帧。`anthropic_messages`、`openai_responses` 已是此语义。

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
| A2 | ToolCall 流式语义修复 | `openai_chat_completions.rs` 适配器对齐语义基准：流状态改按 index 累积 `arguments`，`finish_reason: "tool_calls"` 或 EOF 时冲刷发射完整 ToolCall part；配单元测试（参照 `anthropic_messages.rs` 的 `tool_use_lifecycle_produces_tool_call`）。SSE 客户端收到完整 tool_call 的时机从流早期变为末尾（此前 arguments 本为空），属有意修正 | —（可与 A 并行） |
| B | 配额/usage 逻辑抽取 | 新建 `src/server/chat_common.rs`：从 `openai_api.rs` 抽出 `UsageAccumulator` / `settle_quota_with_actual_usage` / `estimate_token_count`，新增 `prepare_chat_request`（resolve_model → 白名单 → 预扣 → spawn ProviderActor → 取 stream，错误用结构化 `ChatPrepareError` 由两 handler 各自映射）；纯移动无行为变更 | A |
| C | WS handler | 新建 `src/server/ws.rs`：`ws_handler(State, TokenAuth, WebSocketUpgrade)` → 读写任务分离；chat 消费任务把 `ProviderStream` map 为信封帧，连接级 `HashMap<request_id, AbortHandle>` 支撑 cancel 与断连清理（含 provider 层 channel 关闭时中止上游请求的验证）；`server/mod.rs` 注册路由（已核实 axfetchum 0.1.4 `get()` 兼容 WS upgrade handler）。**契约测试** `tests/ws_chat_contract.rs`：mock provider 覆盖 chunk 序列 / 并发归属 / cancel 结算 / 401 / 坏帧 / 慢消费 | A + A2 + B |
| D | 文档与示例 | 重写 `docs/architecture.md` §6.1（删旧草稿与 ConnectionActor 时序）；新增面向插件开发者的 `docs/ws-api.md`；`examples/ws_chat_client.rs` 最小客户端（dev-dependencies 加 `tokio-tungstenite`） | C |

### 复用（不改动）

- `ProviderStream = Stream<Item = Result<LMResponsePart, String>>`（`src/actors/provider/mod.rs`）——与传输协议解耦，WS 只是新的消费端。
- `TokenAuth`（`src/middleware/token_auth.rs`）、`check_and_deduct` / `TokenQuotaContext::from_token` / `adjust_usage`（`src/auth/quota.rs`）。

### 明确否决

- 不落旧稿的 ConnectionActor / GatewayManager 转发层——WS handler 与 `openai_api` 同构，直接编排 store + ProviderActor，避免两套调用链漂移。
- 一请求一连接（无多路复用）方案；session cookie 双模式鉴权（session 用户无配额体系）；服务端非流式 chat（WS 上一切皆流，聚合是客户端的事）。

### 记录待办（不阻塞本节）

- Thinking part 目前三适配器均增量 append，vscode 侧为完整块语义；append 对渲染等价且 OpenAI/Anthropic 官方客户端均如此消费，保持现状，未来 vscode 侧有要求再按语义基准对齐。
- `LanguageModelDataPart.data` 数字数组冗余；连接存续期 token 吊销不踢人（首版接受，`ws-api.md` 注明）。

---

## 5. 不在本计划范围

- 既有 REST Admin API 与 `/v1/chat/completions` 的请求/响应形态变更（`/v1/ws` 为新增接口，不在此限）
- `allowedModels` 多选 UI
- 一次性 Token 下载/二次确认
- 国际化（i18n）
- E2E 测试（§4 的 WS 契约测试为协议级集成测试，不在此限）
