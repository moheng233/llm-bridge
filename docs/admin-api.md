# LLM-Bridge Admin REST API

所有接口挂载在 HTTP 服务器的同一个端口（默认 `3000`）上，与 WebSocket 端点 `/ws` 共存。

## 认证

若服务器通过 `LLM_BRIDGE_AUTH_TOKEN` 环境变量配置了认证令牌，则所有 Admin 接口均需在请求头中携带：

```
Authorization: Bearer <token>
```

未配置令牌时，接口对所有调用方开放。

---

## 模型接口

### 浏览目录模型（全量）

```
GET /api/v1/models
```

返回从 OpenRouter 同步到本地快照的全部模型。

**响应 200**

```json
[
  {
    "model_name": "openai/gpt-4o",
    "capabilities": {
      "name": "openai/gpt-4o",
      "maxInputTokens": 128000,
      "maxOutputTokens": 16384,
      "toolCalling": true,
      "vision": true
    }
  }
]
```

---

### 浏览可用模型（已绑定）

```
GET /api/v1/models/available
```

仅返回至少绑定了一个提供者的模型（即当前可路由的模型）。

**响应 200**

```json
[
  {
    "model_name": "openai/gpt-4o",
    "capabilities": { ... }
  }
]
```

---

## 提供者接口

### 列出所有提供者

```
GET /api/v1/providers
```

**响应 200**

```json
[
  {
    "providerName": "my-openai",
    "providerType": "openai",
    "baseUrl": null
  }
]
```

---

### 创建提供者

```
POST /api/v1/providers
Content-Type: application/json
```

**请求体**

| 字段              | 类型                              | 必填 | 说明                                 |
|-----------------|----------------------------------|------|--------------------------------------|
| `providerName`  | string                           | ✅   | 唯一标识，创建后不可更改              |
| `providerType`  | `"openai"` \| `"anthropic"` \| `"gemini"` | ✅   | 提供者协议类型                        |
| `baseUrl`       | string \| null                   | ❌   | 覆盖默认 API 地址（如私有部署）       |
| `apiKey`        | string                           | ✅   | API 密钥                             |

**响应 201** — 返回刚创建的提供者对象

**响应 409** — 提供者名称已存在

---

### 查询提供者

```
GET /api/v1/providers/:provider_name
```

**响应 200** — 返回指定提供者

**响应 404** — 提供者不存在

---

### 更新提供者

```
PUT /api/v1/providers/:provider_name
Content-Type: application/json
```

**请求体**（与创建相同，但不包含 `providerName`）

| 字段              | 类型                              | 必填 |
|-----------------|----------------------------------|------|
| `providerType`  | string                           | ✅   |
| `baseUrl`       | string \| null                   | ❌   |
| `apiKey`        | string                           | ✅   |

**响应 200** — 返回更新后的提供者对象

**响应 404** — 提供者不存在

---

### 删除提供者

```
DELETE /api/v1/providers/:provider_name
```

删除提供者的同时会级联删除该提供者下的所有模型绑定。

**响应 204** — 删除成功

**响应 404** — 提供者不存在

---

### 更新提供者密钥

```
PUT /api/v1/providers/:provider_name/secret
Content-Type: application/json
```

**请求体**

| 字段      | 类型   | 必填 | 说明     |
|---------|--------|------|--------|
| `apiKey`| string | ✅   | API 密钥 |

**响应 204** — 更新成功

**响应 404** — 提供者不存在

---

## 提供者-模型绑定接口

绑定描述"该路由别名通过哪个提供者、用哪个提供者侧模型名来处理请求"。

### 列出提供者的模型绑定

```
GET /api/v1/providers/:provider_name/models
```

**响应 200**

```json
[
  {
    "modelName": "openai/gpt-4o",
    "providerName": "my-openai",
    "providerModelName": "gpt-4o",
    "priority": 0
  }
]
```

**响应 404** — 提供者不存在

---

### 创建模型绑定

```
POST /api/v1/providers/:provider_name/models
Content-Type: application/json
```

**请求体**

| 字段                 | 类型    | 必填 | 说明                                                 |
|--------------------|--------|------|------------------------------------------------------|
| `modelName`         | string | ✅   | 目录中的标准模型名（`GET /api/v1/models` 中的 `model_name`） |
| `providerModelName` | string | ✅   | 发送给该提供者 API 时使用的模型名                      |
| `priority`          | u32    | ✅   | 路由优先级，数字越小越优先；相同模型有多个绑定时按此排序  |

**响应 201** — 返回创建的绑定对象

**响应 404** — 提供者不存在

**响应 422** — `modelName` 在目录中不存在

---

### 删除模型绑定

```
DELETE /api/v1/providers/:provider_name/models/:model_name
```

`:model_name` 中的 `/` 需 URL 编码为 `%2F`，例如：

```
DELETE /api/v1/providers/my-openai/models/openai%2Fgpt-4o
```

**响应 204** — 删除成功

---

## 错误响应格式

所有错误均返回 JSON：

```json
{
  "error": "错误描述"
}
```

| 状态码 | 含义                   |
|------|------------------------|
| 401  | 缺少或无效的 Bearer 令牌 |
| 404  | 资源不存在              |
| 409  | 资源已存在（唯一性冲突） |
| 422  | 引用的模型不在目录中     |
| 500  | 数据库内部错误           |
