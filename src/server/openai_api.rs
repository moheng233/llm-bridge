use std::sync::Arc;

use axfetchum::ApiRouter;
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response, sse::{Event, Sse}},
    routing::get,
};
use tower_sessions::SessionManagerLayer;
use tower_sessions::MemoryStore;
use futures_util::stream::Stream;
use ractor::Actor;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;
use tracing::{info, instrument};

use crate::actors::{gateway_manager::GatewayManagerMessage, provider::adapters::openai_chat_completions::flatten_thinking_value_for_sse};
use crate::actors::provider::{ProviderActor, ProviderChatRequest, ProviderMessage, ProviderRuntimeConfig};
use crate::db;
use crate::middleware::token_auth::TokenAuth;
use crate::store::Store;
use crate::types::{LMResponsePart, LanguageModelChatMessage, LanguageModelChatMessageRole, LanguageModelInputPart, LanguageModelTextPart};

use super::admin::{admin_crud_routes, model_browse_routes};
use super::auth::{self, AuthState};
use super::tokens;

/// Shared application state for HTTP handlers.
#[derive(Clone)]
pub struct AppState {
    pub gateway_manager: ractor::ActorRef<GatewayManagerMessage>,
    pub store: Arc<Store>,
    pub auth_token: Option<String>,
    /// OIDC auth sub-state（仅在配置了 OIDC 时 Some）
    pub auth: Option<AuthState>,
    /// SQLite 数据库句柄（始终可用）
    pub db: db::Db,
}

// ── Auth ──

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
pub struct OpenAiModelList {
    object: &'static str,
    data: Vec<OpenAiModelEntry>,
}

#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
struct OpenAiModelEntry {
    id: String,
    object: &'static str,
    created: i64,
    owned_by: String,
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

// ── Server Startup ──

fn openai_routes() -> ApiRouter<AppState> {
    ApiRouter::<AppState>::new()
        .group("openai")
        .get("/v1/models", list_models)
            .response::<OpenAiModelList>()
            .auth()
            .done()
        .post("/v1/chat/completions", chat_completions)
            .done()
}

fn token_routes() -> ApiRouter<AppState> {
    ApiRouter::<AppState>::new()
        .group("tokens")
        .get("/api/v1/tokens", tokens::list_tokens)
            .response::<Vec<tokens::TokenListItem>>()
            .auth()
            .done()
        .post("/api/v1/tokens", tokens::create_token)
            .json::<crate::auth::token::CreateTokenRequest, crate::auth::token::CreateTokenResponse>()
            .auth()
            .done()
        .patch("/api/v1/tokens/{id}", tokens::update_token)
            .json::<crate::auth::token::UpdateTokenRequest, tokens::TokenListItem>()
            .auth()
            .done()
        .delete("/api/v1/tokens/{id}", tokens::delete_token)
            .auth()
            .done()
}

/// Merge all route collections (for TypeScript client generation via axfetchum).
pub fn all_api_routes() -> (Router<AppState>, axfetchum::RouteCollection) {
    model_browse_routes()
        .merge(admin_crud_routes())
        .merge(openai_routes())
        .merge(token_routes())
        .build()
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
    let (admin_router, _) = model_browse_routes().build();
    let (admin_ext_router, _) = admin_crud_routes().build();
    let (openai_router, _) = openai_routes().build();

    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store).with_secure(false);

    let app = if let Some(auth_state) = state.auth.clone() {
        let auth_router = Router::new()
            .route("/auth/login", get(auth::login))
            .route("/auth/callback", get(auth::callback))
            .route("/auth/me", get(auth::me))
            .route("/auth/logout", axum::routing::post(auth::logout))
            .with_state(auth_state)
            .layer(session_layer.clone());

        let (tokens_router, _) = token_routes().build();
        let tokens_router = tokens_router
            .with_state(state.clone())
            .layer(session_layer.clone());

        admin_router
            .merge(openai_router)
            .merge(auth_router)
            .merge(tokens_router)
            .merge(admin_ext_router)
            .with_state(state)
            .layer(session_layer)
    } else {
        admin_router
            .merge(openai_router)
            .merge(admin_ext_router)
            .with_state(state)
    };

    let addr = format!("{}:{}", host, port);
    info!("Starting server on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await
}
