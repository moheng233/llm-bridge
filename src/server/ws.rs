//! 协议无关 WebSocket RPC；聊天生命周期由 chat_common 唯一持有。
use super::{
    AppState,
    chat_common::{self, ChatPrepareError, ChatSession},
};
use crate::{
    actors::provider::ProviderChatRequest,
    db::models::{Token, TraceInterface, TraceStatus},
    middleware::token_auth::TokenAuth,
    types::*,
};
use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket},
    },
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{
    sync::{mpsc, watch},
    task::JoinSet,
};

const OUTBOUND_CAPACITY: usize = 64;
const MAX_CONCURRENT: usize = 8;
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
struct Outgoing {
    frames: mpsc::Sender<Message>,
    close: watch::Sender<bool>,
}
impl Outgoing {
    fn send(&self, message: WsServerMessage) -> bool {
        match serde_json::to_string(&message) {
            Ok(json) => self.frame(Message::Text(json.into())),
            Err(_) => {
                let _ = self.close.send(true);
                false
            }
        }
    }
    fn frame(&self, frame: Message) -> bool {
        if self.frames.try_send(frame).is_err() {
            let _ = self.close.send(true);
            return false;
        }
        true
    }
    fn error(&self, id: Option<String>, code: WsErrorCode, message: impl Into<String>) {
        self.send(WsServerMessage::Error {
            id,
            error: WsErrorBody {
                code,
                message: message.into(),
            },
        });
    }
}

pub async fn ws_handler(
    State(state): State<AppState>,
    TokenAuth(token): TokenAuth,
    upgrade: WebSocketUpgrade,
) -> Response {
    upgrade
        .protocols(["lm-bridge.v1"])
        .max_message_size(2 * 1024 * 1024)
        .on_upgrade(move |socket| connection(socket, state, Arc::new(token)))
}

async fn connection(socket: WebSocket, state: AppState, token: Arc<Token>) {
    let (mut sink, mut source) = socket.split();
    let (frames, mut pending) = mpsc::channel(OUTBOUND_CAPACITY);
    let (close, mut closing) = watch::channel(false);
    let outgoing = Outgoing {
        frames,
        close: close.clone(),
    };
    let mut writer_closing = closing.clone();
    let writer_close = close.clone();
    let writer = tokio::spawn(async move {
        let mut ping = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_secs(30),
            Duration::from_secs(30),
        );
        loop {
            let next = tokio::select! {
                biased;
                _ = writer_closing.changed() => break,
                _ = ping.tick() => Message::Ping(Vec::new().into()),
                message = pending.recv() => match message { Some(message) => message, None => break },
            };
            if !matches!(
                tokio::time::timeout(WRITE_TIMEOUT, sink.send(next)).await,
                Ok(Ok(()))
            ) {
                break;
            }
        }
        let _ = writer_close.send(true);
        let _ = tokio::time::timeout(
            Duration::from_millis(250),
            sink.send(Message::Close(Some(CloseFrame {
                code: 1008,
                reason: "connection closed or consumer too slow".into(),
            }))),
        )
        .await;
    });
    let mut active: HashMap<String, watch::Sender<bool>> = HashMap::new();
    let mut tasks = JoinSet::new();
    loop {
        let frame = tokio::select! {
            biased;
            completed = tasks.join_next(), if !tasks.is_empty() => {
                match completed {
                    Some(Ok(id)) => { active.remove(&id); }
                    Some(Err(error)) => { tracing::error!(%error, "WS request task failed"); break; }
                    None => {}
                }
                continue;
            }
            _ = closing.changed() => break,
            frame = source.next() => match frame { Some(Ok(frame)) => frame, _ => break },
        };
        let text = match frame {
            Message::Text(text) => text,
            Message::Ping(data) => {
                if !outgoing.frame(Message::Pong(data)) {
                    break;
                }
                continue;
            }
            Message::Pong(_) => continue,
            Message::Close(_) => break,
            Message::Binary(_) => {
                outgoing.error(
                    None,
                    WsErrorCode::InvalidRequest,
                    "JSON text frames are required",
                );
                continue;
            }
        };
        let request = match serde_json::from_str::<WsClientMessage>(&text) {
            Ok(request) => request,
            Err(error) => {
                let id = serde_json::from_str::<serde_json::Value>(&text)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("id")?
                            .as_str()
                            .filter(|id| id.len() <= 128)
                            .map(str::to_owned)
                    });
                outgoing.error(id, WsErrorCode::InvalidRequest, error.to_string());
                continue;
            }
        };
        let id = match &request {
            WsClientMessage::Chat { id, .. }
            | WsClientMessage::ListModels { id }
            | WsClientMessage::Cancel { id, .. } => id,
        };
        if id.is_empty() || id.len() > 128 || active.contains_key(id) {
            outgoing.error(
                None,
                WsErrorCode::InvalidRequest,
                "id must be nonempty, at most 128 bytes, and not already active",
            );
            continue;
        }
        match request {
            WsClientMessage::Cancel { id, target_id } => {
                if let Some(cancel) = active.get(&target_id) {
                    if cancel.send(true).is_ok() {
                        outgoing.send(WsServerMessage::Result {
                            id,
                            result: WsResult::Cancel { cancelled: true },
                        });
                    } else {
                        outgoing.error(
                            Some(id),
                            WsErrorCode::RequestNotFound,
                            "request already finished",
                        );
                    }
                } else {
                    outgoing.error(
                        Some(id),
                        WsErrorCode::RequestNotFound,
                        "request is not active on this connection",
                    );
                }
            }
            WsClientMessage::ListModels { id } => {
                let allowed = match serde_json::from_str::<Vec<String>>(&token.allowed_models) {
                    Ok(allowed) => allowed,
                    Err(_) => {
                        outgoing.error(
                            Some(id),
                            WsErrorCode::InternalError,
                            "invalid stored model permissions",
                        );
                        continue;
                    }
                };
                match state.store.list_available_models().await {
                    Ok(models) => {
                        let models = models
                            .into_iter()
                            .filter(|model| {
                                allowed.is_empty() || allowed.contains(&model.model_name)
                            })
                            .map(|model| model.nominal_capabilities)
                            .collect();
                        outgoing.send(WsServerMessage::Result {
                            id,
                            result: WsResult::Models(WsListModelsResult(models)),
                        });
                    }
                    Err(error) => outgoing.error(Some(id), WsErrorCode::InternalError, error),
                }
            }
            WsClientMessage::Chat { id, params } => {
                if active.len() >= MAX_CONCURRENT {
                    outgoing.error(
                        Some(id),
                        WsErrorCode::InvalidRequest,
                        "at most 8 concurrent chats per connection",
                    );
                    continue;
                }
                if params.model.is_empty() || params.messages.is_empty() {
                    outgoing.error(
                        Some(id),
                        WsErrorCode::InvalidRequest,
                        "model and messages must be nonempty",
                    );
                    continue;
                }
                let (cancel, cancelled) = watch::channel(false);
                active.insert(id.clone(), cancel);
                let state = state.clone();
                let token = token.clone();
                let outgoing = outgoing.clone();
                tasks.spawn(async move {
                    run_chat(&state, &token, &id, *params, &outgoing, cancelled).await;
                    id
                });
            }
        }
    }
    for cancel in active.values() {
        let _ = cancel.send(true);
    }
    let _ = close.send(true);
    while tasks.join_next().await.is_some() {}
    let _ = writer.await;
}

fn prepare_error(outgoing: &Outgoing, id: &str, error: ChatPrepareError) {
    let code = match error.code {
        "model_not_found" => WsErrorCode::ModelNotFound,
        "model_not_allowed" => WsErrorCode::ModelNotAllowed,
        "quota_exceeded" => WsErrorCode::QuotaExceeded,
        "provider_error" => WsErrorCode::ProviderError,
        _ => WsErrorCode::InternalError,
    };
    outgoing.error(Some(id.into()), code, error.message);
}

async fn run_chat(
    state: &AppState,
    token: &Token,
    id: &str,
    params: WsChatParams,
    outgoing: &Outgoing,
    mut cancel: watch::Receiver<bool>,
) {
    let request = ProviderChatRequest {
        model: params.model,
        messages: params.messages,
        tools: params.tools,
        tool_choice: params.tool_choice,
        temperature: params.temperature,
        max_tokens: params.max_tokens,
        top_p: params.top_p,
        stop: params.stop,
        response_format: params.response_format,
        reasoning: params.reasoning,
        seed: params.seed,
        frequency_penalty: params.frequency_penalty,
        presence_penalty: params.presence_penalty,
        logit_bias: params.logit_bias,
        max_completion_tokens: params.max_completion_tokens,
    };
    let request_id = uuid::Uuid::new_v4().to_string();
    let session = tokio::select! {
        biased;
        _ = cancel.changed() => { outgoing.send(WsServerMessage::Done { id: id.into(), done: WsChatDone { finish_reason: None, cancelled: true } }); return; }
        prepared = chat_common::prepare_chat_request(state, token, request, request_id, TraceInterface::WsRpc) => match prepared {
            Ok(session) => session,
            Err(error) => { prepare_error(outgoing, id, error); return; }
        }
    };
    let ChatSession {
        mut stream,
        mut ready,
        done,
        metadata: _,
    } = session;
    let mut cancelled = false;
    let ready_result = tokio::select! {
        biased;
        _ = cancel.changed() => { cancelled = true; None }
        ready = &mut ready => Some(ready),
    };
    match ready_result {
        Some(Ok(Err(error))) => {
            prepare_error(outgoing, id, error);
            return;
        }
        Some(Err(_)) => {
            outgoing.error(
                Some(id.into()),
                WsErrorCode::InternalError,
                "chat supervisor stopped during startup",
            );
            return;
        }
        _ => {}
    }
    while !cancelled {
        let next = tokio::select! {
            biased;
            _ = cancel.changed() => { cancelled = true; break; }
            part = stream.next() => part,
        };
        match next {
            Some(Ok(chunk)) => {
                if !outgoing.send(WsServerMessage::Chunk {
                    id: id.into(),
                    chunk,
                }) {
                    cancelled = true;
                    break;
                }
            }
            Some(Err(_)) | None => break,
        }
    }
    drop(stream);
    match done.await {
        Ok(outcome) => {
            if let Some(error) = outcome.error {
                outgoing.error(Some(id.into()), WsErrorCode::ProviderError, error);
            } else {
                outgoing.send(WsServerMessage::Done {
                    id: id.into(),
                    done: WsChatDone {
                        finish_reason: outcome.usage.finish_reason,
                        cancelled: cancelled || outcome.status == TraceStatus::Cancelled,
                    },
                });
            }
        }
        Err(_) if cancelled => {
            outgoing.send(WsServerMessage::Done {
                id: id.into(),
                done: WsChatDone {
                    finish_reason: None,
                    cancelled: true,
                },
            });
        }
        Err(_) => outgoing.error(
            Some(id.into()),
            WsErrorCode::InternalError,
            "chat supervisor stopped before settlement",
        ),
    }
}
