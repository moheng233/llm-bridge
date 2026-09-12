# 管理与用量 API

本文描述网关管理接口。完整类型以 [`frontend/src/bindings`](../frontend/src/bindings) 和生成的 [`client.ts`](../frontend/src/bindings/client.ts) 为准；实现与验收状态见 [STATUS](../STATUS.md)。另见 [WebSocket](ws-api.md)、[设备登录](device-login.md) 与 [目录导入](catalog-import.md)。

## 认证边界

- `/api/v1/admin/*` 使用管理员 Session，不接受 API Bearer Token 代替管理员身份。
- `/api/v1/models*`、`/api/v1/tokens*`、`/api/v1/usage/*` 使用用户 Session。
- `/v1/models`、`/v1/chat/completions`、`/v1/ws` 使用 `Authorization: Bearer lb_...`，包括免登录模式。
- 未配置 OIDC 或 Discovery 失败时，服务按既定设计进入免登录管理员模式。管理接口对可访问服务的用户开放，部署者必须提供可信网络或反向代理边界。
- OIDC 模式下管理员授权读取当前数据库角色，旧 Session 不保留降权前的管理员权限。
- API JSON 使用 camelCase；OpenAI 标准字段如 `owned_by` 保持其协议拼写。

## 登录与个人 Token

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/auth/login` | 发起 OIDC 登录，`next` 必须为安全的站内路径 |
| GET | `/auth/callback` | OIDC 回调 |
| GET | `/auth/me` | 当前用户及登录状态 |
| POST | `/auth/logout` | 销毁 Session |
| GET | `/api/v1/tokens` | 当前用户的 Token 列表，不返回哈希或完整凭据 |
| POST | `/api/v1/tokens` | 创建 Token，仅此响应返回明文 |
| PATCH | `/api/v1/tokens/{id}` | 更新当前用户的 Token |
| DELETE | `/api/v1/tokens/{id}` | 删除当前用户的 Token |

创建 Token 请求：

```json
{
  "name": "local-development",
  "allowedModels": ["openai/gpt-4o"],
  "requestQuota": 100,
  "tokenQuota": 100000,
  "quotaPeriod": "monthly"
}
```

`allowedModels=[]` 表示全模型；额度 `0` 表示不限制；周期为 `daily`、`monthly` 或 `unlimited`。这些参数由用户本人管理，**不构成管理员强制预算**。设备登录的 `vscode:` Token 固定使用全模型、unlimited 默认值，不因这份文档改变。

Token 准入预留计入当期额度，真实上游 usage 到达后对原预留记录多退少补。预估不是上游的硬限流器：实际用量可以超过预估，账本不能为了维持限额而截断事实用量。

## 模型浏览

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/v1/models` | 模型目录，包括尚无可用提供者的模型 |
| GET | `/api/v1/models/available` | 已有启用提供者的模型 |

管理页面使用上述 Session 接口；插件使用带 Bearer Token 的 `/v1/models` 或 WS `listModels`。不要把两个列表的认证与返回结构混用。

## Provider 与协议

| 方法 | 路径 | 请求 / 响应类型 |
| --- | --- | --- |
| GET / POST | `/api/v1/admin/providers` | `CreateProviderRequest` / `ProviderResponse` |
| GET / PUT / DELETE | `/api/v1/admin/providers/{id}` | `UpdateProviderRequest` / `ProviderResponse` |
| GET / PUT | `/api/v1/admin/providers/{id}/protocols` | `ProtocolInput[]` / `ProtocolView[]` |
| GET | `/api/v1/admin/providers/{id}/quota` | `ProviderQuotaResponse` |
| GET / POST | `/api/v1/admin/providers/{id}/models` | `AddModelRequest` / `ProviderModelResponse` |
| PUT / DELETE | `/api/v1/admin/providers/{id}/models/{model_id}` | `UpdateModelRequest` / `ProviderModelResponse` |

`providerId` 是稳定的业务标识，例如 `openai`；路径里的 `{id}` 是数据库数值主键。`protocolId` 引用 ProviderProtocol 主键，不能传协议名称。

创建 Provider 的示例请求体：

```json
{
  "providerId": "openai",
  "displayName": "OpenAI",
  "apiKeys": [{"label": "primary", "key": "REPLACE_ME", "weight": 1}],
  "protocols": [{
    "protocol": "openAiChatCompletions",
    "baseUrl": "https://api.openai.com/v1",
    "enabled": true,
    "priority": 100
  }],
  "enabled": true,
  "priority": 100
}
```

协议枚举是 `openAiChatCompletions`、`openAiResponses`、`anthropicMessages`。`compatSettings` 是序列化 JSON 字符串，其字段见 `CompatibilitySettings`：`pathSuffix`、`customHeaders`、`customParams`。

**保存语义：** 创建和更新均将提供者字段与全部协议放在同一事务。重复创建同一 `providerId` 返回 409 `provider_id_exists`，不覆盖原配置。`PUT providers/{id}` 是全量更新，不是 PATCH；缺省/空 `apiKeys`、`protocols` 可能清空对应配置。更新协议需保留已有 id；已有同 label 的 Key 留空表示保留原值，不应把展示掩码当作新 Key 回写。目录只提供预填，不另行更新本地配置。

## 规范模型与关联

| 方法 | 路径 | 请求 / 响应类型 |
| --- | --- | --- |
| GET / POST | `/api/v1/admin/models` | `ModelInput` / `AdminModelResponse` |
| GET / PUT / DELETE | `/api/v1/admin/models/{id}` | `ModelInput` / `AdminModelResponse` |
| GET / POST | `/api/v1/admin/models/{id}/providers` | `AddModelProviderRequest` / `ModelLinkView` |
| PUT / DELETE | `/api/v1/admin/models/{id}/providers/{link_id}` | `UpdateModelProviderRequest` / `ModelLinkView` |
| POST | `/api/v1/admin/models/{id}/providers/{link_id}/test` | `TestModelProviderRequest` / `TestModelProviderResponse` |

规范 `modelName` 是全局唯一名称，例如 `openai/gpt-4o`；`providerModelId` 是上游接受的原始模型 ID，例如 `gpt-4o`。关联层的可空能力字段覆盖模型标称能力，`null` 表示使用标称值。定价单位为 USD / 1M tokens；缺价与零价不同。

创建已有同名模型返回 409 `model_name_exists`；修改已创建模型的 `modelName` 返回 400 `model_name_is_immutable`，其他标称字段仍可显式更新。

## 用户管理

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/v1/admin/users` | 用户列表 |
| PATCH | `/api/v1/admin/users/{id}/role` | 请求 `{"role":"admin"}` 或 `{"role":"member"}` |

不能用客户端缓存的角色决定授权；服务端以当前数据库角色为准。

## 用量与内容追踪

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/v1/usage/summary` | 当前区间与前一区间聚合；字段见 `SummaryQuery` / `UsageSummaryResponse` |
| GET | `/api/v1/usage/traces` | 时间、模型、Token 筛选及分页；字段见 `TracesQuery` / `TraceListResponse` |
| GET | `/api/v1/usage/traces/{id}` | 单次请求详情；字段见 `TraceDetail` |

成员只能查看自己的统计、列表和详情；管理员可查看全局。快照仅在 `LLM_BRIDGE_OBS_CAPTURE_CONTENT=true` 时采集，可能包含请求中的敏感内容。trace 按保留策略删除，日聚合不保存消息内容并长期保留。

## 只读目录与批量连接

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/v1/admin/models-import/preview` | 条件读取源目录、校验三层引用、返回修订和本地匹配信息；不写入 |
| POST | `/api/v1/admin/model-connections` | `CreateModelConnectionsRequest` → `CreateModelConnectionsResponse`；本批新模型与连接原子保存 |

批量请求为 `{items:[{model:{kind:"existing",id},link:AddModelProviderRequest}]}`；新模型引用使用 `{kind:"new",model:ModelInput}`。响应 items 与输入顺序一致，每项包含 `modelId`、`link`、`modelCreated`、`linkCreated`。同批同名新模型必须一致；数据库已存在同名模型时返回 409，要求显式复用。已存在的连接按 `(modelId, protocolId, providerModelId)` 原样复用，不覆盖已有字段。

空批返回 400 `empty_selection`；字段校验错误携带 `itemIndex` 和 `field`。协议必须属于目标提供者，existing 模型必须存在。任何数据库异常整批回滚，不残留新模型或部分连接；此前单独保存的提供者保留。网络中断不证明回滚，应重载本地记录后显式确认重试。

目录仅首次预填，保存后独立维护。旧目录写入接口已移除；客户端选择目录项后通过本地 CRUD / 批量接口保存，不能通过重新读取目录覆盖本地数据。详见 [目录与接入](catalog-import.md)。
