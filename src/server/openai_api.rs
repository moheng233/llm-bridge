use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
};
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;
use tracing::instrument;

use crate::actors::provider::adapters::openai_chat_completions::flatten_thinking_value_for_sse;
use crate::actors::provider::{ProviderChatRequest, ProviderResponseMetadata};
use crate::middleware::token_auth::TokenAuth;
use crate::server::AppState;
use crate::types::{
    LMResponsePart, LanguageModelChatMessage, LanguageModelChatMessageRole, LanguageModelInputPart,
    LanguageModelTextPart, LanguageModelTool, LanguageModelToolCallPart,
    LanguageModelToolResultContent, LanguageModelToolResultPart,
};

// ── Auth ── (legacy — used by check_auth only)

#[allow(dead_code)]
#[allow(clippy::result_large_err)]
fn check_auth(state: &AppState, headers: &HeaderMap) -> Result<(), Response> {
    let Some(expected) = &state.auth_token else {
        return Ok(());
    };
    let provided = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    if provided == Some(expected) {
        Ok(())
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "unauthorized"})),
        )
            .into_response())
    }
}

// ── GET /v1/models ──

#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiModelList {
    object: &'static str,
    data: Vec<OpenAiModelEntry>,
}

/// 单个模型的 API 条目（增强版，包含提供者列表和各自的定价/能力）。
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct OpenAiModelEntry {
    id: String,
    object: &'static str,
    created: i64,
    /// 主要提供者（第一个可用提供者）
    #[serde(rename = "owned_by")]
    #[ts(rename = "owned_by")]
    owned_by: String,
    /// 模型的标称能力
    capabilities: OpenAiModelCapabilities,
    /// 各提供者的定价和能力覆盖
    providers: Vec<OpenAiModelProviderInfo>,
}

#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct OpenAiModelCapabilities {
    max_input_tokens: u32,
    max_output_tokens: u32,
    tool_calling: bool,
    vision: bool,
    thinking: Option<bool>,
    adaptive_thinking: Option<bool>,
}

#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct OpenAiModelProviderInfo {
    provider_id: String,
    provider_display_name: String,
    /// 提供者覆盖的能力（nullable = 使用模型标称值）
    max_input_tokens: Option<i64>,
    max_output_tokens: Option<i64>,
    tool_calling: Option<bool>,
    vision: Option<bool>,
    thinking: Option<bool>,
    adaptive_thinking: Option<bool>,
    /// 提供者特定定价（每 1M tokens，美元）
    input_price_per_1m: Option<f64>,
    output_price_per_1m: Option<f64>,
    cache_read_price_per_1m: Option<f64>,
    enabled: bool,
    priority: i64,
}

#[instrument(level = "debug", skip(state))]
pub async fn list_models(
    State(state): State<AppState>,
    TokenAuth(token): TokenAuth,
) -> Result<Json<OpenAiModelList>, Response> {
    let all_models = state.store.list_available_models().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": {
                    "message": e,
                    "type": "internal_error",
                    "code": "internal_error"
                }
            })),
        )
            .into_response()
    })?;

    // Filter models based on token's allowed_models
    let allowed: Vec<String> = serde_json::from_str(&token.allowed_models).unwrap_or_default();

    let data = all_models
        .into_iter()
        .filter(|m| {
            if allowed.is_empty() {
                true
            } else {
                allowed.iter().any(|a| a == &m.model_name)
            }
        })
        .map(|m| {
            let owned_by = m
                .providers
                .first()
                .map(|p| p.provider_id.clone())
                .unwrap_or_default();

            let providers = m
                .providers
                .into_iter()
                .map(|p| OpenAiModelProviderInfo {
                    provider_id: p.provider_id,
                    provider_display_name: p.provider_display_name,
                    max_input_tokens: p.max_input_tokens,
                    max_output_tokens: p.max_output_tokens,
                    tool_calling: p.tool_calling,
                    vision: p.vision,
                    thinking: p.thinking,
                    adaptive_thinking: p.adaptive_thinking,
                    input_price_per_1m: p.input_price_per_1m,
                    output_price_per_1m: p.output_price_per_1m,
                    cache_read_price_per_1m: p.cache_read_price_per_1m,
                    enabled: p.enabled,
                    priority: p.priority,
                })
                .collect();

            OpenAiModelEntry {
                id: m.model_name,
                object: "model",
                created: 0,
                owned_by,
                capabilities: OpenAiModelCapabilities {
                    max_input_tokens: m.nominal_capabilities.max_input_tokens,
                    max_output_tokens: m.nominal_capabilities.max_output_tokens,
                    tool_calling: m.nominal_capabilities.tool_calling,
                    vision: m.nominal_capabilities.vision,
                    thinking: m.nominal_capabilities.thinking,
                    adaptive_thinking: m.nominal_capabilities.adaptive_thinking,
                },
                providers,
            }
        })
        .collect();

    Ok(Json(OpenAiModelList {
        object: "list",
        data,
    }))
}

// ── POST /v1/chat/completions ──

#[derive(Debug, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<OpenAiMessage>,
    #[serde(default)]
    pub stream: bool,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f64>,
    #[serde(default)]
    pub stream_options: Option<OpenAiStreamOptions>,
    /// OpenAI 标准工具声明：[{"type":"function","function":{"name","description","parameters"}}]
    #[serde(default)]
    pub tools: Option<Vec<OpenAiTool>>,
    /// "auto" | "none" | {"type":"function","function":{"name":...}}
    #[serde(default)]
    pub tool_choice: Option<serde_json::Value>,
    /// 停止序列：string | string[]
    #[serde(default, deserialize_with = "deserialize_stop")]
    pub stop: Option<Vec<String>>,
    /// 结构化输出 / JSON mode：{"type":"json_object"} | {"type":"json_schema","json_schema":{...}}
    #[serde(default)]
    pub response_format: Option<OpenAiResponseFormat>,
    /// OpenAI 推理强度（o 系列 / gpt-5）："low" | "medium" | "high" | "minimal" | "none" | "xhigh"
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    /// OpenRouter 扩展推理配置：{"effort":...} | {"max_tokens":...} | {"enabled":true} | {"exclude":...}
    #[serde(default)]
    pub reasoning: Option<OpenAiReasoning>,
    /// 确定性采样种子
    #[serde(default)]
    pub seed: Option<i64>,
    /// 频率惩罚（-2.0 ~ 2.0）
    #[serde(default)]
    pub frequency_penalty: Option<f64>,
    /// 存在惩罚（-2.0 ~ 2.0）
    #[serde(default)]
    pub presence_penalty: Option<f64>,
    /// token logit 偏置：{"token_id": bias}
    #[serde(default)]
    pub logit_bias: Option<std::collections::HashMap<String, f64>>,
    /// 推理模型的最大补全 token 数（含 reasoning tokens）
    #[serde(default)]
    pub max_completion_tokens: Option<u32>,
}

/// `response_format` 仅支持 OpenAI 官方两种形态；`text` 视为缺省不单独处理。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAiResponseFormat {
    JsonObject,
    JsonSchema { json_schema: serde_json::Value },
}

/// OpenRouter `reasoning` 对象：effort / max_tokens / enabled / exclude 四选若干。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OpenAiReasoning {
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub exclude: Option<bool>,
}

/// OpenAI `stop` 允许单个字符串或字符串数组。
fn deserialize_stop<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(vec![s])),
        Some(serde_json::Value::Array(arr)) => arr
            .into_iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| D::Error::custom("stop array must contain only strings"))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        _ => Err(D::Error::custom(
            "stop must be a string or an array of strings",
        )),
    }
}

/// OpenAI 工具声明（`type: "function"`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiTool {
    pub r#type: String,
    pub function: OpenAiToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiToolFunction {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parameters: serde_json::Value,
}

impl OpenAiTool {
    /// 转换为协议无关的内部工具定义。
    fn into_internal(self) -> LanguageModelTool {
        LanguageModelTool {
            name: self.function.name,
            description: self.function.description,
            input_schema: self.function.parameters,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAiMessage {
    pub role: String,
    #[serde(default, deserialize_with = "deserialize_content")]
    pub content: OpenAiContent,
    pub name: Option<String>,
    /// assistant 消息携带的工具调用列表
    #[serde(default)]
    pub tool_calls: Option<Vec<OpenAiMessageToolCall>>,
    /// role=tool 时携带，对应要回填的 tool_call id
    #[serde(default)]
    pub tool_call_id: Option<String>,
}

/// assistant 消息中的单个工具调用（OpenAI 格式）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiMessageToolCall {
    pub id: String,
    pub r#type: String,
    pub function: OpenAiMessageToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiMessageToolCallFunction {
    pub name: String,
    /// JSON 字符串形式的参数
    pub arguments: String,
}

fn deserialize_content<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<OpenAiContent, D::Error> {
    Ok(Option::<OpenAiContent>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAiContent {
    String(String),
    Array(Vec<OpenAiContentPart>),
}

impl Default for OpenAiContent {
    fn default() -> Self {
        OpenAiContent::String(String::new())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAiContentPart {
    Text { text: String },
    ImageUrl { image_url: OpenAiImageUrl },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAiImageUrl {
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAiStreamOptions {
    pub include_usage: Option<bool>,
}

#[instrument(skip_all, fields(model = %req.model, request_id = %request_id))]
pub async fn chat_completions(
    State(state): State<AppState>,
    TokenAuth(token): TokenAuth,
    request_id: crate::middleware::request_id::RequestId,
    Json(req): Json<ChatCompletionRequest>,
) -> Result<Response, Response> {
    // 先验证并转换输入，非法图片或消息不占用配额。
    let messages = convert_messages(&req.messages).await?;
    let reasoning = merge_reasoning(req.reasoning_effort.as_deref(), req.reasoning.as_ref());
    let request = ProviderChatRequest {
        model: req.model.clone(),
        messages,
        tools: req
            .tools
            .map(|tools| tools.into_iter().map(OpenAiTool::into_internal).collect()),
        tool_choice: req.tool_choice,
        temperature: req.temperature,
        max_tokens: req.max_tokens,
        top_p: req.top_p,
        stop: req.stop,
        response_format: req.response_format.map(Into::into),
        reasoning,
        seed: req.seed,
        frequency_penalty: req.frequency_penalty,
        presence_penalty: req.presence_penalty,
        logit_bias: req.logit_bias,
        max_completion_tokens: req.max_completion_tokens,
    };
    let session = super::chat_common::prepare_chat_request(
        &state,
        &token,
        request,
        request_id.as_str().into(),
        crate::db::models::TraceInterface::OpenAiHttp,
    )
    .await
    .map_err(|error| {
        upstream_error_response(error.status, Some(error.code.into()), &error.message)
    })?;
    session
        .ready
        .await
        .map_err(|_| internal_error("chat supervisor stopped before startup"))?
        .map_err(|error| {
            upstream_error_response(error.status, Some(error.code.into()), &error.message)
        })?;
    if req.stream {
        return Ok(
            Sse::new(stream_to_sse(session.stream, req.model, session.metadata)).into_response(),
        );
    }
    let mut stream = session.stream;
    let mut content = String::new();
    let mut reasoning_content = String::new();
    let mut tool_calls = Vec::new();
    while let Some(item) = stream.next().await {
        match item {
            Ok(LMResponsePart::Text(text)) => content.push_str(&text.value),
            Ok(LMResponsePart::Thinking(thinking)) => reasoning_content.push_str(&flatten_thinking_value_for_sse(&thinking.value)),
            Ok(LMResponsePart::ToolCall(call)) => tool_calls.push(serde_json::json!({
                "id": call.call_id, "type": "function",
                "function": {"name": call.name, "arguments": serde_json::to_string(&call.input).map_err(|e| internal_error(&e.to_string()))?},
            })),
            _ => {}
        }
    }
    let outcome = session
        .done
        .await
        .map_err(|_| internal_error("chat supervisor stopped before settlement"))?;
    if let Some(error) = outcome.error {
        return Err(upstream_error_response(
            502,
            Some("provider_error".into()),
            &error,
        ));
    }
    let has_tools = !tool_calls.is_empty();
    let mut message = serde_json::json!({"role": "assistant", "content": content});
    if !reasoning_content.is_empty() {
        message["reasoning_content"] = reasoning_content.into();
    }
    if has_tools {
        message["tool_calls"] = tool_calls.into();
    }
    let finish_reason = outcome
        .usage
        .finish_reason
        .as_deref()
        .unwrap_or(if has_tools { "tool_calls" } else { "stop" });
    let mut response = serde_json::json!({
        "id": outcome.metadata.id.unwrap_or_else(|| format!("chatcmpl-{}", request_id.as_str())),
        "object": "chat.completion", "created": outcome.metadata.created.unwrap_or(0), "model": req.model,
        "choices": [{"index":0,"message":message,"finish_reason":finish_reason}],
    });
    if let Some(usage) = outcome.usage.to_openai_usage() {
        response["usage"] = usage;
    }
    Ok(Json(response).into_response())
}

/// 合并 OpenAI `reasoning_effort` 与 OpenRouter `reasoning` 对象为协议无关配置。
fn merge_reasoning(
    reasoning_effort: Option<&str>,
    reasoning: Option<&OpenAiReasoning>,
) -> Option<crate::types::LanguageModelReasoningConfig> {
    let effort = reasoning_effort
        .map(str::to_string)
        .or_else(|| reasoning.and_then(|r| r.effort.clone()));
    let max_tokens = reasoning.and_then(|r| r.max_tokens);

    // OpenRouter reasoning.enabled=true 但未给 effort/max_tokens 时，默认 medium
    let effort = effort.or_else(|| match reasoning {
        Some(r) if r.enabled == Some(true) => Some("medium".to_string()),
        _ => None,
    });

    if effort.is_none() && max_tokens.is_none() {
        return None;
    }

    Some(crate::types::LanguageModelReasoningConfig { effort, max_tokens })
}

impl From<OpenAiResponseFormat> for crate::types::LanguageModelResponseFormat {
    fn from(value: OpenAiResponseFormat) -> Self {
        match value {
            OpenAiResponseFormat::JsonObject => {
                crate::types::LanguageModelResponseFormat::JsonObject
            }
            OpenAiResponseFormat::JsonSchema { json_schema } => {
                crate::types::LanguageModelResponseFormat::JsonSchema { json_schema }
            }
        }
    }
}

/// 用于在流式 SSE 中共享上游 metadata（id/created）与角色发送状态。
struct SseSharedState {
    metadata_rx: tokio::sync::oneshot::Receiver<ProviderResponseMetadata>,
    upstream: Option<ProviderResponseMetadata>,
    role_sent: bool,
}

impl SseSharedState {
    fn upstream_id(&mut self) -> String {
        if self.upstream.is_none() {
            self.upstream = self.metadata_rx.try_recv().ok();
        }
        self.upstream
            .as_ref()
            .and_then(|m| m.id.clone())
            .unwrap_or_else(|| "chatcmpl-llm-bridge".to_string())
    }

    fn upstream_created(&mut self) -> u64 {
        if self.upstream.is_none() {
            self.upstream = self.metadata_rx.try_recv().ok();
        }
        self.upstream.as_ref().and_then(|m| m.created).unwrap_or(0)
    }
}

fn stream_to_sse(
    stream: impl Stream<Item = Result<LMResponsePart, String>> + Send + 'static,
    model: String,
    metadata_rx: tokio::sync::oneshot::Receiver<ProviderResponseMetadata>,
) -> impl Stream<Item = Result<Event, axum::Error>> + Send + 'static {
    let shared = std::sync::Arc::new(tokio::sync::Mutex::new(SseSharedState {
        metadata_rx,
        upstream: None,
        role_sent: false,
    }));

    // Map each item, then append a [DONE] sentinel at the end.
    let mapped = stream.then(move |item| {
        let shared = shared.clone();
        let model = model.clone();
        async move { map_part_to_sse(item, &model, shared).await }
    });

    use futures_util::stream;
    let done = stream::once(async { Ok(Event::default().data("[DONE]")) });
    mapped.chain(done)
}

async fn map_part_to_sse(
    item: Result<LMResponsePart, String>,
    model: &str,
    shared: std::sync::Arc<tokio::sync::Mutex<SseSharedState>>,
) -> Result<Event, axum::Error> {
    match item {
        Ok(part) => {
            let mut delta = serde_json::Map::new();
            let mut usage_json: Option<serde_json::Value> = None;

            let finish_reason = match &part {
                LMResponsePart::Text(t) => {
                    delta.insert(
                        "content".to_string(),
                        serde_json::Value::String(t.value.clone()),
                    );
                    None
                }
                LMResponsePart::Thinking(t) => {
                    // Reasoning/thinking content — exposed as `reasoning_content` per DeepSeek / OpenAI extended format.
                    let text = flatten_thinking_value_for_sse(&t.value);
                    delta.insert(
                        "reasoning_content".to_string(),
                        serde_json::Value::String(text),
                    );
                    None
                }
                LMResponsePart::ToolCall(tc) => {
                    // 语义基准：ToolCall 累积完整后一次性发射（含完整 arguments）。
                    // finish_reason 不由此处标注——上游 Usage part 会携带真实的
                    // "tool_calls" finish_reason 单独成 chunk。
                    delta.insert(
                        "tool_calls".to_string(),
                        serde_json::json!([{
                            "index": 0,
                            "id": tc.call_id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": serde_json::to_string(&tc.input)
                                    .unwrap_or_default(),
                            }
                        }]),
                    );
                    None
                }
                LMResponsePart::Usage(u) => {
                    // OpenAI include_usage 格式：choices 为空数组的 usage-only chunk
                    if u.input_tokens.is_some() {
                        usage_json = Some(serde_json::json!({
                            "prompt_tokens": u.input_tokens,
                            "completion_tokens": u.output_tokens.unwrap_or(0),
                            "total_tokens": u.total_tokens,
                        }));
                    }
                    // finish_reason 由下方 chunk 的 choices 携带
                    u.finish_reason.as_deref()
                }
                _ => None,
            };

            // usage-only chunk（OpenAI 格式：choices 为空）
            if let Some(usage) = usage_json {
                let mut guard = shared.lock().await;
                let id = guard.upstream_id();
                let created = guard.upstream_created();
                let chunk = serde_json::json!({
                    "id": id,
                    "object": "chat.completion.chunk",
                    "created": created,
                    "model": model,
                    "choices": [],
                    "usage": usage,
                });
                return Ok(Event::default().data(serde_json::to_string(&chunk).unwrap_or_default()));
            }

            let mut guard = shared.lock().await;
            let id = guard.upstream_id();
            let created = guard.upstream_created();

            // OpenAI 规范：首包必须携带 role: "assistant"
            if !guard.role_sent {
                delta.insert(
                    "role".to_string(),
                    serde_json::Value::String("assistant".to_string()),
                );
                guard.role_sent = true;
            }

            let chunk = serde_json::json!({
                "id": id,
                "object": "chat.completion.chunk",
                "created": created,
                "model": model,
                "choices": [{
                    "index": 0,
                    "delta": delta,
                    "finish_reason": finish_reason,
                }]
            });

            Ok(Event::default().data(serde_json::to_string(&chunk).unwrap_or_default()))
        }
        Err(e) => {
            // #13：流式错误 chunk 携带 code 字段（P3 #18 错误格式部分）
            let error_chunk = serde_json::json!({
                "error": {
                    "message": e,
                    "type": "provider_error",
                    "code": "provider_error"
                }
            });
            Ok(Event::default().data(serde_json::to_string(&error_chunk).unwrap_or_default()))
        }
    }
}

/// #11：单张图片抓取上限 10 MiB，每条消息最多 8 个图片 part。
const MAX_IMAGE_PARTS_PER_MESSAGE: usize = 8;

async fn convert_messages(
    messages: &[OpenAiMessage],
) -> Result<Vec<LanguageModelChatMessage>, Response> {
    let mut out = Vec::with_capacity(messages.len());
    for msg in messages {
        out.push(convert_single_message(msg).await?);
    }
    Ok(out)
}

#[allow(clippy::result_large_err)]
async fn convert_single_message(msg: &OpenAiMessage) -> Result<LanguageModelChatMessage, Response> {
    let role = match msg.role.as_str() {
        "user" => LanguageModelChatMessageRole::User,
        "assistant" => LanguageModelChatMessageRole::Assistant,
        "system" => LanguageModelChatMessageRole::System,
        "developer" => LanguageModelChatMessageRole::Developer,
        _ => LanguageModelChatMessageRole::User,
    };

    // role=tool → ToolResult part
    if msg.role == "tool" {
        let call_id = msg.tool_call_id.clone().unwrap_or_default();
        let text = content_to_text(&msg.content);
        return Ok(LanguageModelChatMessage {
            role: LanguageModelChatMessageRole::User,
            content: vec![LanguageModelInputPart::ToolResult(
                LanguageModelToolResultPart {
                    call_id,
                    content: vec![LanguageModelToolResultContent::Text(
                        LanguageModelTextPart { value: text },
                    )],
                },
            )],
            name: msg.name.clone(),
        });
    }

    let mut parts: Vec<LanguageModelInputPart> = Vec::new();

    // assistant 携带 tool_calls → ToolCall parts
    if let Some(tool_calls) = &msg.tool_calls {
        for tc in tool_calls {
            let input = serde_json::from_str(&tc.function.arguments)
                .unwrap_or(serde_json::Value::String(tc.function.arguments.clone()));
            parts.push(LanguageModelInputPart::ToolCall(
                LanguageModelToolCallPart {
                    call_id: tc.id.clone(),
                    name: tc.function.name.clone(),
                    input,
                },
            ));
        }
    }

    // 文本 + 多模态图片 part（保持原始顺序：先文本再图片会丢序，因此改为按 part 顺序展开）
    match &msg.content {
        OpenAiContent::String(s) => {
            if !s.is_empty() || parts.is_empty() {
                parts.insert(
                    0,
                    LanguageModelInputPart::Text(LanguageModelTextPart { value: s.clone() }),
                );
            }
        }
        OpenAiContent::Array(content_parts) => {
            let mut image_count = 0usize;
            for part in content_parts {
                match part {
                    OpenAiContentPart::Text { text } => {
                        parts.push(LanguageModelInputPart::Text(LanguageModelTextPart {
                            value: text.clone(),
                        }));
                    }
                    OpenAiContentPart::ImageUrl { image_url } => {
                        image_count += 1;
                        if image_count > MAX_IMAGE_PARTS_PER_MESSAGE {
                            return Err(bad_request(&format!(
                                "too many image parts in one message (max {MAX_IMAGE_PARTS_PER_MESSAGE})"
                            )));
                        }
                        let data_part = resolve_image_url(&image_url.url).await?;
                        parts.push(LanguageModelInputPart::Data(data_part));
                    }
                }
            }
            if parts.is_empty() {
                parts.push(LanguageModelInputPart::Text(LanguageModelTextPart {
                    value: String::new(),
                }));
            }
        }
    }

    Ok(LanguageModelChatMessage {
        role,
        content: parts,
        name: msg.name.clone(),
    })
}

/// #11：将 OpenAI image_url 解析为内部 LanguageModelDataPart。
/// 支持 `data:<mime>;base64,<data>` 与 http(s) URL（抓取字节流）。
async fn resolve_image_url(url: &str) -> Result<crate::types::LanguageModelDataPart, Response> {
    super::images::resolve_image_url(url)
        .await
        .map_err(|error| bad_request(&error))
}

fn bad_request(msg: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({
            "error": {
                "message": msg,
                "type": "invalid_request_error",
                "code": "invalid_request_error"
            }
        })),
    )
        .into_response()
}

fn content_to_text(content: &OpenAiContent) -> String {
    match content {
        OpenAiContent::String(s) => s.clone(),
        OpenAiContent::Array(parts) => parts
            .iter()
            .filter_map(|p| match p {
                OpenAiContentPart::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn internal_error(msg: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "error": {
                "message": msg,
                "type": "internal_error",
                "code": "internal_error"
            }
        })),
    )
        .into_response()
}

/// #13：构造透传上游语义状态码的错误响应。
fn upstream_error_response(status: u16, code: Option<String>, message: &str) -> Response {
    let status_code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    let error_type = match status {
        401 => "authentication_error",
        402 => "billing_error",
        429 => "rate_limit_exceeded",
        s if s >= 500 => "upstream_error",
        _ => "invalid_request_error",
    };
    (
        status_code,
        Json(serde_json::json!({
            "error": {
                "message": message,
                "type": error_type,
                "code": code.unwrap_or_else(|| error_type.to_string()),
            }
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD as BASE64;

    #[tokio::test]
    async fn resolve_image_url_decodes_data_uri() {
        let raw = b"\x89PNG\r\n\x1a\n";
        let url = format!("data:image/png;base64,{}", BASE64.encode(raw));
        let part = resolve_image_url(&url).await.unwrap();
        assert_eq!(part.mime_type, "image/png");
        assert_eq!(part.data, raw);
    }

    #[tokio::test]
    async fn resolve_image_url_rejects_non_image_data_uri() {
        let url = format!("data:text/html;base64,{}", BASE64.encode(b"<html>"));
        let result = resolve_image_url(&url).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn resolve_image_url_rejects_bad_base64() {
        let result = resolve_image_url("data:image/png;base64,!!!not-base64!!!").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn resolve_image_url_rejects_unknown_scheme() {
        let result = resolve_image_url("file:///etc/passwd").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn convert_messages_maps_image_url_part() {
        let raw = b"fake-image";
        let messages = vec![OpenAiMessage {
            role: "user".to_string(),
            content: OpenAiContent::Array(vec![
                OpenAiContentPart::Text {
                    text: "describe this".to_string(),
                },
                OpenAiContentPart::ImageUrl {
                    image_url: OpenAiImageUrl {
                        url: format!("data:image/jpeg;base64,{}", BASE64.encode(raw)),
                    },
                },
            ]),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }];
        let converted = convert_messages(&messages).await.unwrap();
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].content.len(), 2);
        assert!(matches!(
            &converted[0].content[0],
            LanguageModelInputPart::Text(t) if t.value == "describe this"
        ));
        match &converted[0].content[1] {
            LanguageModelInputPart::Data(d) => {
                assert_eq!(d.mime_type, "image/jpeg");
                assert_eq!(d.data, raw);
            }
            other => panic!("expected Data part, got {other:?}"),
        }
    }
}
