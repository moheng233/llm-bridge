# models.dev 三层目录导入

数据流：models.dev TOML → `scripts/models-dev-catalog` → Pages `catalog.json` / `contract.json` → 管理员手动预览与选择 → 本地数据库。业务路由只读取本地数据库；不自动同步、不要求目录服务在聊天时在线。

## 来源与契约

`LLM_BRIDGE_MODELS_IMPORT_URL` 覆盖目录 URL，默认 `https://moheng233.github.io/llm-bridge/catalog.json`。同目录读取 `contract.json`；两者 schemaVersion 必须为 1。目录必须包含有效 generatedAt、非空 sourceRev、models/providers/links；拒绝重复业务键、悬空引用、无效 HTTP(S) URL、负数/非有限价格及无效 token 上限。

请求超时 30 秒，manifest 最大 64 KiB、目录最大 16 MiB，读取过程中即执行上限。catalog 缓存携带 ETag/Last-Modified 条件请求，304 复用已验证内容；远端错误不会作为空目录继续导入。

生成器保留模型标称能力，Provider limit/modalities/能力覆盖只进入关联层；base_model_omit 按源规则处理。生成器没有把未映射的 OpenRouter 扩展能力伪装为支持。工作流支持每日和手动触发，不宣称已有跨仓库 push webhook。

## 管理接口

均要求 Admin Session（免登录模式自动注入默认 Admin）。

- `GET /api/v1/admin/models-import/preview`：返回 sourceRev、generatedAt，及 models/providers/links 三层。每项携带 `exists`；link 另有不透明 `key`，客户端应原样回传。
- `POST /api/v1/admin/models-import`：

  ```json
  {"models":["fixture/model"],"providers":["fixture"],"links":["从预览原样取得的key"]}
  ```

  成功返回 `{created,updated,skipped,errors}`。计数包含 ProviderProtocol，因此选择 1 模型+1 Provider+1 关联首次可产生 4 条新建，重导入可产生 4 条更新/复用。updated 表示按业务键复用/更新，不保证每列发生变化。

选择关联会由服务端补齐其模型/Provider 依赖；仅选择父项的 API 请求不会自动选取全部关联。UI 选择父项时主动带上相关关联；清除父项会取消相关关联，避免隐式重新选回。未知 key/父项拒绝整次导入。目录网络或契约加载失败为 502；选择/事务错误为 400，预览数据库错误为 500。

## 幂等与保护

导入在事务与数据库固定锁行保护下按业务键 upsert：modelName、providerId、Provider 内 protocol+baseUrl，以及 model+protocol+providerModelId。失败整次回滚，不吞掉部分写入错误。相同目录重导入不新增重复行。

已有 Provider 的 API Keys、enabled、priority、quota adapter 设置保留；已有 Protocol 的手动设置和关联 priority 保留。目录维护模型标称数据及关联能力/价格；关联 enabled 可随目录更新。导入不会凭空产生可用 API Key，首次导入后需配置凭据。

## UI

模型管理页和 Provider 页共用 `CatalogImportDialog.vue`。支持模型/Provider/关联三层、搜索、新建/更新筛选、每页 50 项、选择联动、导入中禁用交互及事务结果。正文在较矮视口中滚动，关闭和导入按钮保持可达。不是服务器推送的逐行进度：进度表示整个事务进行中/已完成。

专用回归：`tests/catalog_contract.rs` 和生成器测试；实际公开 Pages 源与本地夹具验收应分别记录，不能用旧 Pages 快照证明新生成器已经发布。
