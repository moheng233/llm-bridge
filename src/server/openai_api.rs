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
use ractor::Actor;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;
use tracing::instrument;

use crate::actors::provider::adapters::openai_chat_completions::flatten_thinking_value_for_sse;
use crate::actors::provider::{
    ProviderActor, ProviderChatRequest, ProviderMessage, ProviderRuntimeConfig,
};
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
    #[serde(default)]
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

#[instrument(level = "info", skip(state, token), fields(model = %req.model, stream = req.stream))]
pub async fn chat_completions(
    State(state): State<AppState>,
    TokenAuth(token): TokenAuth,
    Json(req): Json<ChatCompletionRequest>,
) -> Result<Response, Response> {
    // Check model access
    let allowed: Vec<String> = serde_json::from_str(&token.allowed_models).unwrap_or_default();
    if !allowed.is_empty() && !allowed.iter().any(|a| a == &req.model) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": {
                    "message": format!("model '{}' is not allowed for this token", req.model),
                    "type": "model_access_denied",
                    "code": "model_access_denied"
                }
            })),
        )
            .into_response());
    }

    let routes = state.store.resolve_model(&req.model).await.map_err(|e| {
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
    if routes.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": {
                    "message": format!("model '{}' is not available", req.model),
                    "type": "model_not_found",
                    "code": "model_not_found"
                }
            })),
        )
            .into_response());
    }

    // Take the first (highest priority) route.
    let route = &routes[0];

    // Phase 2: Quota check and deduct (before making upstream call)
    let estimated_tokens = estimate_token_count(&req.messages, req.tools.as_deref());
    if let Err(quota_err) =
        crate::auth::quota::check_and_deduct(&state.db, &token, estimated_tokens).await
    {
        let msg = quota_err.to_string();
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": {
                    "message": msg,
                    "type": "quota_exceeded",
                    "code": "quota_exceeded"
                }
            })),
        )
            .into_response());
    }

    // Convert OpenAI messages to our internal format.
    let messages = convert_messages(&req.messages)?;

    let provider_config = ProviderRuntimeConfig {
        id: route.provider_name.clone(),
        compatibility: route.compatibility.clone(),
        api_key: route.api_key.clone(),
        base_url: route.base_url.clone(),
        compat_settings: route.compat_settings.clone(),
    };

    let provider_request = ProviderChatRequest {
        model: route.provider_model_name.clone(),
        messages,
        tools: req
            .tools
            .map(|tools| tools.into_iter().map(OpenAiTool::into_internal).collect()),
        tool_choice: req.tool_choice.clone(),
        temperature: req.temperature,
        max_tokens: req.max_tokens,
        top_p: req.top_p,
    };

    // Spawn provider actor and get stream.
    let (provider_ref, provider_handle) = Actor::spawn(None, ProviderActor, provider_config)
        .await
        .map_err(|e| internal_error(&e.to_string()))?;

    let stream = ractor::call_t!(
        provider_ref,
        |reply| ProviderMessage::ChatRequest(provider_request, reply),
        30_000
    )
    .map_err(|e| internal_error(&e.to_string()))?
    .map_err(|e| internal_error(&e))?;

    // Clean up provider actor when stream ends.
    let cleanup_handle = provider_handle;
    let cleanup_ref = provider_ref;

    if req.stream {
        let usage_handle = UsageHandle::default();
        let sse_stream = stream_to_sse(stream, req.model.clone(), usage_handle.clone());

        // Spawn cleanup after stream is consumed.
        let settle_state = state.clone();
        let settle_ctx = crate::auth::quota::TokenQuotaContext::from_token(&token);
        tokio::spawn(async move {
            cleanup_ref.stop(None);
            let _ = cleanup_handle.await;
            let usage = usage_handle.lock().await.clone();
            settle_quota_with_actual_usage(&settle_state, &settle_ctx, estimated_tokens, &usage)
                .await;
        });

        Ok(Sse::new(sse_stream).into_response())
    } else {
        // Non-streaming: collect all chunks and concatenate.
        let mut stream = stream;
        let mut content = String::new();
        let mut reasoning_content = String::new();
        let mut tool_calls: Vec<serde_json::Value> = Vec::new();
        let mut usage_acc = UsageAccumulator::default();
        while let Some(item) = stream.next().await {
            match item {
                Ok(LMResponsePart::Text(t)) => content.push_str(&t.value),
                Ok(LMResponsePart::Thinking(t)) => {
                    let text = crate::actors::provider::adapters::openai_chat_completions::flatten_thinking_value_for_sse(&t.value);
                    reasoning_content.push_str(&text);
                }
                Ok(LMResponsePart::ToolCall(tc)) => {
                    tool_calls.push(serde_json::json!({
                        "id": tc.call_id,
                        "type": "function",
                        "function": {
                            "name": tc.name,
                            "arguments": serde_json::to_string(&tc.input).unwrap_or_default(),
                        }
                    }));
                }
                Ok(LMResponsePart::Usage(u)) => usage_acc.merge(&u),
                Ok(_) => {}
                Err(e) => {
                    cleanup_ref.stop(None);
                    let _ = cleanup_handle.await;
                    return Err(internal_error(&e));
                }
            }
        }
        cleanup_ref.stop(None);
        let _ = cleanup_handle.await;

        // 按真实 usage 结算配额：多退少补（相对预估）
        let settle_ctx = crate::auth::quota::TokenQuotaContext::from_token(&token);
        settle_quota_with_actual_usage(&state, &settle_ctx, estimated_tokens, &usage_acc).await;

        let has_tool_calls = !tool_calls.is_empty();
        let mut message = serde_json::json!({
            "role": "assistant",
            "content": content,
        });
        if !reasoning_content.is_empty() {
            message["reasoning_content"] = serde_json::Value::String(reasoning_content);
        }
        if has_tool_calls {
            message["tool_calls"] = serde_json::Value::Array(tool_calls);
        }
        let finish_reason = usage_acc
            .finish_reason
            .as_deref()
            .unwrap_or(if has_tool_calls { "tool_calls" } else { "stop" });

        let mut response = serde_json::json!({
            "id": "chatcmpl-llm-bridge",
            "object": "chat.completion",
            "created": 0,
            "model": req.model,
            "choices": [{
                "index": 0,
                "message": message,
                "finish_reason": finish_reason
            }]
        });
        if let Some(usage_json) = usage_acc.to_openai_usage() {
            response["usage"] = usage_json;
        }

        Ok(Json(response).into_response())
    }
}

/// 跨多个 Usage part 聚合用量与 finish_reason。
#[derive(Default, Clone)]
struct UsageAccumulator {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
    reasoning_tokens: Option<u64>,
    cached_tokens: Option<u64>,
    finish_reason: Option<String>,
}

impl UsageAccumulator {
    fn merge(&mut self, u: &crate::types::LanguageModelUsagePart) {
        if u.input_tokens.is_some() {
            self.input_tokens = u.input_tokens;
        }
        if u.output_tokens.is_some() {
            self.output_tokens = u.output_tokens;
        }
        if u.total_tokens.is_some() {
            self.total_tokens = u.total_tokens;
        }
        if u.reasoning_tokens.is_some() {
            self.reasoning_tokens = u.reasoning_tokens;
        }
        if u.cached_tokens.is_some() {
            self.cached_tokens = u.cached_tokens;
        }
        if u.finish_reason.is_some() {
            self.finish_reason = u.finish_reason.clone();
        }
    }

    fn total(&self) -> Option<i64> {
        self.total_tokens
            .or(match (self.input_tokens, self.output_tokens) {
                (Some(i), Some(o)) => Some(i + o),
                _ => None,
            })
            .map(|t| t as i64)
    }

    fn to_openai_usage(&self) -> Option<serde_json::Value> {
        let input = self.input_tokens?;
        let output = self.output_tokens.unwrap_or(0);
        let mut usage = serde_json::json!({
            "prompt_tokens": input,
            "completion_tokens": output,
            "total_tokens": self.total_tokens.unwrap_or(input + output),
        });
        if let Some(reasoning) = self.reasoning_tokens {
            usage["completion_tokens_details"] =
                serde_json::json!({ "reasoning_tokens": reasoning });
        }
        if let Some(cached) = self.cached_tokens {
            usage["prompt_tokens_details"] = serde_json::json!({ "cached_tokens": cached });
        }
        Some(usage)
    }
}

/// 上游返回真实 usage 后，与预估扣减对账：多退少补。
async fn settle_quota_with_actual_usage(
    state: &AppState,
    ctx: &crate::auth::quota::TokenQuotaContext,
    estimated_tokens: i64,
    usage: &UsageAccumulator,
) {
    let Some(actual_total) = usage.total() else {
        return;
    };
    let delta = actual_total - estimated_tokens;
    if delta == 0 {
        return;
    }
    if let Err(e) = crate::auth::quota::adjust_usage(&state.db, ctx, delta).await {
        tracing::warn!(
            token_id = ctx.token_id,
            delta,
            "failed to settle quota with actual usage: {e}"
        );
    }
}

/// 流式路径共享的 usage 累积句柄。
type UsageHandle = std::sync::Arc<tokio::sync::Mutex<UsageAccumulator>>;

fn stream_to_sse(
    stream: impl Stream<Item = Result<LMResponsePart, String>> + Send + 'static,
    model: String,
    usage_handle: UsageHandle,
) -> impl Stream<Item = Result<Event, axum::Error>> + Send + 'static {
    stream.map(move |item| map_part_to_sse(item, &model, usage_handle.clone()))
}

fn map_part_to_sse(
    item: Result<LMResponsePart, String>,
    model: &str,
    usage_acc: UsageHandle,
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
                    Some("tool_calls")
                }
                LMResponsePart::Usage(u) => {
                    // 聚合供流后结算
                    if let Ok(mut acc) = usage_acc.try_lock() {
                        acc.merge(u);
                    }
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
                let chunk = serde_json::json!({
                    "id": "chatcmpl-llm-bridge",
                    "object": "chat.completion.chunk",
                    "created": 0,
                    "model": model,
                    "choices": [],
                    "usage": usage,
                });
                return Ok(Event::default().data(serde_json::to_string(&chunk).unwrap_or_default()));
            }

            let chunk = serde_json::json!({
                "id": "chatcmpl-llm-bridge",
                "object": "chat.completion.chunk",
                "created": 0,
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
            let error_chunk = serde_json::json!({
                "error": {
                    "message": e,
                    "type": "provider_error"
                }
            });
            Ok(Event::default().data(serde_json::to_string(&error_chunk).unwrap_or_default()))
        }
    }
}

#[allow(clippy::result_large_err)]
fn convert_messages(messages: &[OpenAiMessage]) -> Result<Vec<LanguageModelChatMessage>, Response> {
    messages
        .iter()
        .map(|msg| {
            let role = match msg.role.as_str() {
                "user" => LanguageModelChatMessageRole::User,
                "assistant" => LanguageModelChatMessageRole::Assistant,
                "system" => LanguageModelChatMessageRole::User, // map system → user for simplicity
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

            // 文本/多模态内容
            let text = content_to_text(&msg.content);
            if !text.is_empty() || parts.is_empty() {
                parts.insert(
                    0,
                    LanguageModelInputPart::Text(LanguageModelTextPart { value: text }),
                );
            }

            Ok(LanguageModelChatMessage {
                role,
                content: parts,
                name: msg.name.clone(),
            })
        })
        .collect()
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

/// Rough token count estimate for quota pre-check.
/// Uses character count / 4 as a rough heuristic (common for English text).
/// 工具定义 JSON 长度也计入，避免带 tools 时低估。
fn estimate_token_count(messages: &[OpenAiMessage], tools: Option<&[OpenAiTool]>) -> i64 {
    let message_chars: usize = messages
        .iter()
        .map(|m| match &m.content {
            OpenAiContent::String(s) => s.len(),
            OpenAiContent::Array(parts) => parts
                .iter()
                .filter_map(|p| match p {
                    OpenAiContentPart::Text { text } => Some(text.len()),
                    _ => None,
                })
                .sum(),
        })
        .sum();
    let tool_chars: usize = tools
        .map(|t| {
            t.iter()
                .map(|tool| {
                    tool.function.name.len()
                        + tool.function.description.as_deref().unwrap_or("").len()
                        + tool.function.parameters.to_string().len()
                })
                .sum()
        })
        .unwrap_or(0);
    ((message_chars + tool_chars) / 4) as i64
}
