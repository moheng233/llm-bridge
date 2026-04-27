use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response, sse::{Event, Sse}},
    routing::{get, post},
};
use futures_util::stream::Stream;
use ractor::Actor;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;
use tracing::{debug, info, instrument};

use crate::actors::gateway_manager::GatewayManagerMessage;
use crate::actors::provider::{ProviderActor, ProviderChatRequest, ProviderMessage, ProviderRuntimeConfig};
use crate::store::Store;
use crate::types::{LMResponsePart, LanguageModelChatMessage, LanguageModelChatMessageRole, LanguageModelInputPart, LanguageModelTextPart};

use super::admin::all_routes;

/// Shared application state for HTTP handlers.
#[derive(Clone)]
pub struct AppState {
    pub gateway_manager: ractor::ActorRef<GatewayManagerMessage>,
    pub store: Arc<Store>,
    pub auth_token: Option<String>,
}

// ── Auth ──

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

#[derive(Debug, Serialize)]
struct OpenAiModelList {
    object: &'static str,
    data: Vec<OpenAiModelEntry>,
}

#[derive(Debug, Serialize)]
struct OpenAiModelEntry {
    id: String,
    object: &'static str,
    created: i64,
    owned_by: String,
}

#[instrument(level = "debug", skip(state))]
pub async fn list_models(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<OpenAiModelList>, Response> {
    check_auth(&state, &headers)?;

    let models = state.store.list_available_models();
    let data = models
        .into_iter()
        .map(|m| OpenAiModelEntry {
            id: m.model_name,
            object: "model",
            created: 0,
            owned_by: m.provider_ids.into_iter().next().unwrap_or_default(),
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

#[instrument(level = "info", skip(state, headers), fields(model = %req.model, stream = req.stream))]
pub async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ChatCompletionRequest>,
) -> Result<Response, Response> {
    check_auth(&state, &headers)?;

    let routes = state.store.resolve_model(&req.model);
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
        while let Some(item) = stream.next().await {
            match item {
                Ok(LMResponsePart::Text(t)) => content.push_str(&t.value),
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

        Ok(Json(serde_json::json!({
            "id": "chatcmpl-llm-bridge",
            "object": "chat.completion",
            "created": 0,
            "model": req.model,
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": content
                },
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
                let (delta_content, finish_reason) = match &part {
                    LMResponsePart::Text(t) => (Some(t.value.clone()), None),
                    LMResponsePart::ToolCall(_tc) => (None, Some("tool_calls")),
                    _ => (None, None),
                };

                let chunk = serde_json::json!({
                    "id": "chatcmpl-llm-bridge",
                    "object": "chat.completion.chunk",
                    "created": 0,
                    "model": model,
                    "choices": [{
                        "index": 0,
                        "delta": {
                            "content": delta_content,
                        },
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

// ── Server Startup ──

fn openai_routes() -> Router<AppState> {
    Router::new()
        .route("/v1/models", get(list_models))
        .route("/v1/chat/completions", post(chat_completions))
}

#[instrument(
    level = "info",
    skip(state),
    fields(
        host = %host,
        port,
        auth_required = state.auth_token.is_some()
    )
)]
pub async fn start_server(state: AppState, host: &str, port: u16) -> Result<(), std::io::Error> {
    let (admin_router, _admin_routes) = all_routes();
    let openai_router = openai_routes();

    let app = admin_router
        .merge(openai_router)
        .with_state(state);

    let addr = format!("{}:{}", host, port);
    info!("Starting server on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await
}
