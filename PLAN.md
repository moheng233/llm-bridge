# LLM-Bridge 重构计划：Provider 多协议架构重设计

> 日期：2026-06-29
> 目标：废弃 `npm` 字段，新增 `ProviderProtocol` 表将协议从隐式推导改为显式声明。同步删除 models.dev 导入链路。
>
> **设计原则：不考虑向后兼容。现有数据结构大概率不满足新需求，将进行大幅重构。代码追求简洁、明确。**

---

## 1. 指导原则

| 原则 | 说明 |
|------|------|
| **不方案回退** | 一旦确认方案就持续推进，不在开发中途因犹豫而退回旧方案 |
| **解决问题优先** | 遇到障碍先想办法解决，而不是绕过或降低需求 |
| **不自主做决定** | 任何需要权衡取舍的决策都需先讨论确认，不在代码中私自拍板 |
| **用 `cargo add` 加依赖** | 添加新 crate 时统一使用 `cargo add`，保持 `Cargo.toml` 格式一致 |
| **不考虑向后兼容** | 现有数据结构不满足新需求，所有改动不保留兼容层 |
| **简洁明确** | 避免过度抽象。能用 `String` 的地方不用泛型，能直接查 SQLite 的地方不加缓存层 |

---

## 2. 核心数据模型变更

### 2.1 变更概览

```
Before (3 表):
  Provider (npm/base_url/compat_settings/api_keys) ──< ModelProvider (compatibility 枚举) >── LLMModel

After (4 表):
  Provider (api_keys 保留，其余下沉)
   ├── ProviderProtocol (protocol + base_url + compat_settings)     ← 新表
   └── ModelProvider (protocol_id FK，不再存枚举) ──> LLMModel
```

| 表 | 删除 | 新增 |
|---|---|---|
| **Provider** | `npm`, `base_url`, `compat_settings` | `#[has_many] protocols` |
| **ProviderProtocol** (新表) | — | `protocol`, `base_url`(必填), `compat_settings`, `enabled`, `priority` |
| **ModelProvider** | `compatibility` | `protocol_id` FK → ProviderProtocol |

### 2.2 新表: `provider_protocols`

```rust
#[derive(Debug, toasty::Model)]
#[table = "provider_protocols"]
pub struct ProviderProtocol {
    #[key]
    #[auto]
    pub id: u64,

    #[index]
    pub provider_id: u64,
    #[belongs_to(key = provider_id, references = id)]
    pub provider: Deferred<Provider>,

    /// 协议枚举：OpenAiChatCompletions / OpenAiResponses / AnthropicMessages
    pub protocol: ProviderCompatibility,

    /// 必填，每协议独立 URL
    pub base_url: String,

    /// 自定义 HTTP 设置（JSON 对象字符串，对应 CompatibilitySettings）
    pub compat_settings: Option<String>,

    pub enabled: bool,
    /// 多协议间的优先级
    pub priority: i64,

    #[auto]
    pub created_at: Timestamp,
    #[auto]
    pub updated_at: Timestamp,
}
```

### 2.3 Provider 表变更

**删除**: `npm: Option<String>`, `base_url: Option<String>`, `compat_settings: Option<String>`

**修改**: `api_keys: String` → `api_keys: toasty::Json<Vec<ApiKeyEntry>>`（toasty 原生 JSON 列，不再手动 `serde_json::from_str`）

**新增**: `#[has_many] protocols: Deferred<Vec<ProviderProtocol>>`

**保留**: `id`, `provider_id`, `display_name`, `enabled`, `priority`, `created_at`, `updated_at`, `#[has_many] model_links`

`api_keys` 保留在 Provider — 跨协议共享，加权轮询选择 Key。

### 2.4 ModelProvider 表变更

**删除**: `compatibility: ProviderCompatibility`

**新增**: `protocol_id: u64` + `#[belongs_to(key = protocol_id, references = id)] protocol: Deferred<ProviderProtocol>`

---

## 3. 路由解析最终流程

```
resolve_model(model_name)
  → 查询 LLMModel (按 model_name)
  → 查询 ModelProvider (按 model_id, enabled, 按 priority 排序)
  → 对每个 ModelProvider:
      → JOIN ProviderProtocol (按 protocol_id)  → 获取 base_url, compatibility, compat_settings
      → JOIN Provider (按 provider_id)           → 获取 api_keys (已是 Vec<ApiKeyEntry>), 执行 KeySelector
  → 构建 ResolvedProviderRoute 列表
```

ResolvedProviderRoute 结构：

```rust
pub struct ResolvedProviderRoute {
    pub model_name: String,
    pub capabilities: LMModelInfo,
    pub provider_name: String,
    pub provider_model_name: String,
    pub priority: u32,
    pub compatibility: ProviderCompatibility,   // 来自 ProviderProtocol
    pub compat_settings: Option<CompatibilitySettings>, // 来自 ProviderProtocol
    pub base_url: Option<String>,               // 来自 ProviderProtocol（必填）
    pub api_key: String,                        // 来自 Provider，KeySelector 选择
    pub key_label: String,
}
```

---

## 4. 决策汇总

| 决策 | 结论 |
|------|------|
| `compat_settings` | 下沉到 ProviderProtocol |
| `base_url` | 从 Provider 移除，ProviderProtocol 必填 |
| `api_keys` 归属 | 保留在 Provider，跨协议共享 |
| `api_keys` 类型 | `String` → `toasty::Json<Vec<ApiKeyEntry>>`，消除 stringly-typed 反模式 |
| `ModelProvider.compatibility` | 删除枚举字段，改为 `protocol_id` FK |
| models.dev 集成 | 完全删除 |
| `npm` 字段 | 完全删除 |
| 向后兼容 | 不考虑 |
| ProviderProtocol 唯一约束 | 不加（允许同协议多端点） |

---

## 5. 实施步骤

### Phase 1: 数据模型定义

**关键文件**: `src/db/models.rs`

1. 新增 `ProviderProtocol` struct（derive `toasty::Model`，表名 `provider_protocols`）
2. 修改 `Provider`: 删除 `npm`, `base_url`, `compat_settings`; `api_keys` 改为 `toasty::Json<Vec<ApiKeyEntry>>`; 添加 `#[has_many] protocols`
3. 修改 `ModelProvider`: 删除 `compatibility`; 添加 `protocol_id` + `#[belongs_to]`
4. 确保 `ApiKeyEntry` 定义可被 `db::models` 引用（当前在 `src/config/models.rs`，需确认引用路径或移入）

### Phase 2: Store 层适配

**关键文件**: `src/store/mod.rs`, `src/store/router.rs`, `src/store/compat.rs`

1. 修改 `upsert_provider`: 删除 `npm`, `base_url`, `compat_settings` 参数; `api_keys` 参数类型改为 `Vec<ApiKeyEntry>`
2. 新增 `upsert_protocols` 方法: 批量 upsert ProviderProtocol 条目（diff 增/改/删）
3. 修改 `delete_provider`: 级联删除 ProviderProtocol
4. 修改 `ResolvedProviderRoute` 构建: `base_url`/`compatibility`/`compat_settings` 从 ProviderProtocol JOIN 获取
5. 修改 `resolve_model`: 四表 JOIN（models + model_providers + provider_protocols + providers）
6. 修改 `KeySelector`: 直接接收 `&[ApiKeyEntry]` 而非从 JSON 字符串解析
7. 删除 `src/store/compat.rs` 中 `npm_to_compatibility()`

### Phase 3: Admin API 适配

**关键文件**: `src/server/admin.rs`

1. `CreateProviderRequest`/`UpdateProviderRequest`: 删除 `npm`, `base_url`, `compat_settings`; `api_keys` 改为 `Vec<ApiKeyEntry>`; 新增 `protocols: Vec<CreateProtocolEntry>`
2. `AddModelRequest`/`UpdateModelRequest`: `compatibility` 字段改为 `protocol_id: u64`
3. 新增端点: `GET /api/v1/admin/providers/:id/protocols` 查看协议列表; `PUT /api/v1/admin/providers/:id/protocols` 批量修改
4. 删除 models.dev 相关路由和 handler（`/admin/models-dev/search`, `/admin/models-dev/import`, `search_models_dev`, `import_models_dev`）

### Phase 4: 删除 models.dev 集成

**关键文件**: 多个

1. 删除 `src/models_dev.rs`
2. 删除 `src/config/models_dev_catalog.rs` + `pub mod models_dev_catalog` 声明（`src/config/mod.rs`）
3. 删除 `src/store/catalog.rs`
4. 从 `src/store/mod.rs` 删除: `import_from_models_dev`, `search_catalog_providers`, `replace_catalog_cache`, `get_catalog_metadata`, `catalog_provider_count` 及相关类型（`CatalogProviderSummary`, `ImportedProvider`, `ImportedModel`）
5. 从 `src/store/compat.rs` 删除: `ModelCapabilitiesFromDev::from_models_dev`, `ModelPricingFromDev::from_models_dev`, `deduce_base_url`（以及相关的 `ModelCapabilitiesFromDev`、`ModelPricingFromDev` 结构体）
6. 从 `src/actors/gateway_manager.rs` 删除: `initialize_catalog`, `refresh_catalog` 及相关 Task 定义
7. 删除 `data/llm-bridge/catalog_cache.json`

### Phase 5: 前端适配

**关键文件**: `frontend/src/lib/ProvidersPage.svelte`, `frontend/src/bindings/`

1. 删除 models.dev 相关 binding 文件: `CatalogProviderSummary.ts`, `ImportModelsDevRequest.ts`, `ImportedProvider.ts`
2. 重新生成 TypeScript 绑定（运行 `cargo test` 中的 `generate_ts_client`）
3. 修改 `ProvidersPage.svelte`:
   - 删除"从 models.dev 导入提供者"按钮和对话框（`loadCatalog` 调用）
   - 创建/编辑 Provider 表单: 协议配置改为可增删列表（每项含协议类型下拉框 + URL 输入框 + compat settings 展开）
4. 模型关联表单: 协议从 Provider 已有协议列表中选取（而非自由输入 compatibility 字符串）

### Phase 6: Actor / 适配器确认

**关键文件**: `src/actors/provider/adapters.rs`

确认 `stream_chat` 分发逻辑无需改动 — `state.compatibility` 仍然来自路由解析后的 `ResolvedProviderRoute.compatibility`，数据源由 `ModelProvider.compatibility` 枚举字段变为 `ProviderProtocol.protocol`，但到达适配器层的值不变。

---

## 6. 进一步考量

### 6.1 ProviderProtocol 不设唯一约束

允许同一 Provider 下多个相同协议条目（如两个 `OpenAiChatCompletions` 指向不同 URL），支持同一协议的多端点负载均衡或分区域路由。

### 6.2 api_keys 空值场景

如果自部署模型无需认证（如本地 Ollama），`api_keys` 存空数组 `[]`。`KeySelector::select_key` 遇空数组时 Router 应跳过该 Provider（而非 panic）。

### 6.3 ApiKeyEntry 定义位置

当前 `ApiKeyEntry` 定义在 `src/config/models.rs`，包含 `Serialize + Deserialize`。改为 `toasty::Json<Vec<ApiKeyEntry>>` 后需要确认:
- `ApiKeyEntry` 需实现 `Serialize + Deserialize`（已有）
- `db::models.rs` 能引用到该类型（或将其移至 `db::models` 模块）

### 6.4 数据库迁移

当前设计不考虑向后兼容，迁移方式为:
1. 删除旧 SQLite 数据库文件
2. 启动时 toasty 根据新 schema 自动建表
3. 管理员通过 Admin API 重新创建 Provider + 协议 + 模型关联

---

# Part II: 前端可用性与重构计划

> 日期：2026-07-07
> 适用范围：`frontend/`（Svelte 5 + Vite 8 + Tailwind 4 + shadcn-svelte）
> 与 Part I 关系：独立推进，不依赖后端 schema 变更。Part I 的「Phase 5 前端适配」完成后如与之发生冲突，以本文档为前端权威。

---

## 7. 现状评估

### 7.1 整体结论

**基本可用，处于 "MVP 可跑、但不够稳/不够美" 阶段。** 普通用户侧（模型目录、Token 管理）体验尚可；管理员侧（Provider 管理、模型管理）单文件巨大、交互复杂、维护成本高。

### 7.2 技术栈快照

| 维度 | 选型 |
|------|------|
| 框架 | Svelte 5（runes）+ Vite 8 |
| 样式 | Tailwind 4 + bits-ui + shadcn-svelte 风格组件 |
| 图标 | `@lucide/svelte` |
| 路由 | `svelte-spa-router`（hash 路由） |
| 状态 | runes（`$state` / `$derived` / `$effect`）+ 自建 stores |
| 类型 | `ts-rs` 自动生成 bindings |
| 字体 | IBM Plex Sans + JetBrains Mono |

### 7.3 现有页面

| 路由 | 组件 | 行数 | 角色 |
|------|------|------|------|
| `/login` | `LoginPage.svelte` | ~20 | 所有未登录用户 |
| `/models` | `ModelsPage.svelte` | ~200 | 普通用户 |
| `/tokens` | `TokensPage.svelte` | ~230 | 普通用户 |
| `/admin/models` | `AdminModelsPage.svelte` | ~900 | 管理员 |
| `/providers` | `ProvidersPage.svelte` | ~1100 | 管理员 |
| `/users` | `UsersPage.svelte` | ~120 | 管理员 |

### 7.4 主要亮点

- Svelte 5 runes + Tailwind 4 + shadcn-svelte 技术选型现代且一致
- 主题系统完善（FOUC 预防 + localStorage + 系统偏好跟随）
- ts-rs 自动生成类型，前后端契约稳定
- 基础 UX 齐全（skeleton、empty state、error alert）

### 7.5 主要痛点

最大问题在管理员侧：两个页面单文件巨大（900~1100 行），单一职责严重失衡；且全站缺少数据层抽象、全局反馈、路由守卫。

> **当前进度**：首批基础设施已完成（A1 错误格式化、A7 全局 toast、B2 展示组件、B3 useConfirm hook、B4/B5 数据层 store），详见 §13 实施进度。各页面接入替换与剩余项待后续推进。

---

## 8. 问题清单

### 8.1 可用性 / 交互

| # | 状态 | 问题 | 位置 |
|---|------|------|------|
| A1 | ✅ 完成 | `catch (e: any) { error = e.message }` 直接展示后端原始字符串，无重试、无分类 — 已建 `formatApiError`（状态码映射中文主文案 + 原始 message 收 tooltip） | 全部页面 |
| A2 | ⬜ 待做 | 加载失败后 `loading=false`，列表空白，用户只能刷新 | 全部页面 |
| A3 | ⬜ 待做 | 创建 Token 表单缺验证；`requestQuota=0` 语义模糊（**allowedModels 字段保留现状不动**，按用户决策③） | `TokensPage` |
| A4 | — 不做 | 一次性 Token 展示后关闭即丢失（**不加额外保护**，按用户决策④） | `TokensPage` |
| A5 | — 不做 | 协议编辑用「全量 PUT 替换」语义，UI 未告知用户（**保留现状语义**，按用户决策⑤） | `ProvidersPage` |
| A6 | ⬜ 待做 | 连接编辑「清除覆盖」交互绕（checkbox 三态切换 null/value） | `AdminModelsPage` |
| A7 | ✅ 完成 | 无全局 toast，操作成功/失败只在顶部 alert — 已接入 `svelte-sonner`，`App.svelte` 挂载 `<Toaster>`，提供 `toast`/`toastSuccess`/`toastError` | 全部 |
| A8 | ⬜ 待做 | 启用/禁用、角色变更等无确认，与删除操作口径不一致 — 口径已定（决策⑦仅删除确认），待各页面接入 `useConfirm` | 多处 |
| A9 | ⬜ 待做 | 模型目录表格行有 `cursor-pointer` + hover，但点击无反应，误导 — 阶段 1 移除误导 cursor，阶段 2 做 Drawer | `ModelsPage` |
| A10 | ✅ 完成 | 路由无守卫：非 admin 直接访问 `#/admin/models` 会看到空白页 — `App.svelte` 加 `accessDenied` 派生守卫，非 admin 访问 `/admin/*` 渲染 `UnauthorizedPage`（403） | `App.svelte` |
| A11 | ⬜ 待做 | 表格无分页/虚拟化，大数据量会卡 | `ModelsPage` |

### 8.2 代码结构

| # | 状态 | 问题 |
|---|------|------|
| B1 | ✅ 完成 | 两个管理员页面单文件 900~1100 行，职责混杂 — ProvidersPage 拆为 4 文件（980→~230行 orchestrator），AdminModelsPage 拆为 4 文件（857→~180行 orchestrator），共享 ProtocolEditForm 参数化复用 |
| B2 | 🔨 部分完成 | 每个页面重复 header / loading / error / empty 模板，无抽象 — 已抽 `PageShell`/`SectionHeader`/`EmptyState`/`ErrorState` 4 个组件，待各页面接入替换 |
| B3 | 🔨 部分完成 | 两个管理员页面都自己实现 `deleteDialogOpen + deleteTarget + confirmDelete`，重复 — 已抽 `useConfirm` hook + `ConfirmHost`，待两处删除对话框替换接入 |
| B4 | ✅ 完成 | 状态管理分散（每页自己 `loading/error/data`），无统一数据层 — 已建 `ResourceStore`/`MapResourceStore` 基类 + 7 个资源 store，待各页面接入替换 |
| B5 | ✅ 完成 | API 调用散落组件内，无缓存/失效/重试 — 已随 B4 落地 store 层（load/invalidate/markStale/mutate） |
| B6 | ✅ 完成 | 原生 `<select>` 与 shadcn `<Select>` 混用（`ProvidersPage` 用原生，`UsersPage` 用 shadcn）— ProvidersPage 三处原生 select 已换为 shadcn `<Select type="single">`，顺手修复 UsersPage/TokensPage 的 Select 类型错误 |
| B7 | ✅ 完成 | 枚举中文展示散落各处 — 已集中至 `$lib/constants.ts`（协议/配额周期/额度适配器/模型状态 + label 函数），`formatQuotaPeriod` 迁移为 `quotaPeriodLabel` |
| B8 | ✅ 完成 | `Array(6)` / `Array(3)` / `Array(4)` skeleton 数量 magic number 散落 — 已集中至 `SKELETON_ROWS` 常量，五个页面接入 |

> **B2/B3/B4/B5 说明**：基础设施（组件/hook/store 基类）已就绪，但「接入各页面替换现有内联实现」属于阶段 1 各页面修补（A.3）与阶段 2 拆分（B.3/B.4 文件拆分）的工作，故标为「部分完成」。组件本身可在浏览器中验证可用。

### 8.3 视觉 / 设计

| # | 状态 | 问题 |
|---|------|------|
| C1 | ⬜ 待做 | 绿色 CTA (`#22C55E`) 与 shadcn primary 黑色系并存，视觉语言不统一 |
| C2 | ⬜ 待做 | 每个页面 header 单薄（h2 + 一行副标题），缺视觉层次 |
| C3 | ⬜ 待做 | 展开/折叠直接出现/消失，无过渡动画 |
| C4 | ⬜ 待做 | 空态仅「图标+一行字」，缺引导性 |
| C5 | ⬜ 待做 | badge 颜色语义不统一：启用/禁用有的 default/secondary，有的 default/destructive |
| C6 | ⬜ 待做 | 移动端/小屏未优化（表格横向滚动但无 responsive 处理） |

### 8.4 潜在 bug

| # | 状态 | 问题 |
|---|------|------|
| D1 | ⬜ 待做 | `AdminModelsPage.confirmDelete` / `handleToggleModel` 中 catch 没清 `linksLoading`，失败后 loading 卡死 |
| D2 | — 不做 | `ProvidersPage.handleToggle` 把 `apiKeys` 的 `key: ""` 上送，依赖后端容忍空 key（隐式约定，**不在本计划范围**） |
| D3 | ⬜ 待做 | `ModelsPage` 的 `$effect(() => { onlyAvailable; load(); })` 写法可工作但不明确 |
| D4 | ⬜ 待做 | `TokensPage.closeCreate` 等多数 reset 函数未清 `error` |

> **D1/D4 待做说明**：原计划首批随基础设施一起修，实际首批聚焦基础设施（A1/A7/B2/B3/B4/B5），D1/D4 顺延至阶段 1 A.3 各页面修补时处理。

---

## 9. 决策汇总

> 以下 9 项决策已于 2026-07-07 与用户确认，作为本计划权威口径。

| # | 决策项 | 结论 |
|---|--------|------|
| ① | 全局 toast 实现 | `svelte-sonner`（+1 依赖，~3KB） |
| ② | 错误文案策略 | 中文友好主文案 + 原始 message 收进 tooltip |
| ③ | `allowedModels` 字段 | **保留现状**（写死 `[]`，UI 空时隐藏） |
| ④ | 一次性 Token 保护 | **不加**额外保护 |
| ⑤ | 协议编辑语义 | **保留**全量 PUT，不补 UI 提示 |
| ⑥ | 非 admin 访问 `#/admin/*` | 显示 **403 无权限页** |
| ⑦ | 操作确认口径 | **仅删除**需确认（启用/禁用、改角色直接执行） |
| ⑧ | 模型目录表格行交互 | 改为**点击展开详情 Drawer**（分两步：阶段 1 先移除误导 cursor，阶段 2 做 Drawer） |
| ⑨ | 绿色 `#22C55E` 定位 | **收窄为 CTA 局部色**，primary 仍 slate |

**无歧义直接做的部分**（无需决策）：

- ✅ `formatApiError(e)` 统一错误格式化（含 401 自动跳登录扩展）— 已完成
- ⬜ `ProvidersPage` 原生 `<select>` → shadcn `<Select>` — 待阶段 1 A.3
- ✅ 抽 `PageShell` / `EmptyState` / `ErrorState` / `SectionHeader` 4 个展示组件 — 已完成
- ✅ 抽 `useConfirm` hook（仅用于删除）— 已完成
- ⬜ 修 bug D1（catch 漏清 loading）/ D4（reset 漏清 error）— 待阶段 1 A.3

**待确认的开放点**（阶段 1 执行前需对齐）：

- ✅ ~~后端错误响应格式（决定 `formatApiError` 解析逻辑）~~ — 已确认：`ApiError`（来自生成的 client.ts）含 `.status` / `.message` / `.body`，后端响应体格式不固定（`{error}` / `{message}` / 纯文本），client.ts 已做 fallback 解析。`formatApiError` 据此实现。

---

## 10. 实施计划

### Phase A: 快速胜利（1~2 天）

**目标**：不动结构，补齐可用性短板与一致性。

#### A.1 基础设施 ✅ 完成

1. **依赖**：`pnpm add svelte-sonner` ✅
2. **全局 toast 挂载**：在 `App.svelte` 根渲染 `<Toaster />`，配置深色模式跟随 + 中文默认 ✅
3. **错误格式化** `$lib/utils/error.ts` ✅
   ```ts
   export function formatApiError(e: unknown): { title: string; detail?: string }
   ```
   - 解析后端响应体（结构化 / 纯字符串二选一，依后端实况）
   - 401 触发跳登录（已有逻辑保留）
   - 返回 `{ title: 中文主文案, detail?: 原始 message }`，主文案给 toast/alert，detail 用于 tooltip
4. **抽展示组件** `$lib/components/common/` ✅
   - `PageShell.svelte`：包裹 `<SectionHeader />` + 内容区，统一 padding 与 flex
   - `SectionHeader.svelte`：标题 + 描述 + 计数 badge + 右侧 action slot
   - `EmptyState.svelte`：图标 + 标题 + 副标题 + 可选 action
   - `ErrorState.svelte`：错误展示 + 重试按钮
5. **抽 `useConfirm` hook** `$lib/hooks/useConfirm.svelte.ts` ✅
   - 基于 shadcn `Dialog`，store 化 prompt
   - API：`const confirm = useConfirm(); await confirm({ title, description });`
   - 仅用于删除场景（决策⑦）

> **验证**：svelte-check 新增/修改文件全部类型干净；浏览器实测 toast 弹出与 `confirm()` promise resolve 链路正常。

#### A.2 路由守卫（决策⑥）✅ 完成

1. 新建 `UnauthorizedPage.svelte`：图标 + "无访问权限" + 返回首页按钮 ✅
2. 在 `App.svelte` 路由层拦截：路由若为 `/admin/*` 且 `!auth.isAdmin` → 渲染 403 页（不改动 hash，便于后端修复权限后刷新即可恢复） ✅

> **验证**：mock `/auth/me` 返回 member 角色后访问 `/admin/models`，正确渲染 403 页（盾牌图标 + 说明 + 返回按钮）；点击返回按钮跳转 `/models`。admin 用户正常访问不受影响。

#### A.3 各页面修补

`ModelsPage`：
- 移除表格行 `cursor-pointer` + hover（决策⑧第一步）
- 表头补 `aria-sort`

`TokensPage`：
- 修 D4：`closeCreate` 等 reset 函数补 `error = ""`
- 创建校验：数字 `min=0`，`0` 明确显示"不限制"
- 操作成功 → toast 反馈

`ProvidersPage`：
- 原生 `<select>` → shadcn `<Select>`（B6）
- 删除确认改走 `useConfirm` hook（B3）
- 操作反馈 → toast

`AdminModelsPage`：
- 修 D1：`confirmDelete` / `handleToggleModel` catch 末尾补 `linksLoading.delete(id); linksLoading = new Set(...)`
- 删除确认改走 `useConfirm` hook
- 操作反馈 → toast

`UsersPage`：
- 角色变更直接执行（决策⑦），成功 → toast

#### A.4 一致性收尾

- 每个页面顶部模板 → `PageShell` + `SectionHeader`
- 空态 → `EmptyState`
- 错误态 → `ErrorState`（带重试）

---

### Phase B: 数据层与结构重构（3~5 天）

**目标**：把状态从组件中抽离，为管理员页面拆分铺路。

#### B.1 数据层

- 引入轻量数据层封装（基于 `@tanstack/svelte-store` 已有依赖，或自建）
- 每个资源建独立 store：`modelsStore`、`tokensStore`、`providersStore`、`usersStore`
- 组件只消费 store，操作后调 `store.invalidate()` 自动刷新
- 解决 B4 / B5，为乐观更新、重试打基础

#### B.2 枚举集中化 ✅ 完成

- 新建 `$lib/constants.ts`：协议、配额周期、额度适配器、模型状态的 value ↔ 中文 label 映射（B7）
- 删除散落 hardcode
- 顺带集中 `SKELETON_ROWS` 常量（B8），五个页面 skeleton 接入

#### B.3 拆分 `ProvidersPage`（1100 行 → 多文件）✅ 完成

| 文件 | 职责 |
|------|------|
| `lib/ProvidersPage.svelte` | 容器 + list + 删除对话框（~230 行） |
| `lib/providers/ProviderCreateDialog.svelte` | 创建对话框（含协议配置子表单） |
| `lib/providers/ProtocolEditForm.svelte` | 协议增改表单（复用于创建与编辑） |
| `lib/providers/ProviderRow.svelte` | 单行展开内容 |
| `lib/utils/provider.ts` | `emptyProtocol` / `buildQuotaConfigString` / `protocolViewToInput` 工具 |

删除确认留父组件（useConfirm 接入留后续 A.3）。

#### B.4 拆分 `AdminModelsPage`（900 行 → 多文件）✅ 完成

| 文件 | 职责 |
|------|------|
| `lib/AdminModelsPage.svelte` | 容器 + 删除对话框（~180 行） |
| `lib/admin-models/ModelFormDialog.svelte` | 模型增改 |
| `lib/admin-models/ModelLinkEditForm.svelte` | 连接编辑（含 parseNumOrNull + protocolsForSelectedProvider + currentModel 内化） |
| `lib/admin-models/ModelRow.svelte` | 单行展开内容（内嵌 ModelLinkEditForm） |

#### B.5 模型目录详情 Drawer（决策⑧第二步）

- 实现 `ModelsPage` 行点击 → 展开 Drawer
- 内容：模型全字段 + providers 列表 + 能力矩阵
- 与 B.3 / B.4 子组件复用展示逻辑

#### B.6 列表性能

- `ModelsPage` 加分页或虚拟列表（A11）
- 候选：`svelte-virtual-list` / 简单「每页 50 + 加载更多」

---

### Phase C: 设计系统打磨（2~3 天）

**目标**：视觉一致性、动效、响应式。

1. **绿色定位收窄**（决策⑨）：`#22C55E` 仅用于 CTA 按钮、成功状态、强调图标；primary / checkbox / focus ring 仍用 slate 系
2. **SectionHeader 视觉升级**：图标 + 计数 badge + 描述 + action slot 统一视觉层次（C2）
3. **展开/折叠动画**：`tw-animate-css`（已引入）+ bits-ui `Collapsible` 替换手写 `{#if expandedId === ...}`（C3）
4. **badge 语义统一**：建 `statusBadgeVariant(status)` 工具，启用=default / 禁用=secondary / 错误=destructive（C5）
5. **响应式**：sidebar 在小屏自动收为 icon；表格 `< md` 改为 card 列表布局（C6）
6. **a11y 清理**：移除 `svelte-ignore a11y_no_static_element_interactions`，改用真 `<button>`；排序表头补 `aria-sort`

---

## 11. 执行顺序与里程碑

```
第 1 周（Phase A — 快速胜利）
  Day 1   基础设施：svelte-sonner + formatApiError + 4 个展示组件 + useConfirm hook  ✅ 完成
  Day 2   路由守卫 + 各页面修补（bug 修复 + select 统一 + toast 接入）+ 一致性收尾  🔨 路由守卫✅、select 统一✅、枚举集中✅；各页面修补待做

第 2 周（Phase B — 结构重构）
  Day 3-4 数据层 + 枚举集中化                          🔨 数据层 store 基类已就绪，待接入
  Day 5-6 拆分 ProvidersPage                            ⬜ 待做
  Day 7   拆分 AdminModelsPage                          ⬜ 待做
  Day 8   模型目录详情 Drawer + 列表性能                ⬜ 待做

第 3 周（Phase C — 设计打磨）
  Day 9   绿色定位收窄 + SectionHeader 升级             ⬜ 待做
  Day 10  动效 + badge 语义 + 响应式 + a11y             ⬜ 待做
```

**优先级理由**：Phase A 全是低风险高收益的「补丁」，能立刻提升可用性 & 一致性，也为 Phase B 拆分时提供现成的 `PageShell`/`EmptyState`/`useConfirm` 去填充。Phase B 是结构性投资，回报在长期可维护性。Phase C 纯打磨，可按需取舍。

---

## 13. 实施进度

> 最后更新：2026-07-08

### 已完成（首批基础设施）

| 项 | 对应问题 | 交付物 |
|----|----------|--------|
| A1 | 错误格式化 | `frontend/src/lib/utils/error.ts` — `formatApiError` / `toastError` |
| A7 | 全局 toast | `frontend/src/lib/utils/toast.ts` + `App.svelte` 挂载 `<Toaster>` |
| A10 | admin 路由守卫 | `frontend/src/lib/UnauthorizedPage.svelte` + `App.svelte` 的 `accessDenied` 派生守卫 |
| B2 | 展示组件抽离 | `frontend/src/lib/components/common/` — `PageShell` / `SectionHeader` / `EmptyState` / `ErrorState` |
| B3 | useConfirm hook | `frontend/src/lib/hooks/useConfirm.svelte.ts` + `frontend/src/lib/components/common/confirm-host.svelte` |
| B4 | 数据层 store | `frontend/src/lib/stores/resource.svelte.ts` — `ResourceStore` / `MapResourceStore`（基于 `@tanstack/svelte-store`） |
| B5 | API 缓存/失效 | `frontend/src/lib/stores/resources.svelte.ts` — 7 个资源 store 实例 + `resetAllStores` |
| B6 | select 统一 | `ProvidersPage` 三处原生 `<select>` → shadcn `<Select type="single">`；顺手修 UsersPage/TokensPage Select 类型错误 |
| B7 | 枚举集中化 | `frontend/src/lib/constants.ts` — 协议/配额周期/额度适配器/模型状态 + label 函数；`formatQuotaPeriod` 迁移为 `quotaPeriodLabel` |
| B8 | skeleton 常量化 | `frontend/src/lib/constants.ts` — `SKELETON_ROWS` 常量，五个页面接入 |
| — | 401 兑底 | `frontend/src/lib/api.ts` — onError 避免在 `/auth/*` 上死循环跳转 |

### 待做（阶段 1 剩余 + 阶段 2/3）

- **A.2** 路由守卫（403 无权限页）
- **A.3** 各页面修补：ModelsPage 移除误导 cursor、TokensPage 校验+toast、ProvidersPage select 统一+useConfirm 接入、AdminModelsPage 修 D1+useConfirm 接入、UsersPage toast
- **A.4** 一致性收尾：各页面接入 `PageShell`/`SectionHeader`/`EmptyState`/`ErrorState`
- **B.1** 数据层接入各页面（store 已就绪，待替换组件内联状态）
- **B.2~B.6** 枚举集中化、ProvidersPage/AdminModelsPage 拆分、模型目录 Drawer、列表性能
- **Phase C** 设计打磨

### 存量类型错误（非本次范围，待阶段 1 各页面修补时处理）

- `TokensPage.svelte` / `ProvidersPage.svelte` — `asChild` 不在 DialogTrigger props（bits-ui API 变更）
- `TokensPage.svelte` / `UsersPage.svelte` — Select `value` 类型 `string[]` vs `string` 错位
- `TokensPage.svelte:111` — `{ active: boolean }` 不匹配 `UpdateTokenRequest`
- `components/table/Subscribe.svelte` — 2 个警告

---

## 12. 不在本计划范围

明确不做（避免范围蔓延）：

- 后端 API 形态变更（决策⑤保留全量 PUT 语义）
- `allowedModels` 多选 UI（决策③保留现状）
- 一次性 Token 下载/二次确认（决策④不加保护）
- `ProvidersPage.handleToggle` 的空 key 隐式约定（D2，需后端配合）
- 国际化（i18n）— 当前全站中文，未到临界点
- E2E 测试 — 待 Phase B 结构稳定后再考虑
