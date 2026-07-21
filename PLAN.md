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

## 3. 后端：OpenAI 兼容层工具调用透传（chat_completions）

> 来源：2026-07-21 完善 `examples/chat_smoke_test.rs` 时发现。
> 现状：内部类型 `LanguageModelInputPart::ToolCall/ToolResult` 与各上游适配器（openai_chat_completions / openai_responses / anthropic_messages）已具备完整的工具调用双向映射，但 **HTTP 入口 `/v1/chat/completions` 未对客户端暴露**。

### 问题清单

| # | 位置 | 问题 |
|---|------|------|
| A | `server/openai_api.rs` — `ChatCompletionRequest` | 缺少 `tools` / `tool_choice` 字段，客户端无法声明可用工具 |
| B | `server/openai_api.rs` — `OpenAiMessage` | 缺少 `tool_calls` / `tool_call_id` 字段，客户端回放的 `assistant(tool_calls)` 与 `tool` 角色消息在反序列化时被静默丢弃（`serde` 默认忽略未知字段） |
| C | `server/openai_api.rs` — `convert_messages` | 未把 `tool` 角色 / `tool_calls` 映射为内部 `ToolCall` / `ToolResult` part |
| D | `server/openai_api.rs` — 非流式响应 | 聚合循环里 `LMResponsePart::ToolCall` 被 `Ok(_) => {}` 吞掉，响应 JSON 无 `tool_calls` |
| E | `server/openai_api.rs` — `stream_to_sse` | 工具调用仅发一个空数组占位 `delta.tool_calls = []`，未透传真实 `id` / `name` / `arguments` |
| F | `actors/provider/*` — `ProviderChatRequest` | 请求体未携带工具定义，适配器 `build_request_body` 也就无法向上游传 `tools` |

### 修复方案

#### 1. 扩展请求/消息类型（`server/openai_api.rs`）

```rust
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<OpenAiMessage>,
    #[serde(default)]
    pub stream: bool,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f64>,
    #[serde(default)]
    pub stream_options: Option<OpenAiStreamOptions>,
    // 新增 ↓
    pub tools: Option<Vec<OpenAiTool>>,          // OpenAI 标准：{type:"function", function:{name,description,parameters}}
    pub tool_choice: Option<serde_json::Value>,  // "auto" | "none" | {"type":"function","function":{"name":...}}
}

pub struct OpenAiMessage {
    pub role: String,
    #[serde(default)]
    pub content: OpenAiContent,
    pub name: Option<String>,
    // 新增 ↓
    pub tool_calls: Option<Vec<OpenAiMessageToolCall>>, // assistant 消息携带
    pub tool_call_id: Option<String>,                    // role=tool 时携带
}
```

#### 2. `convert_messages` 映射补齐

- `role == "assistant"` 且带 `tool_calls` → 生成 `LanguageModelInputPart::ToolCall`（`call_id` / `name` / `input` = 解析后的 arguments JSON）。
- `role == "tool"` → 生成 `LanguageModelInputPart::ToolResult`（`call_id` = `tool_call_id`，`content` = 文本 part）。
- 其余角色逻辑保持不变。

#### 3. `ProviderChatRequest` 携带工具定义

```rust
pub struct ProviderChatRequest {
    pub model: String,
    pub messages: Vec<LanguageModelChatMessage>,
    pub tools: Option<Vec<OpenAiTool>>,   // 或定义为与协议无关的内部 Tool 类型
    pub tool_choice: Option<serde_json::Value>,
}
```

三个适配器（`openai_chat_completions`、`openai_responses`、`anthropic_messages`）在 `build_request_body` 中把工具定义序列化为各自上游格式：
- openai chat completions：`tools` + `tool_choice` 原样透传；
- openai responses：`tools` 转换为 responses API 格式；
- anthropic：`tools` → `tools` 数组（`name` / `description` / `input_schema`）。

#### 4. 非流式响应透传 `tool_calls`

聚合循环中收集 `LMResponsePart::ToolCall`，在最终 `message` JSON 中输出：

```json
"message": {
  "role": "assistant",
  "content": "...",
  "tool_calls": [{"id":"call_xxx","type":"function","function":{"name":"...","arguments":"{...}"}}]
}
```

并将 `finish_reason` 设为 `"tool_calls"`（当存在工具调用时）。

#### 5. 流式 SSE 透传 `tool_calls`

`stream_to_sse` 中遇到 `LMResponsePart::ToolCall` 时，按 OpenAI chunk 格式输出：

```json
"delta": {"tool_calls": [{"index":0,"id":"call_xxx","type":"function","function":{"name":"...","arguments":"..."}}]}
```

注意：上游是增量推送 arguments 的场景下，当前适配器（`openai_chat_completions.rs` 的 `map_event`）对「续传 arguments」分支是空的（注释 `// For simplicity...`）。如需严格对齐 OpenAI 增量语义，需在适配器内按 `index` 累积 arguments 后再发完整 ToolCall，或改为透传增量片段。

#### 6. 验收方式

- `examples/chat_smoke_test.rs` 场景 3（工具调用回放）在修复后应能：
  1. 请求携带 `tools` 声明 `get_current_weather`；
  2. 上游真实返回 `finish_reason=tool_calls` 的响应并被桥接层透传；
  3. 客户端回填 `tool` 结果后拿到最终自然语言回复（端到端，而非回放）。
- `curl -s ... /v1/chat/completions -d '{"model":..., "messages":[...], "tools":[...]}'` 手工验证非流式与 `stream:true` 两种路径。

### 影响面

- 仅 `server/openai_api.rs` 的请求/响应外壳 + `ProviderChatRequest` 结构 + 三个适配器的 `build_request_body`；内部 `LanguageModelInputPart` 类型与上游映射逻辑已就绪，无需改动。
- `estimate_token_count`（配额预估）目前只统计 `content`，带 `tools` 时建议把工具定义 JSON 长度也计入，避免低估。

---

## 4. 不在本计划范围

- 后端 API 形态变更
- `allowedModels` 多选 UI
- 一次性 Token 下载/二次确认
- 国际化（i18n）
- E2E 测试
