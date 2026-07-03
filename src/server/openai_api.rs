use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response, sse::{Event, Sse}},
};
use futures_util::stream::Stream;
use ractor::Actor;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;
use tracing::instrument;

use crate::actors::provider::adapters::openai_chat_completions::flatten_thinking_value_for_sse;
use crate::actors::provider::{ProviderActor, ProviderChatRequest, ProviderMessage, ProviderRuntimeConfig};
use crate::middleware::token_auth::TokenAuth;
use crate::server::AppState;
use crate::types::{LMResponsePart, LanguageModelChatMessage, LanguageModelChatMessageRole, LanguageModelInputPart, LanguageModelTextPart};

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
        Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "unauthorized"}))).into_response())
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
    let all_models = state.store.list_available_models().await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": {
                        "message": e,
                        "type": "internal_error",
                        "code": "internal_error"
                    }
                })),
            ).into_response()
        })?;

    // Filter models based on token's allowed_models
    let allowed: Vec<String> =
        serde_json::from_str(&token.allowed_models).unwrap_or_default();

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
            let owned_by = m.providers
                .first()
                .map(|p| p.provider_id.clone())
                .unwrap_or_default();

            let providers = m.providers.into_iter().map(|p| OpenAiModelProviderInfo {
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
            }).collect();

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
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAiMessage {
    pub role: String,
    #[serde(default)]
    pub content: OpenAiContent,
    pub name: Option<String>,
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
    ImageUrl {
        image_url: OpenAiImageUrl,
    },
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
    let allowed: Vec<String> =
        serde_json::from_str(&token.allowed_models).unwrap_or_default();
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
        ).into_response());
    }

    let routes = state.store.resolve_model(&req.model).await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": {
                        "message": e,
                        "type": "internal_error",
                        "code": "internal_error"
                    }
                })),
            ).into_response()
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
        ).into_response());
    }

    // Take the first (highest priority) route.
    let route = &routes[0];

    // Phase 2: Quota check and deduct (before making upstream call)
    let estimated_tokens = estimate_token_count(&req.messages);
    if let Err(quota_err) = crate::auth::quota::check_and_deduct(
        &state.db,
        &token,
        estimated_tokens,
    )
    .await
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
        ).into_response());
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
        let sse_stream = stream_to_sse(stream, req.model.clone());

        // Spawn cleanup after stream is consumed.
        tokio::spawn(async move {
            cleanup_ref.stop(None);
            let _ = cleanup_handle.await;
        });

        Ok(Sse::new(sse_stream).into_response())
    } else {
        // Non-streaming: collect all chunks and concatenate.
        let mut stream = stream;
        let mut content = String::new();
        let mut reasoning_content = String::new();
        while let Some(item) = stream.next().await {
            match item {
                Ok(LMResponsePart::Text(t)) => content.push_str(&t.value),
                Ok(LMResponsePart::Thinking(t)) => {
                    let text = crate::actors::provider::adapters::openai_chat_completions::flatten_thinking_value_for_sse(&t.value);
                    reasoning_content.push_str(&text);
                }
                Ok(_) => {},
                Err(e) => {
                    cleanup_ref.stop(None);
                    let _ = cleanup_handle.await;
                    return Err(internal_error(&e));
                }
            }
        }
        cleanup_ref.stop(None);
        let _ = cleanup_handle.await;

        let mut message = serde_json::json!({
            "role": "assistant",
            "content": content,
        });
        if !reasoning_content.is_empty() {
            message["reasoning_content"] = serde_json::Value::String(reasoning_content);
        }

        Ok(Json(serde_json::json!({
            "id": "chatcmpl-llm-bridge",
            "object": "chat.completion",
            "created": 0,
            "model": req.model,
            "choices": [{
                "index": 0,
                "message": message,
                "finish_reason": "stop"
            }]
        })).into_response())
    }
}

fn stream_to_sse(
    stream: impl Stream<Item = Result<LMResponsePart, String>> + Send + 'static,
    model: String,
) -> impl Stream<Item = Result<Event, axum::Error>> + Send + 'static {
    stream.map(move |item| {
        match item {
            Ok(part) => {
                let mut delta = serde_json::Map::new();

                let finish_reason = match &part {
                    LMResponsePart::Text(t) => {
                        delta.insert("content".to_string(), serde_json::Value::String(t.value.clone()));
                        None
                    }
                    LMResponsePart::Thinking(t) => {
                        // Reasoning/thinking content — exposed as `reasoning_content` per DeepSeek / OpenAI extended format.
                        let text = flatten_thinking_value_for_sse(&t.value);
                        delta.insert("reasoning_content".to_string(), serde_json::Value::String(text));
                        None
                    }
                    LMResponsePart::ToolCall(_tc) => {
                        delta.insert("tool_calls".to_string(), serde_json::Value::Array(vec![]));
                        Some("tool_calls")
                    }
                    _ => None,
                };

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
    })
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

            let content = match &msg.content {
                OpenAiContent::String(s) => {
                    vec![LanguageModelInputPart::Text(LanguageModelTextPart {
                        value: s.clone(),
                    })]
                }
                OpenAiContent::Array(_parts) => {
                    // For array content, flatten to text for now.
                    let text = _parts
                        .iter()
                        .filter_map(|p| match p {
                            OpenAiContentPart::Text { text } => Some(text.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    vec![LanguageModelInputPart::Text(LanguageModelTextPart {
                        value: text,
                    })]
                }
            };

            Ok(LanguageModelChatMessage {
                role,
                content,
                name: msg.name.clone(),
            })
        })
        .collect()
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
    ).into_response()
}

/// Rough token count estimate for quota pre-check.
/// Uses character count / 4 as a rough heuristic (common for English text).
fn estimate_token_count(messages: &[OpenAiMessage]) -> i64 {
    let total_chars: usize = messages
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
    (total_chars / 4) as i64
}


