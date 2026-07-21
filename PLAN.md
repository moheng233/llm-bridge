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
| 10 | 南 | `convert_messages` | `system` 角色被降级为 `user`（Anthropic 适配器本支持独立 system prompt，入口永远收不到）；不支持 `developer` 角色 |
| 11 | 南 | `content_to_text` / `OpenAiContentPart` | 多模态 `image_url` part 被丢弃（内部 `LanguageModelDataPart` 与适配器图片映射已就绪，入口未接）；`input_audio` / `video_url` / `file` 不支持 |
| 12 | 南+北 | `ChatCompletionRequest` + 适配器 | `seed` / `frequency_penalty` / `presence_penalty` / `logit_bias` / `max_completion_tokens` 未反序列化、无通路 |
| 13 | 北+南 | `chat_completions` 错误处理 | 上游错误统一包装为 500 + `provider_error`，429 / 402 / 400 等语义状态码不透传，客户端无法做针对性重试 |
| 14 | 北 | `openai_chat_completions.rs` `map_event` | 增量 tool_call arguments 续传分支为空（`// For simplicity...`），非完整 OpenAI 增量语义；Anthropic / Responses 适配器已按缓冲累积处理 |

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

## 4. 不在本计划范围

- 后端 API 形态变更
- `allowedModels` 多选 UI
- 一次性 Token 下载/二次确认
- 国际化（i18n）
- E2E 测试
