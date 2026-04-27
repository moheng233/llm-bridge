# LLM-Bridge 架构文档

## 1. 项目概述

### 1.1 项目简介

LLM-Bridge 是一个高性能的 LLM（大语言模型）网关服务，旨在统一管理和路由多个 LLM 提供者的请求。它提供了一个中间层，使得客户端可以通过统一的接口访问不同的 LLM 提供者（如 OpenAI、Anthropic、Gemini），并支持模型目录管理、提供者配置、路由策略等高级功能。

### 1.2 核心特性

- **多提供者支持**：支持 OpenAI、Anthropic、Gemini 等多个 LLM 提供者
- **模型目录管理**：自动从 OpenRouter 同步模型目录，提供模型能力信息
- **灵活的路由策略**：支持基于优先级的模型路由和回退机制
- **WebSocket 实时通信**：支持流式响应，提供实时交互体验
- **RESTful 管理 API**：完整的提供者和模型绑定管理接口
- **Actor 模型架构**：基于 ractor 的高并发 Actor 模型设计
- **可观测性**：集成 OpenTelemetry，支持日志、指标和链路追踪
- **类型安全**：自动生成 TypeScript 类型定义，前后端类型一致

### 1.3 技术栈

**后端（Rust）**：
- axum：Web 框架
- tokio：异步运行时
- ractor：Actor 模型框架
- fjall：键值存储数据库
- reqwest：HTTP 客户端
- opentelemetry：可观测性
- serde/serde_json：序列化
- ts-rs：TypeScript 类型生成

**前端（TypeScript/Svelte）**：
- Svelte 5：前端框架
- Vite：构建工具
- TailwindCSS：CSS 框架
- bits-ui：UI 组件库
- svelte-spa-router：路由
- @tanstack/svelte-store：状态管理

## 2. 系统架构

### 2.1 整体架构

```
┌─────────────────────────────────────────────────────────────────┐
│                          客户端层                                │
│  ┌──────────────────┐  ┌──────────────────┐  ┌────────────────┐│
│  │  VS Code 扩展    │  │  Web 管理界面    │  │  其他客户端     ││
│  └──────────────────┘  └──────────────────┘  └────────────────┘│
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ WebSocket / REST API
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                        LLM-Bridge 网关                           │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │                    HTTP Server (axum)                     │  │
│  │  ┌────────────────┐  ┌────────────────────────────────┐  │  │
│  │  │  WebSocket API │  │      REST Admin API            │  │  │
│  │  │  /ws           │  │  /api/v1/models                │  │  │
│  │  │                │  │  /api/v1/providers             │  │  │
│  │  │                │  │  /api/v1/providers/{id}/models  │  │  │
│  │  └────────────────┘  └────────────────────────────────┘  │  │
│  └──────────────────────────────────────────────────────────┘  │
│                              │                                  │
│                              ▼                                  │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │                   Actor 层 (ractor)                      │  │
│  │  ┌──────────────────────────────────────────────────┐   │  │
│  │  │          GatewayManagerActor                      │   │  │
│  │  │  - 模型目录管理                                    │   │  │
│  │  │  - 路由解析                                        │   │  │
│  │  │  - 定期刷新目录                                    │   │  │
│  │  └──────────────────────────────────────────────────┘   │  │
│  │                         │                               │  │
│  │         ┌───────────────┼───────────────┐               │  │
│  │         ▼               ▼               ▼               │  │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐       │  │
│  │  │Connection  │  │Connection  │  │Connection  │       │  │
│  │  │Actor 1     │  │Actor 2     │  │Actor N     │       │  │
│  │  └────────────┘  └────────────┘  └────────────┘       │  │
│  │         │               │               │               │  │
│  │         └───────────────┼───────────────┘               │  │
│  │                         ▼                               │  │
│  │                  ┌────────────┐                         │  │
│  │                  │ Provider   │                         │  │
│  │                  │ Actor      │                         │  │
│  │                  └────────────┘                         │  │
│  └──────────────────────────────────────────────────────────┘  │
│                              │                                  │
│                              ▼                                  │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │                  数据持久层 (fjall)                       │  │
│  │  ┌──────────────┐  ┌──────────────┐  ┌────────────────┐  │  │
│  │  │catalog_models│  │  providers   │  │provider_models │  │  │
│  │  └──────────────┘  └──────────────┘  └────────────────┘  │  │
│  │  ┌──────────────┐  ┌──────────────┐                      │  │
│  │  │provider_     │  │  metadata    │                      │  │
│  │  │secrets      │  │              │                      │  │
│  │  └──────────────┘  └──────────────┘                      │  │
│  └──────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ HTTP/HTTPS
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      外部 LLM 提供者                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │   OpenAI     │  │  Anthropic   │  │   Gemini     │          │
│  │   API        │  │     API      │  │    API       │          │
│  └──────────────┘  └──────────────┘  └──────────────┘          │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │              OpenRouter 模型目录服务                      │  │
│  └──────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 架构分层

系统采用分层架构设计，从上到下分为：

1. **客户端层**：VS Code 扩展、Web 管理界面、其他客户端
2. **HTTP 服务层**：处理 WebSocket 连接和 REST API 请求
3. **Actor 层**：核心业务逻辑，使用 Actor 模型实现高并发
4. **数据持久层**：基于 fjall 的键值存储，持久化配置和模型目录

## 3. 核心组件

### 3.1 HTTP Server (axum)

**职责**：
- 监听 HTTP 请求
- 处理 WebSocket 连接升级
- 路由请求到对应的处理器
- 管理应用状态

**关键代码**：`src/server/ws.rs`, `src/server/admin.rs`

**主要功能**：
- WebSocket 端点：`/ws`
- REST API 端点：`/api/v1/*`
- 认证中间件：Bearer Token 认证

### 3.2 GatewayManagerActor

**职责**：
- 管理模型目录
- 解析模型路由
- 定期刷新 OpenRouter 目录
- 协调所有 ConnectionActor

**关键代码**：`src/actors/gateway_manager.rs`

**主要功能**：
- `GetAvailableModels`：获取所有可用模型
- `ResolveModel`：解析模型到具体的提供者路由
- `RefreshCatalog`：刷新模型目录

**生命周期**：
1. 应用启动时创建
2. 初始化时从 OpenRouter 拉取模型目录
3. 定期刷新模型目录（可配置间隔）
4. 应用关闭时停止

### 3.3 ConnectionActor

**职责**：
- 管理单个 WebSocket 连接
- 处理客户端认证
- 将客户端请求路由到 ProviderActor
- 转发 Provider 响应给客户端

**关键代码**：`src/actors/connection.rs`

**主要功能**：
- `IncomingWSMessage`：处理来自 WebSocket 的消息
- `ProviderChunk`：转发提供者的响应块
- `ProviderError`：处理提供者错误

**生命周期**：
1. WebSocket 连接建立时创建
2. 认证成功后开始处理请求
3. WebSocket 连接关闭时停止

### 3.4 ProviderActor

**职责**：
- 与具体的 LLM 提供者通信
- 将请求转换为提供者特定的格式
- 流式处理提供者响应
- 处理提供者错误

**关键代码**：`src/actors/provider/mod.rs`, `src/actors/provider/adapters/`

**主要功能**：
- `ChatRequest`：处理聊天请求，返回流式响应

**支持的提供者**：
- OpenAI：支持 GPT 系列、o 系列模型
- Anthropic：支持 Claude 系列模型
- Gemini：待实现

**生命周期**：
1. 每次聊天请求时创建
2. 处理完请求后停止

### 3.5 DatabaseRepo

**职责**：
- 封装所有数据库操作
- 提供模型目录的 CRUD 操作
- 提供提供者配置的 CRUD 操作
- 提供模型绑定的 CRUD 操作
- 管理提供者密钥（加密存储）

**关键代码**：`src/db/mod.rs`

**数据结构**：
- `CatalogModelRecord`：模型目录记录
- `ProviderRecord`：提供者配置记录
- `ProviderModelRecord`：提供者-模型绑定记录
- `ResolvedProviderRoute`：解析后的路由信息

## 4. Actor 模型设计

### 4.1 Actor 层次结构

```
GatewayManagerActor (单例)
    │
    ├── ConnectionActor 1 (每个 WebSocket 连接一个)
    │       │
    │       └── ProviderActor (每次请求创建)
    │
    ├── ConnectionActor 2
    │       │
    │       └── ProviderActor
    │
    └── ConnectionActor N
            │
            └── ProviderActor
```

### 4.2 Actor 通信

**消息类型**：

1. **GatewayManagerMessage**：
   ```rust
   pub enum GatewayManagerMessage {
       GetAvailableModels(ractor::RpcReplyPort<Result<Vec<AvailableModel>, String>>),
       ResolveModel(String, ractor::RpcReplyPort<Result<ResolvedProviderRoute, String>>),
       RefreshCatalog,
   }
   ```

2. **ConnectionMessage**：
   ```rust
   pub enum ConnectionMessage {
       IncomingWSMessage(GatewayMessage),
       ProviderChunk(LMResponsePart),
       ProviderError(String),
   }
   ```

3. **ProviderMessage**：
   ```rust
   pub enum ProviderMessage {
       ChatRequest(ProviderChatRequest, ractor::RpcReplyPort<Result<ProviderStream, String>>),
   }
   ```

### 4.3 Actor 生命周期管理

**GatewayManagerActor**：
- 启动：应用启动时创建，从数据库加载配置，启动目录刷新循环
- 运行：处理来自 ConnectionActor 的请求
- 停止：应用关闭时停止

**ConnectionActor**：
- 启动：WebSocket 连接建立时创建
- 运行：认证 -> 处理请求 -> 转发响应
- 停止：WebSocket 连接关闭时停止

**ProviderActor**：
- 启动：每次聊天请求时创建
- 运行：转换请求格式 -> 调用提供者 API -> 流式返回响应
- 停止：请求完成或出错时停止

## 5. 数据流

### 5.1 客户端连接流程

```
客户端                   HTTP Server           ConnectionActor      GatewayManagerActor
  │                          │                       │                      │
  ├──WebSocket 连接────────>│                       │                      │
  │                          ├──创建 Actor────────>│                      │
  │                          │                       │                      │
  ├──Connect 消息───────────>│──转发消息──────────>│                      │
  │                          │                       ├──GetAvailableModels─>│
  │                          │                       │<──模型列表──────────│
  │<──Connected 事件─────────────────────────────│                      │
  │                          │                       │                      │
```

### 5.2 聊天请求流程

```
客户端         ConnectionActor      GatewayManagerActor     ProviderActor      LLM Provider
  │                  │                      │                     │                  │
  ├──Chat 请求──────>│                      │                     │                  │
  │                  ├──ResolveModel────────>│                     │                  │
  │                  │<──路由信息────────────│                     │                  │
  │                  │                      │                     │                  │
  │                  │                      │    创建 ProviderActor │                  │
  │                  │                      │<────────────────────│                  │
  │                  │                      │                     │                  │
  │                  │                      │    ChatRequest─────>│                  │
  │                  │                      │                     ├──API 请求────────>│
  │                  │                      │                     │<──流式响应───────│
  │<──ChatResponseChunk───<───────────────────────────────────│                  │
  │<──ChatResponseChunk───<───────────────────────────────────│                  │
  │<──ChatResponseChunk───<───────────────────────────────────│                  │
  │                  │                      │                     │                  │
```

### 5.3 模型目录同步流程

```
GatewayManagerActor            OpenRouter API           DatabaseRepo
       │                              │                       │
       ├──定期触发 RefreshCatalog─────>│                       │
       │                              ├──获取模型列表────────>│
       │                              │<──模型数据────────────│
       │<──模型列表───────────────────│                       │
       │                                                      │
       ├──replace_catalog──────────────────────────────────>│
       │                                                      ├──清空旧数据
       │                                                      ├──写入新数据
       │                                                      ├──持久化
       │<──成功─────────────────────────────────────────────│
```

## 6. API 设计

### 6.1 WebSocket API

**端点**：`/ws`

**消息格式**：GatewayEnvelope

```typescript
interface GatewayEnvelope {
  requestId?: string;
  timestamp: number;
  message: GatewayMessage;
}
```

**消息类型**：

1. **Connect**：客户端连接认证
   ```typescript
   {
     type: "connect",
     payload: {
       authToken?: string
     }
   }
   ```

2. **Connected**：连接成功响应
   ```typescript
   {
     type: "connected",
     payload: {
       gatewayId: string,
       availableModels: AvailableModelInfo[]
     }
   }
   ```

3. **Chat**：聊天请求
   ```typescript
   {
     type: "chat",
     payload: {
       canonicalModelName: string,
       messages: LanguageModelChatMessage[]
     }
   }
   ```

4. **ChatResponseChunk**：聊天响应块
   ```typescript
   {
     type: "chatResponseChunk",
     payload: {
       chunk: LMResponsePart
     }
   }
   ```

5. **Error**：错误消息
   ```typescript
   {
     type: "error",
     payload: {
       code: string,
       message: string
     }
   }
   ```

### 6.2 REST Admin API

详见 `docs/admin-api.md`

**主要端点**：

- `GET /api/v1/models` - 列出所有模型
- `GET /api/v1/models/available` - 列出可用模型
- `GET /api/v1/providers` - 列出提供者
- `POST /api/v1/providers` - 创建提供者
- `PUT /api/v1/providers/:name` - 更新提供者
- `DELETE /api/v1/providers/:name` - 删除提供者
- `GET /api/v1/providers/:name/models` - 列出模型绑定
- `POST /api/v1/providers/:name/models` - 创建模型绑定
- `DELETE /api/v1/providers/:name/models/:model` - 删除模型绑定

**认证**：
- 通过 `Authorization: Bearer <token>` 头
- Token 通过环境变量 `LLM_BRIDGE_AUTH_TOKEN` 配置

## 7. 数据存储

### 7.1 数据库设计

使用 fjall 键值存储，包含以下 keyspace：

1. **catalog_models**：模型目录
   - Key: model_name (String)
   - Value: CatalogModelRecord (bincode 编码)

2. **providers**：提供者配置
   - Key: provider_name (String)
   - Value: ProviderRecord (bincode 编码)

3. **provider_models**：提供者-模型绑定
   - Key: "{model_name}_{provider_name}" (String)
   - Value: ProviderModelRecord (bincode 编码)

4. **provider_secrets**：提供者密钥
   - Key: provider_name (String)
   - Value: api_key (String)

5. **metadata**：元数据
   - Key: "schema_version"
   - Value: SchemaVersionRecord (bincode 编码)
   - Key: "catalog_refresh"
   - Value: CatalogRefreshRecord (bincode 编码)

### 7.2 数据模型

**CatalogModelRecord**：
```rust
struct CatalogModelRecord {
    model_name: String,
    capabilities: LMModelInfo,
    fetched_at: i64,
}
```

**ProviderRecord**：
```rust
struct ProviderRecord {
    provider_name: String,
    provider_type: ProviderType,  // OpenAI, Anthropic, Gemini
    base_url: Option<String>,
}
```

**ProviderModelRecord**：
```rust
struct ProviderModelRecord {
    model_name: String,
    provider_name: String,
    provider_model_name: String,
    priority: u32,
}
```

**LMModelInfo**：
```rust
struct LMModelInfo {
    name: String,
    max_input_tokens: u32,
    max_output_tokens: u32,
    tool_calling: bool,
    vision: bool,
    thinking: Option<bool>,
    adaptive_thinking: Option<bool>,
    edit_tools: EndpointEditToolName,
}
```

### 7.3 数据一致性

- **原子性**：使用 fjall 的事务支持
- **持久性**：每次写入后调用 `persist(PersistMode::SyncAll)`
- **引用完整性**：
  - 删除提供者时级联删除相关的模型绑定和密钥
  - 创建模型绑定时检查模型和提供者是否存在
  - 目录刷新时清理无效的模型绑定

## 8. 配置管理

### 8.1 环境变量

**服务器配置**：
- `LLM_BRIDGE_GATEWAY_ID`：网关 ID（默认：llm-bridge-v1）
- `LLM_BRIDGE_HOST`：监听地址（默认：127.0.0.1）
- `LLM_BRIDGE_PORT`：监听端口（默认：3000）
- `LLM_BRIDGE_AUTH_TOKEN`：认证令牌（可选）
- `LLM_BRIDGE_DB_PATH`：数据库路径（默认：./data/llm-bridge）

**模型目录配置**：
- `LLM_BRIDGE_CATALOG_BASE_URL`：OpenRouter API 地址（默认：https://openrouter.ai/api/v1）
- `LLM_BRIDGE_CATALOG_OUTPUT_MODALITIES`：输出模式（默认：text）
- `LLM_BRIDGE_CATALOG_REFRESH_INTERVAL_SECS`：刷新间隔（默认：900 秒）
- `LLM_BRIDGE_CATALOG_REQUEST_TIMEOUT_SECS`：请求超时（默认：15 秒）
- `LLM_BRIDGE_CATALOG_STRICT_BOOTSTRAP`：严格启动模式（默认：true）
- `LLM_BRIDGE_CATALOG_COUNT_CONSISTENCY_CHECK`：数量一致性检查（默认：true）
- `LLM_BRIDGE_CATALOG_API_KEY`：OpenRouter API 密钥（可选）

### 8.2 配置加载

配置在应用启动时从环境变量加载，存储在 `RuntimeSettings` 结构中：

```rust
struct RuntimeSettings {
    gateway_id: String,
    server: ServerConfig,
    database: DatabaseConfig,
    model_catalog: ModelCatalogConfig,
}
```

## 9. 前端架构

### 9.1 技术栈

- **框架**：Svelte 5（使用 Runes API）
- **构建工具**：Vite 8
- **样式**：TailwindCSS 4
- **UI 组件**：bits-ui
- **路由**：svelte-spa-router
- **状态管理**：@tanstack/svelte-store
- **类型安全**：自动生成的 TypeScript 客户端

### 9.2 页面结构

- **模型目录页面**：`ModelsPage.svelte` - 展示所有可用模型
- **提供者管理页面**：`ProvidersPage.svelte` - 管理提供者配置
- **模型绑定页面**：`BindingsPage.svelte` - 管理模型路由绑定

### 9.3 类型生成

使用 `ts-rs` 和 `axfetchum` 自动生成 TypeScript 类型：

```rust
#[derive(Serialize, TS)]
#[ts(export)]
struct ProviderResponse {
    provider_name: String,
    provider_type: ProviderType,
    base_url: Option<String>,
}
```

生成的 TypeScript 类型：
```typescript
export interface ProviderResponse {
    providerName: string;
    providerType: ProviderType;
    baseUrl?: string;
}
```

### 9.4 API 客户端

通过 `generate_ts_client.rs` 测试自动生成 TypeScript API 客户端：
- 类型安全的 API 调用
- 自动处理认证
- 错误处理

## 10. 部署与运维

### 10.1 构建和运行

**构建后端**：
```bash
cargo build --release
```

**构建前端**：
```bash
cd frontend
bun install
bun run build
```

**运行**：
```bash
./llm-bridge
```

### 10.2 监控和日志

**日志**：
- 使用 `tracing` 库记录日志
- 支持结构化日志
- 集成 OpenTelemetry

**指标**：
- 通过 OpenTelemetry 导出指标
- 可接入 Prometheus、Grafana 等

**链路追踪**：
- 通过 OpenTelemetry 导出链路
- 可接入 Jaeger、Zipkin 等

### 10.3 性能考虑

**并发处理**：
- Actor 模型天然支持高并发
- 每个 WebSocket 连接独立的 ConnectionActor
- 请求级别的 ProviderActor 隔离

**资源管理**：
- 异步 I/O 避免阻塞
- 流式处理减少内存占用
- 定期清理无效数据

**优化建议**：
- 调整 tokio 线程池大小
- 配置合适的数据库路径（SSD）
- 根据负载调整刷新间隔

### 10.4 安全考虑

**认证**：
- Bearer Token 认证
- API 密钥加密存储
- 环境变量管理敏感信息

**网络安全**：
- HTTPS（建议配置反向代理）
- CORS 配置（建议配置反向代理）

**数据安全**：
- API 密钥不记录在日志中
- 敏感信息存储在独立的 keyspace

### 10.5 故障恢复

**数据库**：
- fjall 支持 WAL（Write-Ahead Logging）
- 定期持久化到磁盘
- 数据文件位于 `./data/llm-bridge`

**Actor 监督**：
- Actor 失败时自动重启
- 连接断开时清理资源
- 错误传播和恢复

**容错机制**：
- 提供者失败时返回错误
- 模型目录获取失败时使用缓存
- 严格启动模式可配置

## 11. 扩展性设计

### 11.1 添加新的 LLM 提供者

1. 在 `ProviderType` 枚举中添加新类型
2. 在 `src/actors/provider/adapters/` 创建新的适配器文件
3. 在 `adapters.rs` 的 `stream_chat` 函数中添加分支
4. 实现提供者特定的 API 调用逻辑

### 11.2 添加新的消息类型

1. 在 `protocol.rs` 中定义新的消息类型
2. 在相应的 Actor 中添加消息处理逻辑
3. 使用 `ts-rs` 导出 TypeScript 类型

### 11.3 扩展数据模型

1. 在 `db/mod.rs` 中定义新的数据结构
2. 添加对应的 keyspace
3. 实现 CRUD 操作
4. 更新 schema 版本

### 11.4 添加新的 API 端点

1. 在 `server/admin.rs` 中定义新的路由
2. 实现处理函数
3. 使用 `axfetchum` 导出类型定义
4. 运行 `cargo test generate_ts_client` 生成 TypeScript 客户端

## 12. 开发指南

### 12.1 项目结构

```
llm-bridge/
├── Cargo.toml                  # Rust 项目配置
├── src/
│   ├── main.rs                 # 应用入口
│   ├── lib.rs                  # 库入口
│   ├── protocol.rs             # 通信协议定义
│   ├── types.rs                # 类型定义
│   ├── actors/                 # Actor 实现
│   │   ├── mod.rs
│   │   ├── gateway_manager.rs  # 网关管理器
│   │   ├── connection.rs       # 连接管理器
│   │   └── provider/           # 提供者 Actor
│   │       ├── mod.rs
│   │       └── adapters/       # 提供者适配器
│   ├── server/                 # HTTP 服务器
│   │   ├── mod.rs
│   │   ├── ws.rs               # WebSocket 处理
│   │   └── admin.rs            # REST API
│   ├── db/                     # 数据库
│   │   └── mod.rs
│   ├── config/                 # 配置管理
│   │   ├── mod.rs
│   │   ├── models.rs           # 配置模型
│   │   └── openrouter_catalog.rs
│   ├── routing/                # 路由逻辑
│   │   ├── mod.rs
│   │   └── models.rs
│   └── observability/          # 可观测性
│       └── mod.rs
├── frontend/                   # 前端项目
│   ├── package.json
│   ├── src/
│   │   ├── main.ts             # 前端入口
│   │   ├── App.svelte          # 主应用组件
│   │   ├── lib/                # 库代码
│   │   │   ├── ModelsPage.svelte
│   │   │   ├── ProvidersPage.svelte
│   │   │   ├── BindingsPage.svelte
│   │   │   ├── components/      # UI 组件
│   │   │   └── bindings/        # 自动生成的类型
│   │   └── assets/
│   └── dist/                   # 构建输出
├── docs/                       # 文档
│   ├── admin-api.md            # REST API 文档
│   └── architecture.md         # 架构文档
├── examples/                   # 示例代码
│   ├── openai_stream_cli.rs
│   └── anthropic_stream_cli.rs
└── tests/                      # 测试
    └── generate_ts_client.rs   # TypeScript 客户端生成
```

### 12.2 开发流程

1. **添加新功能**：
   - 在相应的模块中添加代码
   - 更新类型定义
   - 添加测试
   - 更新文档

2. **测试**：
   - 运行单元测试：`cargo test`
   - 运行集成测试：`cargo test --test integration`
   - 类型生成测试：`cargo test generate_ts_client`

3. **构建和部署**：
   - 开发模式：`cargo run`
   - 生产构建：`cargo build --release`
   - 前端构建：`cd frontend && bun run build`

### 12.3 最佳实践

**代码组织**：
- 遵循模块化设计
- 单一职责原则
- 清晰的错误处理

**性能优化**：
- 使用异步 I/O
- 避免不必要的克隆
- 流式处理大数据

**安全性**：
- 验证所有输入
- 不记录敏感信息
- 使用安全的序列化

**可维护性**：
- 添加充分的注释
- 编写单元测试
- 保持文档更新

## 13. 未来规划

### 13.1 短期目标

- [ ] 完成 Gemini 提供者支持
- [ ] 添加请求限流
- [ ] 实现请求重试机制
- [ ] 添加请求缓存
- [ ] 完善监控指标

### 13.2 中期目标

- [ ] 支持自定义模型能力配置
- [ ] 实现智能路由策略
- [ ] 添加请求日志和审计
- [ ] 实现配置热重载
- [ ] 支持集群部署

### 13.3 长期目标

- [ ] 支持模型负载均衡
- [ ] 实现成本优化策略
- [ ] 添加模型性能监控
- [ ] 支持多租户
- [ ] 实现模型 A/B 测试

## 14. 总结

LLM-Bridge 是一个设计良好的 LLM 网关服务，采用了现代化的技术栈和架构设计：

1. **高性能**：基于 Rust 和 Actor 模型，支持高并发
2. **可扩展**：模块化设计，易于添加新的提供者和功能
3. **易维护**：清晰的代码结构，完善的文档
4. **类型安全**：自动生成 TypeScript 类型，前后端一致
5. **可观测**：集成 OpenTelemetry，支持日志、指标、链路追踪

通过本文档，开发者可以全面了解系统的设计和实现，便于后续的开发、运维和扩展。