# WebSocket RPC

端点：`GET /v1/ws`，升级前验证 `Authorization: Bearer lb_...`；无效或缺失 Token 返回 HTTP 401。建议子协议 `lm-bridge.v1`。外部网络使用 `wss://`。浏览器原生 WebSocket 无法设置任意 Authorization header；本接口面向能配置握手 header 的插件/客户端，不接受 query-string Token。

## 信封

所有应用消息为 JSON text frame。请求 ID 非空、最多 128 字节、不能与本连接正在执行的 chat 重复。ID 归属于连接，不可跨连接取消。

```json
{"id":"models-1","method":"listModels"}
{"id":"chat-1","method":"chat","params":{"model":"fixture/model","messages":[{"role":"user","content":[{"value":"hello"}]}],"maxTokens":512}}
{"id":"cancel-1","method":"cancel","targetId":"chat-1"}
```

chat 消息不是 OpenAI 的字符串 content：它使用 `LanguageModelChatMessage`，role 为 user/assistant/system/developer，content 为 LM parts 数组。工具调用为 `{callId,name,input}`，工具结果为 `{callId,content:[{value:...}]}`，二进制输入为 `{mimeType,data:[0,1,...]}`。工具定义为 `{name,description?,inputSchema}`。

其余可选参数：tools、toolChoice、temperature、topP、stop、responseFormat、reasoning、seed、frequencyPenalty、presencePenalty、logitBias、maxCompletionTokens。准确类型见 `frontend/src/bindings/WsChatParams.ts` 及相关 LM 绑定；不另加客户端臆测的字段。

响应按请求 ID 多路复用，且只有下列四类信封：

```json
{"id":"models-1","result":[{"name":"fixture/model","maxInputTokens":8192,"maxOutputTokens":4096,"toolCalling":true,"vision":false}]}
{"id":"chat-1","chunk":{"value":"hello"}}
{"id":"chat-1","chunk":{"inputTokens":40,"outputTokens":60,"totalTokens":100}}
{"id":"chat-1","done":{"finishReason":"stop","cancelled":false}}
{"id":"cancel-1","result":{"cancelled":true}}
{"id":"chat-1","done":{"finishReason":null,"cancelled":true}}
{"id":null,"error":{"code":"invalid_request","message":"JSON text frames are required"}}
```

模型结果元素的完整结构以 `LMModelInfo` 绑定为准；返回列表按当前 Token 的 allowedModels 过滤。chunk 使用已有未标记的 `LMResponsePart` union，不增加 type/kind 包装。Text/Thinking 增量转发；ToolCall 的 JSON 参数累计完整后发出，不能把参数碎片当作完整调用。usage 由上游提供，不保证每个请求均有；done 不伪造 usage。错误以 error 终结，不再发送 success done。

错误码：`invalid_request`、`model_not_found`、`model_not_allowed`、`quota_exceeded`、`provider_error`、`request_not_found`、`internal_error`。坏 JSON 会尽量保留合法 id，否则 id=null。二进制应用帧返回 invalid_request，不直接作为聊天内容。

## 生命周期与限制

- 每连接最多 8 个并发 chat；第 9 个返回 invalid_request。listModels/cancel 不占聊天槽位。
- 出站 mpsc 容量 64；队列满或写入超时触发连接清理。关闭码 1008，写入超时 5 秒。
- 服务端每 30 秒 Ping，处理对端 Ping/Pong。单条入站消息上限 2 MiB。
- cancel 首先回复取消命令的 result；目标 chat 在共享结算完成后发送 cancelled done。完成后的 targetId 返回 request_not_found。
- 断连/慢消费者会取消本连接所有请求并关闭上游流；不会直接 abort 负责计费的 supervisor。
- HTTP 与 WS 共用准入、预留、路由回退和唯一结算点。有真实 usage 时修正预留；已经启动但 usage 未知的取消/错误保留估算预留，不谎报零消耗。已经输出的请求不切换备用上游。
- Token 只在握手时验证；连接建立后吊销 Token 不主动断开已有连接。部署或客户端应按自身凭据策略重连。

## 可运行示例

```bash
LLM_BRIDGE_API_KEY=lb_... cargo run --example ws_chat_client -- ws://127.0.0.1:3000/v1/ws MODEL hello
```

示例在 `examples/ws_chat_client.rs`，使用现有 tokio-tungstenite 0.29，正确处理握手 header、Ping/Pong、done 和 error。依赖依据：[官方 connect_async API](https://docs.rs/tokio-tungstenite/0.29.0/tokio_tungstenite/fn.connect_async.html)、[社区鉴权/握手讨论](https://github.com/snapview/tokio-tungstenite/issues/327)。契约回归位于 `tests/ws_chat_contract.rs`。网关协议不等于可安装的 VS Code 插件已经交付。
