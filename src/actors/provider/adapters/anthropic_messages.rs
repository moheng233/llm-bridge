use std::collections::HashMap;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::instrument;

use crate::types::{
    LMResponsePart, LanguageModelChatMessage, LanguageModelChatMessageRole, LanguageModelDataPart,
    LanguageModelInputPart, LanguageModelTextPart, LanguageModelThinkingPart,
    LanguageModelThinkingValue, LanguageModelTool, LanguageModelToolCallPart,
    LanguageModelToolResultContent, LanguageModelUsagePart,
};

use super::super::{ProviderChatRequest, ProviderResponseSender, ProviderState};

const DEFAULT_ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com/v1";
const ANTHROPIC_VERSION: &str = "2023-06-01";
// Anthropic requires max_tokens to be set explicitly; 4096 is a safe default.
const DEFAULT_MAX_TOKENS: u32 = 4096;

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
    let (system, messages) = build_messages(&request.messages)?;
    let payload = build_request_body(
        &request.model,
        system,
        messages,
        request.tools.as_deref(),
        request.tool_choice.as_ref(),
        request.temperature,
        request.max_tokens,
        request.top_p,
    )?;
    let endpoint = build_endpoint(state);

    let mut req_builder = state
        .client
        .post(&endpoint)
        .header("x-api-key", &state.api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json");

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
        .map_err(|error| format!("anthropic messages request failed: {error}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .map_err(|error| format!("anthropic error response read failed: {error}"))?;
        return Err(format!(
            "anthropic messages request failed (status {}): {}",
            status.as_u16(),
            parse_error_message(&body)
        ));
    }

    let mut stream_state = AnthropicStreamState::default();
    let mut decoder = SseDecoder::default();
    let mut body_stream = response.bytes_stream();

    while let Some(chunk) = body_stream.next().await {
        let chunk = chunk.map_err(|error| format!("anthropic stream read failed: {error}"))?;
        decoder.push(chunk.as_ref());

        while let Some(frame) = decoder.next_frame() {
            if let Some(data) = extract_sse_data(&frame) {
                let parts = map_event(&data, &mut stream_state)?;
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

#[allow(clippy::too_many_arguments)]
fn build_request_body(
    model: &str,
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    tools: Option<&[LanguageModelTool]>,
    tool_choice: Option<&Value>,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    top_p: Option<f64>,
) -> Result<Value, String> {
    let mut body = json!({
        "model": model,
        "messages": messages,
        "max_tokens": max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        "stream": true,
    });

    if let Some(system_prompt) = system {
        body["system"] = Value::String(system_prompt);
    }
    if let Some(temperature) = temperature {
        body["temperature"] = json!(temperature);
    }
    if let Some(top_p) = top_p {
        body["top_p"] = json!(top_p);
    }

    if let Some(tools) = tools
        && !tools.is_empty()
    {
        let anthropic_tools: Vec<Value> = tools
            .iter()
            .map(|tool| {
                let mut t = json!({
                    "name": tool.name,
                    "input_schema": tool.input_schema,
                });
                if let Some(desc) = &tool.description {
                    t["description"] = Value::String(desc.clone());
                }
                t
            })
            .collect();
        body["tools"] = Value::Array(anthropic_tools);

        // Anthropic tool_choice: {"type":"auto"} | {"type":"none"} | {"type":"tool","name":"..."}
        if let Some(choice) = tool_choice {
            body["tool_choice"] = map_tool_choice_to_anthropic(choice);
        }
    }

    Ok(body)
}

/// 将 OpenAI 格式的 tool_choice 映射为 Anthropic 格式。
fn map_tool_choice_to_anthropic(choice: &Value) -> Value {
    match choice {
        Value::String(s) => match s.as_str() {
            "none" => json!({"type": "none"}),
            "required" => json!({"type": "any"}),
            _ => json!({"type": "auto"}),
        },
        Value::Object(map) => {
            // OpenAI: {"type":"function","function":{"name":"..."}}
            if let Some(name) = map
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(Value::as_str)
            {
                json!({"type": "tool", "name": name})
            } else {
                json!({"type": "auto"})
            }
        }
        _ => json!({"type": "auto"}),
    }
}

/// Splits the message list into an optional system prompt and a list of Anthropic messages.
///
/// Anthropic does not allow a `system` role message inside the messages array; it must be
/// provided as a top-level `system` field. This function extracts any leading system content.
fn build_messages(
    messages: &[LanguageModelChatMessage],
) -> Result<(Option<String>, Vec<AnthropicMessage>), String> {
    let mut anthropic_messages: Vec<AnthropicMessage> = Vec::new();

    for message in messages {
        let role = match message.role {
            LanguageModelChatMessageRole::User => "user",
            LanguageModelChatMessageRole::Assistant => "assistant",
        };

        let content = build_content_blocks(&message.content)?;
        if content.is_empty() {
            continue;
        }

        // Anthropic requires alternating user/assistant turns. When we see consecutive messages
        // with the same role, merge their content blocks into the previous message.
        if let Some(last) = anthropic_messages.last_mut()
            && last.role == role
        {
            last.content.extend(content);
            continue;
        }

        anthropic_messages.push(AnthropicMessage {
            role: role.to_string(),
            content,
        });
    }

    Ok((None, anthropic_messages))
}

fn build_content_blocks(
    parts: &[LanguageModelInputPart],
) -> Result<Vec<AnthropicContentBlock>, String> {
    let mut blocks: Vec<AnthropicContentBlock> = Vec::new();

    for part in parts {
        match part {
            LanguageModelInputPart::Text(text) => {
                blocks.push(AnthropicContentBlock::Text(AnthropicTextBlock {
                    text: text.value.clone(),
                }));
            }
            LanguageModelInputPart::Data(data) => {
                blocks.push(build_image_block(data)?);
            }
            LanguageModelInputPart::ToolCall(tool_call) => {
                let input = tool_call.input.clone();
                blocks.push(AnthropicContentBlock::ToolUse(AnthropicToolUseBlock {
                    id: tool_call.call_id.clone(),
                    name: tool_call.name.clone(),
                    input,
                }));
            }
            LanguageModelInputPart::ToolResult(tool_result) => {
                let content = tool_result_to_content(&tool_result.content);
                blocks.push(AnthropicContentBlock::ToolResult(
                    AnthropicToolResultBlock {
                        tool_use_id: tool_result.call_id.clone(),
                        content,
                    },
                ));
            }
            LanguageModelInputPart::Thinking(thinking) => {
                // Anthropic does not accept thinking blocks as user input in the same way;
                // flatten the thinking value to a text block to preserve conversational context.
                let text = flatten_thinking_value(&thinking.value);
                if !text.is_empty() {
                    blocks.push(AnthropicContentBlock::Text(AnthropicTextBlock { text }));
                }
            }
        }
    }

    Ok(blocks)
}

fn build_image_block(part: &LanguageModelDataPart) -> Result<AnthropicContentBlock, String> {
    if !part.mime_type.starts_with("image/") {
        return Err(format!(
            "unsupported data mime type for anthropic: {}",
            part.mime_type
        ));
    }

    let data = BASE64_STANDARD.encode(&part.data);
    Ok(AnthropicContentBlock::Image(AnthropicImageBlock {
        source: AnthropicImageSource::Base64 {
            media_type: part.mime_type.clone(),
            data,
        },
    }))
}

fn tool_result_to_content(
    contents: &[LanguageModelToolResultContent],
) -> Vec<AnthropicToolResultContent> {
    contents
        .iter()
        .map(|content| match content {
            LanguageModelToolResultContent::Text(text) => AnthropicToolResultContent::Text {
                text: text.value.clone(),
            },
            LanguageModelToolResultContent::PromptTsx(tsx) => AnthropicToolResultContent::Text {
                text: tsx.value.to_string(),
            },
            LanguageModelToolResultContent::Data(data) => {
                if data.mime_type.starts_with("image/") {
                    let encoded = BASE64_STANDARD.encode(&data.data);
                    AnthropicToolResultContent::Image {
                        source: AnthropicImageSource::Base64 {
                            media_type: data.mime_type.clone(),
                            data: encoded,
                        },
                    }
                } else {
                    AnthropicToolResultContent::Text {
                        text: json!({ "mimeType": data.mime_type, "data": data.data }).to_string(),
                    }
                }
            }
            LanguageModelToolResultContent::Unknown(value) => AnthropicToolResultContent::Text {
                text: value.to_string(),
            },
        })
        .collect()
}

fn flatten_thinking_value(value: &LanguageModelThinkingValue) -> String {
    match value {
        LanguageModelThinkingValue::String(s) => s.clone(),
        LanguageModelThinkingValue::Array(parts) => parts.join("\n"),
    }
}

fn build_endpoint(state: &ProviderState) -> String {
    let base = resolve_base_url(state.base_url.as_deref());
    let path_suffix = state
        .compat_settings
        .as_ref()
        .and_then(|s| s.path_suffix.as_deref())
        .unwrap_or("");

    format!("{base}{path_suffix}/messages")
}

fn resolve_base_url(base_url: Option<&str>) -> String {
    base_url
        .unwrap_or(DEFAULT_ANTHROPIC_BASE_URL)
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
        #[serde(rename = "type")]
        error_type: Option<String>,
        message: Option<String>,
    }

    serde_json::from_str::<ErrorWrapper>(body)
        .ok()
        .and_then(|wrapper| wrapper.error)
        .map(|error| match (error.error_type, error.message) {
            (Some(t), Some(m)) => format!("{t}: {m}"),
            (_, Some(m)) => m,
            (Some(t), None) => t,
            (None, None) => body.to_string(),
        })
        .unwrap_or_else(|| body.to_string())
}

// ---------------------------------------------------------------------------
// SSE event dispatch
// ---------------------------------------------------------------------------

#[derive(Default)]
struct AnthropicStreamState {
    // Keyed by content block index; accumulates fragmented tool_use input JSON.
    tool_input_buffers: HashMap<u32, ToolUseBuffer>,
    // Tracks the current block type by index for fast routing of delta events.
    block_types: HashMap<u32, String>,
    // Input tokens from message_start, merged into the final Usage part.
    input_tokens: Option<u64>,
}

#[derive(Default)]
struct ToolUseBuffer {
    id: String,
    name: String,
    json_delta: String,
}

fn map_event(data: &str, state: &mut AnthropicStreamState) -> Result<Vec<LMResponsePart>, String> {
    let event: RawEvent = serde_json::from_str(data)
        .map_err(|error| format!("invalid anthropic event payload: {error}"))?;

    match event.event_type.as_str() {
        "content_block_start" => {
            let event: ContentBlockStartEvent = serde_json::from_value(event.payload)
                .map_err(|e| format!("invalid content_block_start: {e}"))?;
            on_content_block_start(event, state)
        }
        "content_block_delta" => {
            let event: ContentBlockDeltaEvent = serde_json::from_value(event.payload)
                .map_err(|e| format!("invalid content_block_delta: {e}"))?;
            on_content_block_delta(event, state)
        }
        "content_block_stop" => {
            let event: ContentBlockStopEvent = serde_json::from_value(event.payload)
                .map_err(|e| format!("invalid content_block_stop: {e}"))?;
            on_content_block_stop(event, state)
        }
        "message_start" => {
            let event: MessageStartEvent = serde_json::from_value(event.payload)
                .map_err(|e| format!("invalid message_start: {e}"))?;
            if let Some(usage) = event.message.usage {
                state.input_tokens = usage.input_tokens;
            }
            Ok(Vec::new())
        }
        "message_delta" => {
            let event: MessageDeltaEvent = serde_json::from_value(event.payload)
                .map_err(|e| format!("invalid message_delta: {e}"))?;

            let stop_reason = event.delta.stop_reason.clone();
            let output_tokens = event.usage.and_then(|u| u.output_tokens);

            if stop_reason.is_none() && output_tokens.is_none() {
                return Ok(Vec::new());
            }

            let finish_reason = stop_reason.as_deref().map(map_anthropic_stop_reason);
            let output = output_tokens.map(u64::from);

            Ok(vec![LMResponsePart::Usage(LanguageModelUsagePart {
                input_tokens: state.input_tokens,
                output_tokens: output,
                total_tokens: match (state.input_tokens, output) {
                    (Some(i), Some(o)) => Some(i + o),
                    _ => None,
                },
                reasoning_tokens: None,
                cached_tokens: None,
                finish_reason: finish_reason.map(str::to_string),
            })])
        }
        "error" => {
            let event: ErrorEvent = serde_json::from_value(event.payload)
                .map_err(|e| format!("invalid error event: {e}"))?;
            Err(format!(
                "{}: {}",
                event.error.error_type, event.error.message
            ))
        }
        // message_start and message_stop are lifecycle markers with no output parts.
        _ => Ok(Vec::new()),
    }
}

fn on_content_block_start(
    event: ContentBlockStartEvent,
    state: &mut AnthropicStreamState,
) -> Result<Vec<LMResponsePart>, String> {
    let block_type = match &event.content_block {
        ContentBlock::Text { .. } => "text",
        ContentBlock::ToolUse { id, name, .. } => {
            state.tool_input_buffers.insert(
                event.index,
                ToolUseBuffer {
                    id: id.clone(),
                    name: name.clone(),
                    json_delta: String::new(),
                },
            );
            "tool_use"
        }
        ContentBlock::Thinking { .. } => "thinking",
        ContentBlock::RedactedThinking { .. } => "redacted_thinking",
    };

    state
        .block_types
        .insert(event.index, block_type.to_string());
    Ok(Vec::new())
}

fn on_content_block_delta(
    event: ContentBlockDeltaEvent,
    state: &mut AnthropicStreamState,
) -> Result<Vec<LMResponsePart>, String> {
    match event.delta {
        ContentBlockDelta::TextDelta { text } => {
            Ok(vec![LMResponsePart::Text(LanguageModelTextPart {
                value: text,
            })])
        }
        ContentBlockDelta::InputJsonDelta { partial_json } => {
            if let Some(buf) = state.tool_input_buffers.get_mut(&event.index) {
                buf.json_delta.push_str(&partial_json);
            }
            Ok(Vec::new())
        }
        ContentBlockDelta::ThinkingDelta { thinking } => {
            let block_id = event.index.to_string();
            Ok(vec![LMResponsePart::Thinking(LanguageModelThinkingPart {
                value: LanguageModelThinkingValue::String(thinking),
                id: Some(block_id),
                metadata: None,
            })])
        }
        ContentBlockDelta::SignatureDelta { signature } => {
            // Anthropic thinking signature; attach to the next thinking emission via metadata.
            // For streaming we store it so content_block_stop can include it; ignore for now.
            let _ = signature;
            Ok(Vec::new())
        }
    }
}

fn on_content_block_stop(
    event: ContentBlockStopEvent,
    state: &mut AnthropicStreamState,
) -> Result<Vec<LMResponsePart>, String> {
    let block_type = state.block_types.remove(&event.index);

    if block_type.as_deref() == Some("tool_use")
        && let Some(buf) = state.tool_input_buffers.remove(&event.index)
    {
        let input = serde_json::from_str(&buf.json_delta).unwrap_or(Value::String(buf.json_delta));

        return Ok(vec![LMResponsePart::ToolCall(LanguageModelToolCallPart {
            call_id: buf.id,
            name: buf.name,
            input,
        })]);
    }

    Ok(Vec::new())
}

// ---------------------------------------------------------------------------
// SSE framing utilities (same algorithm as openai adapter)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct SseDecoder {
    buffer: Vec<u8>,
}

impl SseDecoder {
    fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    fn next_frame(&mut self) -> Option<String> {
        let frame_end = find_frame_boundary(&self.buffer)?;
        let delimiter_len = if self.buffer.get(frame_end..frame_end + 4) == Some(b"\r\n\r\n") {
            4
        } else {
            2
        };
        let frame = self.buffer.drain(..frame_end).collect::<Vec<_>>();
        self.buffer.drain(..delimiter_len);
        Some(String::from_utf8_lossy(&frame).into_owned())
    }
}

fn find_frame_boundary(buffer: &[u8]) -> Option<usize> {
    let mut index = 0;
    while index < buffer.len() {
        if buffer[index..].starts_with(b"\r\n\r\n") || buffer[index..].starts_with(b"\n\n") {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn extract_sse_data(frame: &str) -> Option<String> {
    let mut payload_lines: Vec<String> = Vec::new();

    for line in frame.lines() {
        if let Some(data) = line.strip_prefix("data:") {
            payload_lines.push(data.trim_start().to_string());
        }
    }

    if payload_lines.is_empty() {
        return None;
    }

    let payload = payload_lines.join("\n");
    if payload == "[DONE]" {
        return None;
    }

    Some(payload)
}

// ---------------------------------------------------------------------------
// Wire types: Anthropic request
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContentBlock>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContentBlock {
    Text(AnthropicTextBlock),
    Image(AnthropicImageBlock),
    ToolUse(AnthropicToolUseBlock),
    ToolResult(AnthropicToolResultBlock),
}

#[derive(Debug, Clone, Serialize)]
struct AnthropicTextBlock {
    text: String,
}

#[derive(Debug, Clone, Serialize)]
struct AnthropicImageBlock {
    source: AnthropicImageSource,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicImageSource {
    Base64 { media_type: String, data: String },
}

#[derive(Debug, Clone, Serialize)]
struct AnthropicToolUseBlock {
    id: String,
    name: String,
    input: Value,
}

#[derive(Debug, Clone, Serialize)]
struct AnthropicToolResultBlock {
    tool_use_id: String,
    content: Vec<AnthropicToolResultContent>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicToolResultContent {
    Text { text: String },
    Image { source: AnthropicImageSource },
}

// ---------------------------------------------------------------------------
// Wire types: Anthropic response / SSE events
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(flatten)]
    payload: Value,
}

#[derive(Debug, Deserialize)]
struct ContentBlockStartEvent {
    index: u32,
    content_block: ContentBlock,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    Thinking {
        thinking: String,
    },
    RedactedThinking {
        data: String,
    },
}

#[derive(Debug, Deserialize)]
struct ContentBlockDeltaEvent {
    index: u32,
    delta: ContentBlockDelta,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
enum ContentBlockDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
    ThinkingDelta { thinking: String },
    SignatureDelta { signature: String },
}

#[derive(Debug, Deserialize)]
struct ContentBlockStopEvent {
    index: u32,
}

/// 将 Anthropic stop_reason 映射为 OpenAI 风格 finish_reason。
fn map_anthropic_stop_reason(reason: &str) -> &'static str {
    match reason {
        "end_turn" | "stop_sequence" => "stop",
        "max_tokens" => "length",
        "tool_use" => "tool_calls",
        _ => "stop",
    }
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct MessageStartEvent {
    message: MessageStartMessage,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct MessageStartMessage {
    usage: Option<MessageStartUsage>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct MessageStartUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct MessageDeltaEvent {
    delta: MessageDelta,
    usage: Option<MessageDeltaUsage>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct MessageDelta {
    stop_reason: Option<String>,
    stop_sequence: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct MessageDeltaUsage {
    output_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ErrorEvent {
    error: AnthropicError,
}

#[derive(Debug, Deserialize)]
struct AnthropicError {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_text_delta_frame(index: u32, text: &str) -> String {
        serde_json::to_string(&json!({
            "type": "content_block_delta",
            "index": index,
            "delta": { "type": "text_delta", "text": text }
        }))
        .unwrap()
    }

    fn make_thinking_delta_frame(index: u32, thinking: &str) -> String {
        serde_json::to_string(&json!({
            "type": "content_block_delta",
            "index": index,
            "delta": { "type": "thinking_delta", "thinking": thinking }
        }))
        .unwrap()
    }

    fn make_tool_use_start(index: u32, id: &str, name: &str) -> String {
        serde_json::to_string(&json!({
            "type": "content_block_start",
            "index": index,
            "content_block": { "type": "tool_use", "id": id, "name": name, "input": {} }
        }))
        .unwrap()
    }

    fn make_input_json_delta(index: u32, partial: &str) -> String {
        serde_json::to_string(&json!({
            "type": "content_block_delta",
            "index": index,
            "delta": { "type": "input_json_delta", "partial_json": partial }
        }))
        .unwrap()
    }

    fn make_content_block_stop(index: u32) -> String {
        serde_json::to_string(&json!({
            "type": "content_block_stop",
            "index": index
        }))
        .unwrap()
    }

    fn make_error_event(error_type: &str, message: &str) -> String {
        serde_json::to_string(&json!({
            "type": "error",
            "error": { "type": error_type, "message": message }
        }))
        .unwrap()
    }

    #[test]
    fn text_delta_produces_text_part() {
        let mut state = AnthropicStreamState::default();
        let data = make_text_delta_frame(0, "hello");
        let parts = map_event(&data, &mut state).unwrap();
        assert_eq!(parts.len(), 1);
        if let LMResponsePart::Text(part) = &parts[0] {
            assert_eq!(part.value, "hello");
        } else {
            panic!("expected Text part");
        }
    }

    #[test]
    fn thinking_delta_produces_thinking_part() {
        let mut state = AnthropicStreamState::default();
        let data = make_thinking_delta_frame(0, "reasoning...");
        let parts = map_event(&data, &mut state).unwrap();
        assert_eq!(parts.len(), 1);
        if let LMResponsePart::Thinking(part) = &parts[0] {
            assert!(
                matches!(&part.value, LanguageModelThinkingValue::String(s) if s == "reasoning...")
            );
        } else {
            panic!("expected Thinking part");
        }
    }

    #[test]
    fn tool_use_lifecycle_produces_tool_call() {
        let mut state = AnthropicStreamState::default();

        let start = make_tool_use_start(0, "call_abc", "search");
        map_event(&start, &mut state).unwrap();

        let delta1 = make_input_json_delta(0, r#"{"q":"#);
        map_event(&delta1, &mut state).unwrap();

        let delta2 = make_input_json_delta(0, r#""rust"}"#);
        map_event(&delta2, &mut state).unwrap();

        let stop = make_content_block_stop(0);
        let parts = map_event(&stop, &mut state).unwrap();

        assert_eq!(parts.len(), 1);
        if let LMResponsePart::ToolCall(call) = &parts[0] {
            assert_eq!(call.call_id, "call_abc");
            assert_eq!(call.name, "search");
            assert_eq!(call.input, json!({"q": "rust"}));
        } else {
            panic!("expected ToolCall part");
        }
    }

    #[test]
    fn error_event_returns_err() {
        let mut state = AnthropicStreamState::default();
        let data = make_error_event("overloaded_error", "Service temporarily unavailable");
        let result = map_event(&data, &mut state);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("overloaded_error"));
        assert!(msg.contains("Service temporarily unavailable"));
    }

    #[test]
    fn sse_decoder_handles_lf_lf_boundary() {
        let mut decoder = SseDecoder::default();
        decoder.push(b"event: content_block_delta\ndata: {\"type\":\"test\"}\n\n");
        let frame = decoder.next_frame().unwrap();
        assert!(frame.contains("content_block_delta"));
    }

    #[test]
    fn sse_decoder_handles_crlf_crlf_boundary() {
        let mut decoder = SseDecoder::default();
        decoder.push(b"event: ping\ndata: {}\r\n\r\n");
        let frame = decoder.next_frame().unwrap();
        assert!(frame.contains("ping"));
    }

    #[test]
    fn extract_sse_data_skips_done() {
        let frame = "data: [DONE]";
        assert!(extract_sse_data(frame).is_none());
    }

    #[test]
    fn message_start_and_delta_produce_usage_with_finish_reason() {
        let mut state = AnthropicStreamState::default();

        let start = serde_json::to_string(&json!({
            "type": "message_start",
            "message": { "usage": { "input_tokens": 25, "output_tokens": 1 } }
        }))
        .unwrap();
        let parts = map_event(&start, &mut state).unwrap();
        assert!(parts.is_empty());
        assert_eq!(state.input_tokens, Some(25));

        let delta = serde_json::to_string(&json!({
            "type": "message_delta",
            "delta": { "stop_reason": "end_turn" },
            "usage": { "output_tokens": 7 }
        }))
        .unwrap();
        let parts = map_event(&delta, &mut state).unwrap();
        assert_eq!(parts.len(), 1);
        if let LMResponsePart::Usage(u) = &parts[0] {
            assert_eq!(u.input_tokens, Some(25));
            assert_eq!(u.output_tokens, Some(7));
            assert_eq!(u.total_tokens, Some(32));
            assert_eq!(u.finish_reason.as_deref(), Some("stop"));
        } else {
            panic!("expected Usage part");
        }
    }

    #[test]
    fn message_delta_maps_stop_reasons() {
        let cases = [
            ("end_turn", "stop"),
            ("stop_sequence", "stop"),
            ("max_tokens", "length"),
            ("tool_use", "tool_calls"),
        ];
        for (anthropic, expected) in cases {
            let mut state = AnthropicStreamState::default();
            let delta = serde_json::to_string(&json!({
                "type": "message_delta",
                "delta": { "stop_reason": anthropic },
                "usage": { "output_tokens": 1 }
            }))
            .unwrap();
            let parts = map_event(&delta, &mut state).unwrap();
            if let LMResponsePart::Usage(u) = &parts[0] {
                assert_eq!(
                    u.finish_reason.as_deref(),
                    Some(expected),
                    "for {anthropic}"
                );
            } else {
                panic!("expected Usage part for {anthropic}");
            }
        }
    }

    #[test]
    fn build_request_body_carries_sampling_params() {
        let body = build_request_body(
            "claude-test",
            None,
            vec![],
            None,
            None,
            Some(0.7),
            Some(123),
            Some(0.9),
        )
        .unwrap();
        assert_eq!(body["max_tokens"], 123);
        assert_eq!(body["temperature"], 0.7);
        assert_eq!(body["top_p"], 0.9);
    }

    #[test]
    fn build_request_body_defaults_max_tokens() {
        let body =
            build_request_body("claude-test", None, vec![], None, None, None, None, None).unwrap();
        assert_eq!(body["max_tokens"], DEFAULT_MAX_TOKENS);
        assert!(body.get("temperature").is_none());
        assert!(body.get("top_p").is_none());
    }
}
