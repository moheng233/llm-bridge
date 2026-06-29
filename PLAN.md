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
