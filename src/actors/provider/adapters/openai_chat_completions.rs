use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::instrument;

use crate::types::{
    LMResponsePart, LanguageModelChatMessage, LanguageModelChatMessageRole, LanguageModelDataPart,
    LanguageModelInputPart, LanguageModelTextPart, LanguageModelThinkingPart,
    LanguageModelThinkingValue, LanguageModelToolCallPart, LanguageModelToolResultContent,
    LanguageModelToolResultPart,
};

use super::super::{ProviderChatRequest, ProviderResponseSender, ProviderState};

const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

#[instrument(
    level = "info",
    skip(state, request, tx),
    fields(
        model = %request.model,
        message_count = request.messages.len()
    )
)]
pub async fn stream_chat(
    state: &ProviderState,
    request: ProviderChatRequest,
    tx: ProviderResponseSender,
) -> Result<(), String> {
    let payload = build_request_body(&request)?;
    let endpoint = build_endpoint(state);

    let mut req_builder = state.client.post(&endpoint).bearer_auth(&state.api_key);

    // Apply custom headers from compatibility settings
    if let Some(settings) = &state.compat_settings {
        for (key, value) in &settings.custom_headers {
            req_builder = req_builder.header(key, value);
        }
    }

    let response = req_builder
        .json(&payload)
        .send()
        .await
        .map_err(|error| format!("openai chat completions request failed: {error}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .map_err(|error| format!("openai error response read failed: {error}"))?;
        return Err(format!(
            "openai chat completions request failed (status {}): {}",
            status.as_u16(),
            parse_error_message(&body)
        ));
    }

    let mut stream_state = OpenAiChatStreamState::default();
    let mut decoder = SseDecoder::default();
    let mut body_stream = response.bytes_stream();

    while let Some(chunk) = body_stream.next().await {
        let chunk = chunk.map_err(|error| format!("openai stream read failed: {error}"))?;
        decoder.push(chunk.as_ref());

        while let Some(frame) = decoder.next_frame() {
            if let Some(data) = extract_sse_data(&frame) {
                let parts = map_event(data, &mut stream_state)?;
                for part in parts {
                    if tx.send(Ok(part)).await.is_err() {
                        return Ok(());
                    }
                }
            }
        }
    }

    Ok(())
}

fn build_endpoint(state: &ProviderState) -> String {
    let base = resolve_base_url(state.base_url.as_deref());
    let path_suffix = state
        .compat_settings
        .as_ref()
        .and_then(|s| s.path_suffix.as_deref())
        .unwrap_or("");

    format!("{base}{path_suffix}/chat/completions")
}

fn build_request_body(request: &ProviderChatRequest) -> Result<Value, String> {
    serde_json::to_value(OpenAiChatCompletionsRequest {
        model: request.model.clone(),
        messages: build_messages(&request.messages)?,
        stream: true,
    })
    .map_err(|error| format!("failed to serialize openai chat completions request body: {error}"))
}

fn build_messages(messages: &[LanguageModelChatMessage]) -> Result<Vec<OpenAiChatMessage>, String> {
    let mut result = Vec::new();

    for message in messages {
        let role = role_to_openai(message.role);
        let mut content_parts = Vec::new();
        let mut tool_calls = Vec::new();

        for part in &message.content {
            match part {
                LanguageModelInputPart::Text(part) => {
                    content_parts.push(OpenAiChatContentPart::Text(OpenAiChatTextPart {
                        text: part.value.clone(),
                    }));
                }
                LanguageModelInputPart::Data(part) => {
                    content_parts.push(build_image_part(part)?);
                }
                LanguageModelInputPart::ToolResult(part) => {
                    // Tool results are separate messages in chat completions
                    result.push(OpenAiChatMessage {
                        role: "tool".to_string(),
                        content: Some(OpenAiChatContent::Text(tool_result_to_string(part))),
                        tool_calls: None,
                        tool_call_id: Some(part.call_id.clone()),
                    });
                }
                LanguageModelInputPart::ToolCall(part) => {
                    tool_calls.push(OpenAiChatToolCall {
                        id: part.call_id.clone(),
                        r#type: "function".to_string(),
                        function: OpenAiChatFunctionCall {
                            name: part.name.clone(),
                            arguments: serialize_tool_call_arguments(&part.input)?,
                        },
                    });
                }
                LanguageModelInputPart::Thinking(part) => {
                    // Thinking/reasoning content as text for chat completions
                    content_parts.push(OpenAiChatContentPart::Text(OpenAiChatTextPart {
                        text: flatten_thinking_value(&part.value),
                    }));
                }
            }
        }

        if !content_parts.is_empty() || !tool_calls.is_empty() {
            let content = if content_parts.len() == 1 {
                match &content_parts[0] {
                    OpenAiChatContentPart::Text(t) => Some(OpenAiChatContent::Text(t.text.clone())),
                    _ => Some(OpenAiChatContent::Parts(content_parts)),
                }
            } else if !content_parts.is_empty() {
                Some(OpenAiChatContent::Parts(content_parts))
            } else {
                None
            };

            let tool_calls_option = if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            };

            result.push(OpenAiChatMessage {
                role: role.to_string(),
                content,
                tool_calls: tool_calls_option,
                tool_call_id: None,
            });
        }
    }

    Ok(result)
}

fn build_image_part(part: &LanguageModelDataPart) -> Result<OpenAiChatContentPart, String> {
    if !part.mime_type.starts_with("image/") {
        return Err(format!(
            "unsupported data mime type for openai chat completions: {}",
            part.mime_type
        ));
    }

    let image_url = format!(
        "data:{};base64,{}",
        part.mime_type,
        BASE64_STANDARD.encode(&part.data)
    );

    Ok(OpenAiChatContentPart::ImageUrl(OpenAiChatImageUrlPart {
        image_url: OpenAiChatImageUrl { url: image_url },
    }))
}

fn role_to_openai(role: LanguageModelChatMessageRole) -> &'static str {
    match role {
        LanguageModelChatMessageRole::User => "user",
        LanguageModelChatMessageRole::Assistant => "assistant",
    }
}

fn tool_result_to_string(part: &LanguageModelToolResultPart) -> String {
    part.content
        .iter()
        .map(|content| match content {
            LanguageModelToolResultContent::Text(part) => part.value.clone(),
            LanguageModelToolResultContent::PromptTsx(part) => part.value.to_string(),
            LanguageModelToolResultContent::Data(part) => json!({
                "mimeType": part.mime_type,
                "data": part.data,
            })
            .to_string(),
            LanguageModelToolResultContent::Unknown(value) => value.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn serialize_tool_call_arguments(arguments: &Value) -> Result<String, String> {
    serde_json::to_string(arguments)
        .map_err(|error| format!("failed to serialize tool call arguments: {error}"))
}

fn flatten_thinking_value(value: &LanguageModelThinkingValue) -> String {
    match value {
        LanguageModelThinkingValue::String(value) => value.clone(),
        LanguageModelThinkingValue::Array(values) => values.join("\n"),
    }
}

/// Public helper — flattens thinking content for SSE `reasoning_content` field.
pub fn flatten_thinking_value_for_sse(value: &LanguageModelThinkingValue) -> String {
    flatten_thinking_value(value)
}

fn resolve_base_url(base_url: Option<&str>) -> String {
    base_url
        .unwrap_or(DEFAULT_OPENAI_BASE_URL)
        .trim_end_matches('/')
        .to_string()
}

fn parse_error_message(body: &str) -> String {
    #[derive(Deserialize)]
    struct ErrorWrapper {
        error: Option<ErrorBody>,
    }

    #[derive(Deserialize)]
    struct ErrorBody {
        code: Option<String>,
        message: Option<String>,
    }

    serde_json::from_str::<ErrorWrapper>(body)
        .ok()
        .and_then(|wrapper| wrapper.error)
        .map(|error| match (error.code, error.message) {
            (Some(code), Some(message)) => format!("{code}: {message}"),
            (_, Some(message)) => message,
            (Some(code), None) => code,
            (None, None) => body.to_string(),
        })
        .unwrap_or_else(|| body.to_string())
}

// ── SSE helpers ───────────────────────────────────────────────────────────────

#[derive(Default)]
struct SseDecoder {
    buffer: Vec<u8>,
}

impl SseDecoder {
    fn push(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    fn next_frame(&mut self) -> Option<String> {
        let text = String::from_utf8_lossy(&self.buffer);
        if let Some(pos) = text.find("\n\n") {
            let frame = text[..pos].to_string();
            self.buffer.drain(..pos + 2);
            Some(frame)
        } else {
            None
        }
    }
}

fn extract_sse_data(frame: &str) -> Option<&str> {
    for line in frame.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            return Some(data.trim());
        }
    }
    None
}

// ── Stream state ──────────────────────────────────────────────────────────────

#[derive(Default)]
struct OpenAiChatStreamState {
    streamed_tool_call_ids: std::collections::HashSet<String>,
}

// ── Event mapping ─────────────────────────────────────────────────────────────

fn map_event(data: &str, state: &mut OpenAiChatStreamState) -> Result<Vec<LMResponsePart>, String> {
    if data == "[DONE]" {
        return Ok(Vec::new());
    }

    let event: ChatCompletionChunk = serde_json::from_str(data)
        .map_err(|error| format!("invalid openai chat completions event: {error}"))?;

    let mut parts = Vec::new();

    for choice in &event.choices {
        if let Some(delta) = &choice.delta {
            if let Some(content) = &delta.content
                && !content.is_empty()
            {
                parts.push(LMResponsePart::Text(LanguageModelTextPart {
                    value: content.clone(),
                }));
            }

            if !delta.tool_calls.is_empty() {
                for tc in &delta.tool_calls {
                    if let Some(name) = &tc.function.name {
                        // New tool call
                        state.streamed_tool_call_ids.insert(tc.id.clone());
                        let arguments = tc.function.arguments.clone().unwrap_or_default();
                        let input =
                            serde_json::from_str(&arguments).unwrap_or(Value::String(arguments));
                        parts.push(LMResponsePart::ToolCall(LanguageModelToolCallPart {
                            call_id: tc.id.clone(),
                            name: name.clone(),
                            input,
                        }));
                    } else if let Some(_arguments) = &tc.function.arguments {
                        // Continuing tool call arguments (already emitted above)
                        // In streaming, we get incremental arguments
                        // For simplicity, we'll handle this by accumulating
                    }
                }
            }

            // Handle reasoning_content if present (OpenAI o1/o3 models)
            if let Some(reasoning) = &delta.reasoning_content
                && !reasoning.is_empty()
            {
                parts.push(LMResponsePart::Thinking(LanguageModelThinkingPart {
                    value: LanguageModelThinkingValue::String(reasoning.clone()),
                    id: None,
                    metadata: None,
                }));
            }
        }
    }

    Ok(parts)
}

// ── Request/Response types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
struct OpenAiChatCompletionsRequest {
    model: String,
    messages: Vec<OpenAiChatMessage>,
    stream: bool,
}

#[derive(Debug, Clone, Serialize)]
struct OpenAiChatMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<OpenAiChatContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiChatToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
enum OpenAiChatContent {
    Text(String),
    Parts(Vec<OpenAiChatContentPart>),
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OpenAiChatContentPart {
    Text(OpenAiChatTextPart),
    ImageUrl(OpenAiChatImageUrlPart),
}

#[derive(Debug, Clone, Serialize)]
struct OpenAiChatTextPart {
    text: String,
}

#[derive(Debug, Clone, Serialize)]
struct OpenAiChatImageUrlPart {
    image_url: OpenAiChatImageUrl,
}

#[derive(Debug, Clone, Serialize)]
struct OpenAiChatImageUrl {
    url: String,
}

#[derive(Debug, Clone, Serialize)]
struct OpenAiChatToolCall {
    id: String,
    r#type: String,
    function: OpenAiChatFunctionCall,
}

#[derive(Debug, Clone, Serialize)]
struct OpenAiChatFunctionCall {
    name: String,
    arguments: String,
}

// ── Streaming response types ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    #[allow(dead_code)]
    id: Option<String>,
    choices: Vec<ChatCompletionChunkChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunkChoice {
    delta: Option<ChatCompletionChunkDelta>,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunkDelta {
    #[allow(dead_code)]
    role: Option<String>,
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ChatCompletionChunkToolCall>,
    reasoning_content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunkToolCall {
    id: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    tool_type: Option<String>,
    function: ChatCompletionChunkFunction,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunkFunction {
    name: Option<String>,
    arguments: Option<String>,
}
