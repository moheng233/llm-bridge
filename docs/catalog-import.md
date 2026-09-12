# models.dev 只读目录与本地接入

数据流：models.dev TOML → `scripts/models-dev-catalog` → Pages `catalog.json` / `contract.json` → 管理员预览与选择 → 可编辑本地草稿 → 显式保存。目录仅首次预填，保存后独立维护，不同步、不覆盖。业务路由只读取本地数据库，不要求目录服务在聊天时在线。

## 来源与契约

`LLM_BRIDGE_MODELS_IMPORT_URL` 覆盖目录 URL，默认 `https://moheng233.github.io/llm-bridge/catalog.json`。同目录读取 `contract.json`；两者 schemaVersion 必须为 1。目录必须包含有效 generatedAt、非空 sourceRev、models/providers/links；拒绝重复业务键、悬空引用、无效 HTTP(S) URL、负数/非有限价格及无效 Token 上限。源文件可下载不等于通过契约校验；失败时显示实际错误，仍可手动配置。

请求超时 30 秒，manifest 最大 64 KiB、目录最大 16 MiB，读取过程中即执行上限。catalog 缓存携带 ETag/Last-Modified 条件请求，304 复用已验证内容；远端错误不会作为空目录继续。

生成器保留模型标称能力，Provider limit/modalities/能力覆盖只进入关联层；base_model_omit 按源规则处理。未映射的源字段不伪装为支持。工作流支持每日和手动触发；外部 schemaVersion 保持 1。

目录限定为文本输出聊天模型；明确非文本输出会在生成时告警排除，不把图像/音频模型的零 Token 限额改成伪造的正值。图像输入、文本输出仍保留视觉能力；未声明输出模态按旧源兼容处理。需要 `${…}` 部署变量的提供者端点不能直接使用，会告警排除其连接，仍可手动配置真实端点。

生成器在写盘前校验与消费端一致的 Token 范围、价格、URL、键及引用约束；工作流先跑回归，校验失败不发布。预览失败返回 HTTP 502，并在接入页和模型创建页显示具体原因，例如 `openai/gpt-image-2.maxOutputTokens: expected integer 1..4294967295, got 0`；重试和手动配置仍可用。修改本地生成器后还需发布新的 Pages 产物，刷新才能替换旧目录。

## 只读预览

`GET /api/v1/admin/models-import/preview` 要求 Admin Session，返回 sourceRev、generatedAt 及 models/providers/links 三层。每项带 `exists`；连接的 `key` 是不透明选择标识，不应拆解或重造。目录中的本地匹配标记不代表另一个独立提供者实例也已有相同连接；工作页按目标实例的模型、协议与上游 ID 判定已添加状态。

旧目录 POST 写入接口已移除。预览不创建提供者、模型或连接，不修改已有本地字段。

## 两个保存检查点

`/admin/setup` 提供“选择提供者 / 连接设置 / 添加模型 / 完成”四步工作页。可从目录、已有提供者或手动配置开始；Query 只保存已持久化记录 ID 和可选参考目录 ID，不保存 Key 或完整草稿。

1. 保存提供者：提供者及全部协议同一事务提交。创建遇到重复实例 ID 返回 409 `provider_id_exists`，不会覆盖旧实例。可显式使用已有实例，或换新 ID 创建独立实例。编辑保留协议 id；已有同 label 的 Key 留空表示保留原值，不回传展示掩码。成功后立即清除明文 Key 草稿。
2. 保存模型连接：`POST /api/v1/admin/model-connections` 接收 `CreateModelConnectionsRequest`，单个事务保存本批新模型与连接。失败整批回滚，但第一步已保存的提供者保留。响应按输入顺序返回模型 ID、连接及 `modelCreated` / `linkCreated`，不返回数据库行计数。

模型引用显式区分 `{kind:"existing",id}` 与 `{kind:"new",model:ModelInput}`。同批同名新模型定义必须一致；数据库已有同名模型时返回 409 `model_name_exists`，由用户选择复用本地定义或修改新 ID。复用不覆盖共享标称。模型 ID 创建后不可修改，其他标称字段可显式维护。

连接业务键为 `(modelId, protocolId, providerModelId)`；已存在时原样复用，不覆盖价格、能力或启用状态。alpha/beta 等别名分别保存，共享一份标称定义；连接覆盖 `null` 表示继承，价格为空表示未知，0 才表示免费。协议必须属于当前提供者。空批、无效字段与冲突草稿被拒绝，字段错误带 `itemIndex` / `field`。

响应中断不证明事务回滚：工作页显示“保存结果待确认”，禁止自动重发，允许查看或重载本地记录，再由用户确认重试。继续添加模型也先重载本地记录，不把刚提交的数据再次当成新建。

## 长期维护与客户端使用

提供者详情 `/providers/:id` 与模型详情 `/admin/models/:id` 复用连接编辑器。目录刷新不覆盖本地标称、Key、端点或连接覆盖。手动检测发送真实上游请求，可能产生费用；记录仅保存在当前浏览会话，跨页面可见，配置修改、刷新、退出或切换用户后失效，不代表实时健康。

客户端使用网关模型 ID、网关 Base URL 和自己的访问令牌，不使用上游 API Key。Base URL 根据当前访问入口展示；客户端使用其他域名时应替换。

回归：`tests/catalog_contract.rs` 覆盖条件读取；`tests/provider_setup_contract.rs` 覆盖权限、冲突、标称/覆盖分层、复用无覆盖及事务回滚。真实公开源与本地夹具验收分别记录，不用本地成功冒充公开目录可用。
