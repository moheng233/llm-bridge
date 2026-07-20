# LLM-Bridge 开发计划

> 最后更新：2026-07-20
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

## 3. 不在本计划范围

- 后端 API 形态变更
- `allowedModels` 多选 UI
- 一次性 Token 下载/二次确认
- 国际化（i18n）
- E2E 测试
