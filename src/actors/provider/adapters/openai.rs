use std::collections::{HashMap, HashSet};

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
    let endpoint = format!("{}/responses", resolve_base_url(state.base_url.as_deref()));
    let response = state
        .client
        .post(endpoint)
        .bearer_auth(&state.api_key)
        .json(&payload)
        .send()
        .await
        .map_err(|error| format!("openai request failed: {error}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .map_err(|error| format!("openai error response read failed: {error}"))?;
        return Err(format!(
            "openai responses request failed (status {}): {}",
            status.as_u16(),
            parse_error_message(&body)
        ));
    }

    let mut stream_state = OpenAiStreamState::default();
    let mut decoder = SseDecoder::default();
    let mut body_stream = response.bytes_stream();

    while let Some(chunk) = body_stream.next().await {
        let chunk = chunk.map_err(|error| format!("openai stream read failed: {error}"))?;
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

fn build_request_body(request: &ProviderChatRequest) -> Result<Value, String> {
    serde_json::to_value(OpenAiResponsesCreateRequest {
        model: request.model.clone(),
        input: build_input_items(&request.messages)?,
        stream: true,
    })
    .map_err(|error| format!("failed to serialize openai request body: {error}"))
}

fn build_input_items(
    messages: &[LanguageModelChatMessage],
) -> Result<Vec<OpenAiInputItem>, String> {
    let mut items = Vec::new();

    for message in messages {
        let mut content = Vec::new();

        for part in &message.content {
            match part {
                LanguageModelInputPart::Text(part) => {
                    content.push(OpenAiInputContent::InputText(OpenAiInputText {
                        text: part.value.clone(),
                    }));
                }
                LanguageModelInputPart::Data(part) => {
                    content.push(build_image_input(part)?);
                }
                LanguageModelInputPart::ToolResult(part) => {
                    if !content.is_empty() {
                        items.push(OpenAiInputItem::Message(OpenAiInputMessage {
                            role: role_to_openai(message.role).to_string(),
                            content,
                        }));
                        content = Vec::new();
                    }

                    items.push(OpenAiInputItem::FunctionCallOutput(
                        OpenAiFunctionCallOutput::new(
                            part.call_id.clone(),
                            tool_result_to_output(part),
                        ),
                    ));
                }
                LanguageModelInputPart::ToolCall(part) => {
                    if !content.is_empty() {
                        items.push(OpenAiInputItem::Message(OpenAiInputMessage {
                            role: role_to_openai(message.role).to_string(),
                            content,
                        }));
                        content = Vec::new();
                    }

                    items.push(OpenAiInputItem::FunctionCall(OpenAiFunctionCall::new(
                        part.call_id.clone(),
                        part.name.clone(),
                        serialize_tool_call_arguments(&part.input)?,
                    )));
                }
                LanguageModelInputPart::Thinking(part) => {
                    if let Some(reasoning_item) = build_reasoning_item(part) {
                        if !content.is_empty() {
                            items.push(OpenAiInputItem::Message(OpenAiInputMessage {
                                role: role_to_openai(message.role).to_string(),
                                content,
                            }));
                            content = Vec::new();
                        }

                        items.push(OpenAiInputItem::Reasoning(reasoning_item));
                    } else {
                        content.push(OpenAiInputContent::InputText(OpenAiInputText {
                            text: flatten_thinking_value(&part.value),
                        }));
                    }
                }
            }
        }

        if !content.is_empty() {
            items.push(OpenAiInputItem::Message(OpenAiInputMessage {
                role: role_to_openai(message.role).to_string(),
                content,
            }));
        }
    }

    Ok(items)
}

fn build_image_input(part: &LanguageModelDataPart) -> Result<OpenAiInputContent, String> {
    if !part.mime_type.starts_with("image/") {
        return Err(format!(
            "unsupported data mime type for openai responses: {}",
            part.mime_type
        ));
    }

    let image_url = format!(
        "data:{};base64,{}",
        part.mime_type,
        BASE64_STANDARD.encode(&part.data)
    );

    Ok(OpenAiInputContent::InputImage(OpenAiInputImage {
        image_url,
        detail: None,
        file_id: None,
    }))
}

fn role_to_openai(role: LanguageModelChatMessageRole) -> &'static str {
    match role {
        LanguageModelChatMessageRole::User => "user",
        LanguageModelChatMessageRole::Assistant => "assistant",
    }
}

fn tool_result_to_output(part: &LanguageModelToolResultPart) -> String {
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

fn build_reasoning_item(part: &LanguageModelThinkingPart) -> Option<OpenAiReasoningItem> {
    let metadata = part.metadata.as_ref()?;
    let encrypted_content = metadata
        .get("encrypted_content")
        .or_else(|| metadata.get("encryptedContent"))?
        .as_str()?
        .to_string();

    Some(OpenAiReasoningItem::new(part.id.clone(), encrypted_content))
}

fn flatten_thinking_value(value: &LanguageModelThinkingValue) -> String {
    match value {
        LanguageModelThinkingValue::String(value) => value.clone(),
        LanguageModelThinkingValue::Array(values) => values.join("\n"),
    }
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

#[derive(Default)]
struct OpenAiStreamState {
    function_arguments: HashMap<String, String>,
    emitted_tool_calls: HashSet<String>,
    streamed_message_parts: HashSet<String>,
    emitted_message_parts: HashSet<String>,
    streamed_reasoning_items: HashSet<String>,
    emitted_reasoning_items: HashSet<String>,
}

fn map_event(data: &str, state: &mut OpenAiStreamState) -> Result<Vec<LMResponsePart>, String> {
    let event: RawEvent = serde_json::from_str(data)
        .map_err(|error| format!("invalid openai event payload: {error}"))?;

    match event.event_type.as_str() {
        "response.output_text.delta" => {
            let event: OutputTextDeltaEvent = serde_json::from_value(event.payload)
                .map_err(|error| format!("invalid output_text.delta event: {error}"))?;
            mark_message_part_streamed(state, event.item_id.as_deref(), event.content_index);
            Ok(vec![LMResponsePart::Text(LanguageModelTextPart {
                value: event.delta,
            })])
        }
        "response.output_text.done" => {
            let event: OutputTextDoneEvent = serde_json::from_value(event.payload)
                .map_err(|error| format!("invalid output_text.done event: {error}"))?;
            Ok(
                emit_output_text_if_needed(&event.item_id, event.content_index, event.text, state)
                    .into_iter()
                    .collect(),
            )
        }
        "response.refusal.delta" => {
            let event: RefusalDeltaEvent = serde_json::from_value(event.payload)
                .map_err(|error| format!("invalid refusal.delta event: {error}"))?;
            mark_message_part_streamed(state, event.item_id.as_deref(), event.content_index);
            Ok(vec![LMResponsePart::Text(LanguageModelTextPart {
                value: event.delta,
            })])
        }
        "response.refusal.done" => {
            let event: RefusalDoneEvent = serde_json::from_value(event.payload)
                .map_err(|error| format!("invalid refusal.done event: {error}"))?;
            Ok(
                emit_refusal_if_needed(&event.item_id, event.content_index, event.refusal, state)
                    .into_iter()
                    .collect(),
            )
        }
        "response.reasoning_text.delta" => {
            let event: ReasoningTextDeltaEvent = serde_json::from_value(event.payload)
                .map_err(|error| format!("invalid reasoning_text.delta event: {error}"))?;
            state.streamed_reasoning_items.insert(event.item_id.clone());
            Ok(vec![LMResponsePart::Thinking(LanguageModelThinkingPart {
                value: LanguageModelThinkingValue::String(event.delta),
                id: Some(event.item_id),
                metadata: None,
            })])
        }
        "response.reasoning_text.done" => {
            let _: ReasoningTextDoneEvent = serde_json::from_value(event.payload)
                .map_err(|error| format!("invalid reasoning_text.done event: {error}"))?;
            Ok(Vec::new())
        }
        "response.reasoning_summary_text.delta" => {
            let event: ReasoningSummaryTextDeltaEvent = serde_json::from_value(event.payload)
                .map_err(|error| format!("invalid reasoning_summary_text.delta event: {error}"))?;
            state.streamed_reasoning_items.insert(event.item_id.clone());
            Ok(vec![LMResponsePart::Thinking(LanguageModelThinkingPart {
                value: LanguageModelThinkingValue::String(event.delta),
                id: Some(event.item_id),
                metadata: None,
            })])
        }
        "response.reasoning_summary_text.done" => {
            let _: ReasoningSummaryTextDoneEvent = serde_json::from_value(event.payload)
                .map_err(|error| format!("invalid reasoning_summary_text.done event: {error}"))?;
            Ok(Vec::new())
        }
        "response.function_call_arguments.delta" => {
            let event: FunctionCallArgumentsDeltaEvent = serde_json::from_value(event.payload)
                .map_err(|error| format!("invalid function_call_arguments.delta event: {error}"))?;
            state
                .function_arguments
                .entry(event.item_id)
                .and_modify(|value| value.push_str(&event.delta))
                .or_insert(event.delta);
            Ok(Vec::new())
        }
        "response.function_call_arguments.done" => {
            let event: FunctionCallArgumentsDoneEvent = serde_json::from_value(event.payload)
                .map_err(|error| format!("invalid function_call_arguments.done event: {error}"))?;
            let fallback_arguments = state
                .function_arguments
                .remove(&event.item_id)
                .unwrap_or_default();
            let arguments = if event.arguments.is_empty() {
                fallback_arguments
            } else {
                event.arguments
            };
            let input = serde_json::from_str(&arguments).unwrap_or(Value::String(arguments));
            state.emitted_tool_calls.insert(event.item_id.clone());

            Ok(vec![LMResponsePart::ToolCall(LanguageModelToolCallPart {
                call_id: event.item_id,
                name: event.name,
                input,
            })])
        }
        "response.output_item.added" => {
            let event: ResponseOutputItemAddedEvent = serde_json::from_value(event.payload)
                .map_err(|error| format!("invalid output_item.added event: {error}"))?;
            map_output_item(&event.item, state)
        }
        "response.output_item.done" => {
            let event: ResponseOutputItemDoneEvent = serde_json::from_value(event.payload)
                .map_err(|error| format!("invalid output_item.done event: {error}"))?;
            map_output_item(&event.item, state)
        }
        "response.content_part.added" => {
            let _: ResponseContentPartAddedEvent = serde_json::from_value(event.payload)
                .map_err(|error| format!("invalid content_part.added event: {error}"))?;
            Ok(Vec::new())
        }
        "response.content_part.done" => {
            let event: ResponseContentPartDoneEvent = serde_json::from_value(event.payload)
                .map_err(|error| format!("invalid content_part.done event: {error}"))?;
            map_message_content_part(&event.item_id, event.content_index, &event.part, state)
        }
        "response.failed" => {
            let event: ResponseFailedEvent = serde_json::from_value(event.payload)
                .map_err(|error| format!("invalid response.failed event: {error}"))?;
            let message = event
                .response
                .error
                .and_then(|error| error.message)
                .unwrap_or_else(|| "openai response failed".to_string());
            Err(message)
        }
        "response.incomplete" => {
            let event: ResponseIncompleteEvent = serde_json::from_value(event.payload)
                .map_err(|error| format!("invalid response.incomplete event: {error}"))?;
            let reason = event
                .response
                .incomplete_details
                .and_then(|details| details.reason)
                .unwrap_or_else(|| "unknown".to_string());
            Err(format!("openai response incomplete: {reason}"))
        }
        "error" => {
            let event: ErrorEvent = serde_json::from_value(event.payload)
                .map_err(|error| format!("invalid error event: {error}"))?;
            let mut message = event.message;
            if let Some(code) = event.code {
                message = format!("{code}: {message}");
            }
            Err(message)
        }
        _ => Ok(Vec::new()),
    }
}

fn map_output_item(
    item: &OpenAiResponseOutputItem,
    state: &mut OpenAiStreamState,
) -> Result<Vec<LMResponsePart>, String> {
    match item.item_type.as_deref() {
        Some("message") => map_message_output_item(item, state),
        Some("function_call") => map_function_call_output_item(item, state),
        Some("reasoning") => map_reasoning_output_item(item, state),
        _ => Ok(Vec::new()),
    }
}

fn map_message_output_item(
    item: &OpenAiResponseOutputItem,
    state: &mut OpenAiStreamState,
) -> Result<Vec<LMResponsePart>, String> {
    let item_id = match &item.id {
        Some(item_id) => item_id,
        None => return Ok(Vec::new()),
    };

    let mut parts = Vec::new();
    for (content_index, part) in item.content.iter().enumerate() {
        parts.extend(map_message_content_part(
            item_id,
            content_index as u32,
            part,
            state,
        )?);
    }

    Ok(parts)
}

fn map_function_call_output_item(
    item: &OpenAiResponseOutputItem,
    state: &mut OpenAiStreamState,
) -> Result<Vec<LMResponsePart>, String> {
    let call_id = item
        .call_id
        .clone()
        .or_else(|| item.id.clone())
        .ok_or_else(|| "function_call output item is missing id".to_string())?;

    if state.emitted_tool_calls.contains(&call_id) {
        return Ok(Vec::new());
    }

    let name = match &item.name {
        Some(name) => name.clone(),
        None => return Ok(Vec::new()),
    };

    let arguments = item
        .arguments
        .clone()
        .or_else(|| state.function_arguments.remove(&call_id))
        .unwrap_or_default();

    if arguments.is_empty() {
        return Ok(Vec::new());
    }

    let input = serde_json::from_str(&arguments).unwrap_or(Value::String(arguments));
    state.emitted_tool_calls.insert(call_id.clone());

    Ok(vec![LMResponsePart::ToolCall(LanguageModelToolCallPart {
        call_id,
        name,
        input,
    })])
}

fn map_reasoning_output_item(
    item: &OpenAiResponseOutputItem,
    state: &mut OpenAiStreamState,
) -> Result<Vec<LMResponsePart>, String> {
    let item_id = match &item.id {
        Some(item_id) => item_id.clone(),
        None => return Ok(Vec::new()),
    };

    let summary_texts = item
        .summary
        .iter()
        .map(|part| part.text.clone())
        .collect::<Vec<_>>();
    let metadata = item
        .encrypted_content
        .as_ref()
        .map(|encrypted_content| json!({ "encrypted_content": encrypted_content }));
    let has_streamed_text = state.streamed_reasoning_items.contains(&item_id);

    if summary_texts.is_empty() && metadata.is_none() {
        return Ok(Vec::new());
    }

    if has_streamed_text && metadata.is_none() {
        return Ok(Vec::new());
    }

    if !state.emitted_reasoning_items.insert(item_id.clone()) && !has_streamed_text {
        return Ok(Vec::new());
    }

    let value = match summary_texts.as_slice() {
        [] => LanguageModelThinkingValue::String(String::new()),
        [single] => LanguageModelThinkingValue::String(single.clone()),
        _ => LanguageModelThinkingValue::Array(summary_texts),
    };

    Ok(vec![LMResponsePart::Thinking(LanguageModelThinkingPart {
        value,
        id: Some(item_id),
        metadata,
    })])
}

fn map_message_content_part(
    item_id: &str,
    content_index: u32,
    part: &OpenAiResponseContentPart,
    state: &mut OpenAiStreamState,
) -> Result<Vec<LMResponsePart>, String> {
    let mapped = match part {
        OpenAiResponseContentPart::OutputText(part) => {
            emit_output_text_if_needed(item_id, content_index, part.text.clone(), state)
        }
        OpenAiResponseContentPart::Refusal(part) => {
            emit_refusal_if_needed(item_id, content_index, part.refusal.clone(), state)
        }
        OpenAiResponseContentPart::SummaryText(_) => None,
    };

    Ok(mapped.into_iter().collect())
}

fn emit_output_text_if_needed(
    item_id: &str,
    content_index: u32,
    text: String,
    state: &mut OpenAiStreamState,
) -> Option<LMResponsePart> {
    if text.is_empty() || !mark_message_part_emitted(state, item_id, content_index) {
        return None;
    }

    Some(LMResponsePart::Text(LanguageModelTextPart { value: text }))
}

fn emit_refusal_if_needed(
    item_id: &str,
    content_index: u32,
    refusal: String,
    state: &mut OpenAiStreamState,
) -> Option<LMResponsePart> {
    if refusal.is_empty() || !mark_message_part_emitted(state, item_id, content_index) {
        return None;
    }

    Some(LMResponsePart::Text(LanguageModelTextPart {
        value: refusal,
    }))
}

fn mark_message_part_streamed(
    state: &mut OpenAiStreamState,
    item_id: Option<&str>,
    content_index: Option<u32>,
) {
    let Some(key) = message_part_key(item_id, content_index) else {
        return;
    };

    state.streamed_message_parts.insert(key);
}

fn mark_message_part_emitted(
    state: &mut OpenAiStreamState,
    item_id: &str,
    content_index: u32,
) -> bool {
    let key = format!("{item_id}:{content_index}");
    if state.streamed_message_parts.contains(&key) {
        return false;
    }

    state.emitted_message_parts.insert(key)
}

fn message_part_key(item_id: Option<&str>, content_index: Option<u32>) -> Option<String> {
    Some(format!("{}:{}", item_id?, content_index?))
}

#[derive(Debug, Clone, Serialize)]
struct OpenAiResponsesCreateRequest {
    model: String,
    input: Vec<OpenAiInputItem>,
    stream: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
enum OpenAiInputItem {
    Message(OpenAiInputMessage),
    FunctionCall(OpenAiFunctionCall),
    FunctionCallOutput(OpenAiFunctionCallOutput),
    Reasoning(OpenAiReasoningItem),
}

#[derive(Debug, Clone, Serialize)]
struct OpenAiInputMessage {
    role: String,
    content: Vec<OpenAiInputContent>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OpenAiInputContent {
    InputText(OpenAiInputText),
    InputImage(OpenAiInputImage),
}

#[derive(Debug, Clone, Serialize)]
struct OpenAiInputText {
    text: String,
}

#[derive(Debug, Clone, Serialize)]
struct OpenAiInputImage {
    image_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct OpenAiFunctionCallOutput {
    #[serde(rename = "type")]
    item_type: &'static str,
    call_id: String,
    output: String,
}

impl OpenAiFunctionCallOutput {
    fn new(call_id: String, output: String) -> Self {
        Self {
            item_type: "function_call_output",
            call_id,
            output,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct OpenAiFunctionCall {
    #[serde(rename = "type")]
    item_type: &'static str,
    call_id: String,
    name: String,
    arguments: String,
}

impl OpenAiFunctionCall {
    fn new(call_id: String, name: String, arguments: String) -> Self {
        Self {
            item_type: "function_call",
            call_id,
            name,
            arguments,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct OpenAiReasoningItem {
    #[serde(rename = "type")]
    item_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    encrypted_content: String,
}

impl OpenAiReasoningItem {
    fn new(id: Option<String>, encrypted_content: String) -> Self {
        Self {
            item_type: "reasoning",
            id,
            encrypted_content,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(flatten)]
    payload: Value,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OpenAiResponseStatus {
    Completed,
    Failed,
    InProgress,
    Cancelled,
    Queued,
    Incomplete,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct OpenAiResponse {
    id: Option<String>,
    object: Option<String>,
    created_at: Option<u64>,
    status: Option<OpenAiResponseStatus>,
    error: Option<ResponseError>,
    incomplete_details: Option<IncompleteDetails>,
    model: Option<String>,
    #[serde(default)]
    output: Vec<OpenAiResponseOutputItem>,
    usage: Option<OpenAiResponseUsage>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct OpenAiResponseUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
    input_tokens_details: Option<OpenAiInputTokenDetails>,
    output_tokens_details: Option<OpenAiOutputTokenDetails>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct OpenAiInputTokenDetails {
    cached_tokens: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct OpenAiOutputTokenDetails {
    reasoning_tokens: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct OpenAiResponseOutputItem {
    id: Option<String>,
    #[serde(rename = "type")]
    item_type: Option<String>,
    status: Option<OpenAiResponseStatus>,
    role: Option<String>,
    call_id: Option<String>,
    name: Option<String>,
    arguments: Option<String>,
    encrypted_content: Option<String>,
    #[serde(default)]
    summary: Vec<OpenAiSummaryText>,
    #[serde(default)]
    content: Vec<OpenAiResponseContentPart>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct ResponseOutputItemAddedEvent {
    item: OpenAiResponseOutputItem,
    output_index: u32,
    sequence_number: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct ResponseOutputItemDoneEvent {
    item: OpenAiResponseOutputItem,
    output_index: u32,
    sequence_number: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OpenAiResponseContentPart {
    OutputText(OpenAiOutputText),
    Refusal(OpenAiOutputRefusal),
    SummaryText(OpenAiSummaryText),
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct ResponseContentPartAddedEvent {
    item_id: String,
    output_index: u32,
    content_index: u32,
    part: OpenAiResponseContentPart,
    sequence_number: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct ResponseContentPartDoneEvent {
    item_id: String,
    output_index: u32,
    content_index: u32,
    part: OpenAiResponseContentPart,
    sequence_number: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct OpenAiOutputText {
    text: String,
    #[serde(default)]
    annotations: Vec<Value>,
    #[serde(default)]
    logprobs: Vec<Value>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct OpenAiOutputRefusal {
    refusal: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct OpenAiSummaryText {
    text: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct OutputTextDeltaEvent {
    item_id: Option<String>,
    output_index: Option<u32>,
    content_index: Option<u32>,
    delta: String,
    sequence_number: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct OutputTextDoneEvent {
    item_id: String,
    output_index: u32,
    content_index: u32,
    text: String,
    sequence_number: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct RefusalDeltaEvent {
    item_id: Option<String>,
    output_index: Option<u32>,
    content_index: Option<u32>,
    delta: String,
    sequence_number: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct RefusalDoneEvent {
    item_id: String,
    output_index: u32,
    content_index: u32,
    refusal: String,
    sequence_number: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ReasoningTextDeltaEvent {
    item_id: String,
    output_index: Option<u32>,
    content_index: Option<u32>,
    delta: String,
    sequence_number: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ReasoningTextDoneEvent {
    item_id: String,
    output_index: u32,
    content_index: u32,
    text: String,
    sequence_number: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ReasoningSummaryTextDeltaEvent {
    item_id: String,
    output_index: Option<u32>,
    summary_index: Option<u32>,
    delta: String,
    sequence_number: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ReasoningSummaryTextDoneEvent {
    item_id: String,
    output_index: u32,
    summary_index: u32,
    text: String,
    sequence_number: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct FunctionCallArgumentsDeltaEvent {
    item_id: String,
    output_index: Option<u32>,
    delta: String,
    sequence_number: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct FunctionCallArgumentsDoneEvent {
    item_id: String,
    name: String,
    output_index: Option<u32>,
    arguments: String,
    sequence_number: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ResponseFailedEvent {
    response: OpenAiResponse,
    sequence_number: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct ResponseError {
    code: Option<String>,
    message: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ResponseIncompleteEvent {
    response: OpenAiResponse,
    sequence_number: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct IncompleteDetails {
    reason: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ErrorEvent {
    code: Option<String>,
    message: String,
    param: Option<String>,
    sequence_number: Option<u64>,
}

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
    let mut payload_lines = Vec::new();

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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::types::{LanguageModelInputPart, LanguageModelToolResultContent};

    #[test]
    fn builds_openai_request_body_from_messages() {
        let request = ProviderChatRequest {
            model: "gpt-5.4".to_string(),
            messages: vec![
                LanguageModelChatMessage::user(
                    vec![LanguageModelInputPart::Text(LanguageModelTextPart {
                        value: "hello".to_string(),
                    })],
                    None,
                ),
                LanguageModelChatMessage::assistant(
                    vec![LanguageModelInputPart::ToolResult(
                        LanguageModelToolResultPart {
                            call_id: "call_1".to_string(),
                            content: vec![
                                LanguageModelToolResultContent::Text(LanguageModelTextPart {
                                    value: "42".to_string(),
                                }),
                                LanguageModelToolResultContent::Unknown(json!({"ok": true})),
                            ],
                        },
                    )],
                    None,
                ),
            ],
        };

        let payload = build_request_body(&request).expect("request body should build");
        assert_eq!(payload["model"], "gpt-5.4");
        assert_eq!(payload["stream"], true);
        assert_eq!(payload["input"][0]["role"], "user");
        assert_eq!(payload["input"][0]["content"][0]["text"], "hello");
        assert_eq!(payload["input"][1]["type"], "function_call_output");
        assert_eq!(payload["input"][1]["call_id"], "call_1");
        assert_eq!(payload["input"][1]["output"], "42\n{\"ok\":true}");
    }

    #[test]
    fn supports_tool_call_and_reasoning_inputs() {
        let request = ProviderChatRequest {
            model: "gpt-5.4".to_string(),
            messages: vec![LanguageModelChatMessage::assistant(
                vec![
                    LanguageModelInputPart::ToolCall(LanguageModelToolCallPart {
                        call_id: "call_1".to_string(),
                        name: "get_weather".to_string(),
                        input: json!({"city": "Paris"}),
                    }),
                    LanguageModelInputPart::Thinking(LanguageModelThinkingPart {
                        value: LanguageModelThinkingValue::String(
                            "hidden chain of thought".to_string(),
                        ),
                        id: Some("rs_1".to_string()),
                        metadata: Some(json!({"encrypted_content": "enc_123"})),
                    }),
                ],
                None,
            )],
        };

        let payload = build_request_body(&request).expect("request body should build");
        assert_eq!(payload["input"][0]["type"], "function_call");
        assert_eq!(payload["input"][0]["call_id"], "call_1");
        assert_eq!(payload["input"][0]["name"], "get_weather");
        assert_eq!(payload["input"][0]["arguments"], "{\"city\":\"Paris\"}");
        assert_eq!(payload["input"][1]["type"], "reasoning");
        assert_eq!(payload["input"][1]["id"], "rs_1");
        assert_eq!(payload["input"][1]["encrypted_content"], "enc_123");
    }

    #[test]
    fn falls_back_to_text_when_thinking_has_no_encrypted_content() {
        let request = ProviderChatRequest {
            model: "gpt-5.4".to_string(),
            messages: vec![LanguageModelChatMessage::assistant(
                vec![LanguageModelInputPart::Thinking(
                    LanguageModelThinkingPart {
                        value: LanguageModelThinkingValue::Array(vec![
                            "first".to_string(),
                            "second".to_string(),
                        ]),
                        id: None,
                        metadata: None,
                    },
                )],
                None,
            )],
        };

        let payload = build_request_body(&request).expect("request body should build");
        assert_eq!(payload["input"][0]["role"], "assistant");
        assert_eq!(payload["input"][0]["content"][0]["type"], "input_text");
        assert_eq!(payload["input"][0]["content"][0]["text"], "first\nsecond");
    }

    #[test]
    fn sse_decoder_supports_lf_and_crlf_boundaries() {
        let mut decoder = SseDecoder::default();
        decoder.push(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"A\"}\n\n");
        decoder.push(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"B\"}\r\n\r\n");

        let first = extract_sse_data(&decoder.next_frame().expect("first frame"));
        let second = extract_sse_data(&decoder.next_frame().expect("second frame"));

        assert_eq!(
            first.as_deref(),
            Some("{\"type\":\"response.output_text.delta\",\"delta\":\"A\"}")
        );
        assert_eq!(
            second.as_deref(),
            Some("{\"type\":\"response.output_text.delta\",\"delta\":\"B\"}")
        );
    }

    #[test]
    fn done_frames_are_ignored() {
        assert!(extract_sse_data("data: [DONE]").is_none());
    }

    #[test]
    fn maps_text_refusal_and_reasoning_events() {
        let mut state = OpenAiStreamState::default();

        let text = map_event(
            r#"{"type":"response.output_text.delta","delta":"Hello"}"#,
            &mut state,
        )
        .expect("text event should parse");
        let refusal = map_event(
            r#"{"type":"response.refusal.delta","delta":"No"}"#,
            &mut state,
        )
        .expect("refusal event should parse");
        let reasoning = map_event(
            r#"{"type":"response.reasoning_text.delta","item_id":"rs_1","delta":"Think"}"#,
            &mut state,
        )
        .expect("reasoning event should parse");

        assert!(matches!(
            &text[0],
            LMResponsePart::Text(LanguageModelTextPart { value }) if value == "Hello"
        ));
        assert!(matches!(
            &refusal[0],
            LMResponsePart::Text(LanguageModelTextPart { value }) if value == "No"
        ));
        assert!(matches!(
            &reasoning[0],
            LMResponsePart::Thinking(LanguageModelThinkingPart {
                value: LanguageModelThinkingValue::String(value),
                id: Some(id),
                ..
            }) if value == "Think" && id == "rs_1"
        ));
    }

    #[test]
    fn output_text_done_emits_when_no_delta_arrived() {
        let mut state = OpenAiStreamState::default();

        let parts = map_event(
            r#"{"type":"response.output_text.done","item_id":"msg_1","output_index":0,"content_index":0,"text":"Hello"}"#,
            &mut state,
        )
        .expect("output_text.done should parse");

        assert!(matches!(
            &parts[0],
            LMResponsePart::Text(LanguageModelTextPart { value }) if value == "Hello"
        ));
    }

    #[test]
    fn content_part_done_emits_refusal_when_no_delta_arrived() {
        let mut state = OpenAiStreamState::default();

        let parts = map_event(
            r#"{"type":"response.content_part.done","item_id":"msg_1","output_index":0,"content_index":0,"part":{"type":"refusal","refusal":"No"}}"#,
            &mut state,
        )
        .expect("content_part.done should parse");

        assert!(matches!(
            &parts[0],
            LMResponsePart::Text(LanguageModelTextPart { value }) if value == "No"
        ));
    }

    #[test]
    fn accumulates_function_call_arguments_until_done() {
        let mut state = OpenAiStreamState::default();

        let delta = map_event(
            r#"{"type":"response.function_call_arguments.delta","item_id":"call_1","delta":"{\"city\":"}"#,
            &mut state,
        )
        .expect("delta event should parse");
        let done = map_event(
            r#"{"type":"response.function_call_arguments.done","item_id":"call_1","name":"get_weather","arguments":"{\"city\":\"Paris\"}"}"#,
            &mut state,
        )
        .expect("done event should parse");

        assert!(delta.is_empty());
        assert!(matches!(
            &done[0],
            LMResponsePart::ToolCall(LanguageModelToolCallPart { call_id, name, input })
                if call_id == "call_1"
                    && name == "get_weather"
                    && input == &json!({"city": "Paris"})
        ));
    }

    #[test]
    fn maps_failed_and_error_events_to_errors() {
        let mut state = OpenAiStreamState::default();

        let failed = map_event(
            r#"{"type":"response.failed","response":{"error":{"message":"provider failed"}}}"#,
            &mut state,
        )
        .expect_err("response.failed should become an error");
        let errored = map_event(
            r#"{"type":"error","code":"bad_request","message":"invalid input"}"#,
            &mut state,
        )
        .expect_err("error event should become an error");

        assert_eq!(failed, "provider failed");
        assert_eq!(errored, "bad_request: invalid input");
    }

    #[test]
    fn emits_tool_call_from_output_item_done_when_needed() {
        let mut state = OpenAiStreamState::default();

        let parts = map_event(
            r#"{"type":"response.output_item.done","output_index":0,"item":{"id":"fc_1","type":"function_call","name":"get_weather","arguments":"{\"city\":\"Paris\"}"}}"#,
            &mut state,
        )
        .expect("output_item.done should parse");

        assert!(matches!(
            &parts[0],
            LMResponsePart::ToolCall(LanguageModelToolCallPart { call_id, name, input })
                if call_id == "fc_1"
                    && name == "get_weather"
                    && input == &json!({"city": "Paris"})
        ));
    }

    #[test]
    fn output_item_done_does_not_duplicate_function_call_done() {
        let mut state = OpenAiStreamState::default();

        let first = map_event(
            r#"{"type":"response.function_call_arguments.done","item_id":"fc_1","name":"get_weather","arguments":"{\"city\":\"Paris\"}"}"#,
            &mut state,
        )
        .expect("function_call_arguments.done should parse");
        let second = map_event(
            r#"{"type":"response.output_item.done","output_index":0,"item":{"id":"fc_1","type":"function_call","name":"get_weather","arguments":"{\"city\":\"Paris\"}"}}"#,
            &mut state,
        )
        .expect("output_item.done should parse");

        assert_eq!(first.len(), 1);
        assert!(second.is_empty());
    }

    #[test]
    fn emits_reasoning_from_output_item_done() {
        let mut state = OpenAiStreamState::default();

        let parts = map_event(
            r#"{"type":"response.output_item.done","output_index":0,"item":{"id":"rs_1","type":"reasoning","summary":[{"text":"step one","type":"summary_text"},{"text":"step two","type":"summary_text"}],"encrypted_content":"enc_123"}}"#,
            &mut state,
        )
        .expect("reasoning output item should parse");

        assert!(matches!(
            &parts[0],
            LMResponsePart::Thinking(LanguageModelThinkingPart {
                value: LanguageModelThinkingValue::Array(values),
                id: Some(id),
                metadata: Some(metadata),
            }) if id == "rs_1"
                && values == &vec!["step one".to_string(), "step two".to_string()]
                && metadata["encrypted_content"] == "enc_123"
        ));
    }

    #[test]
    fn emits_message_text_from_output_item_done_when_needed() {
        let mut state = OpenAiStreamState::default();

        let parts = map_event(
            r#"{"type":"response.output_item.done","output_index":0,"item":{"id":"msg_1","type":"message","role":"assistant","content":[{"type":"output_text","text":"Hello","annotations":[]}]}}"#,
            &mut state,
        )
        .expect("message output_item.done should parse");

        assert!(matches!(
            &parts[0],
            LMResponsePart::Text(LanguageModelTextPart { value }) if value == "Hello"
        ));
    }

    #[test]
    fn output_item_message_does_not_duplicate_streamed_text() {
        let mut state = OpenAiStreamState::default();

        let delta = map_event(
            r#"{"type":"response.output_text.delta","item_id":"msg_1","output_index":0,"content_index":0,"delta":"Hel"}"#,
            &mut state,
        )
        .expect("output_text.delta should parse");
        let item = map_event(
            r#"{"type":"response.output_item.done","output_index":0,"item":{"id":"msg_1","type":"message","role":"assistant","content":[{"type":"output_text","text":"Hello","annotations":[]}]}}"#,
            &mut state,
        )
        .expect("message output_item.done should parse");

        assert_eq!(delta.len(), 1);
        assert!(item.is_empty());
    }

    #[test]
    fn output_item_reasoning_only_emits_metadata_after_deltas() {
        let mut state = OpenAiStreamState::default();

        let delta = map_event(
            r#"{"type":"response.reasoning_text.delta","item_id":"rs_1","delta":"Think"}"#,
            &mut state,
        )
        .expect("reasoning delta should parse");
        let item = map_event(
            r#"{"type":"response.output_item.done","output_index":0,"item":{"id":"rs_1","type":"reasoning","encrypted_content":"enc_123"}}"#,
            &mut state,
        )
        .expect("reasoning output item should parse");

        assert_eq!(delta.len(), 1);
        assert!(matches!(
            &item[0],
            LMResponsePart::Thinking(LanguageModelThinkingPart {
                value: LanguageModelThinkingValue::String(value),
                id: Some(id),
                metadata: Some(metadata),
            }) if value.is_empty() && id == "rs_1" && metadata["encrypted_content"] == "enc_123"
        ));
    }
}
