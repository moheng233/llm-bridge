use ractor::{Actor, ActorProcessingErr, ActorRef};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tracing::{Instrument, debug, info, info_span, instrument, warn};

use crate::actors::gateway_manager::GatewayManagerMessage;
use crate::actors::provider::{
    ProviderActor, ProviderChatRequest, ProviderMessage, ProviderRuntimeConfig,
};
use crate::protocol::{
    AvailableModelInfo, ChatResponseChunk, ConnectedEvent, ErrorEvent, GatewayEnvelope,
    GatewayMessage,
};

pub struct ConnectionActor;

pub struct ConnectionState {
    pub gateway_manager: ActorRef<GatewayManagerMessage>,
    pub ws_sender: mpsc::Sender<GatewayEnvelope>,
    pub gateway_id: String,
    pub auth_token: Option<String>,
    pub authenticated: bool,
}

#[derive(Debug, Clone)]
pub enum ConnectionMessage {
    IncomingWSMessage(GatewayMessage),
    ProviderChunk(crate::types::LMResponsePart),
    ProviderError(String),
}

impl ConnectionMessage {
    fn kind(&self) -> &'static str {
        match self {
            Self::IncomingWSMessage(_) => "incoming_ws_message",
            Self::ProviderChunk(_) => "provider_chunk",
            Self::ProviderError(_) => "provider_error",
        }
    }
}

#[ractor::async_trait]
impl Actor for ConnectionActor {
    type Msg = ConnectionMessage;
    type State = ConnectionState;
    type Arguments = (
        ActorRef<GatewayManagerMessage>,
        mpsc::Sender<GatewayEnvelope>,
        String,
        Option<String>,
    );

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(ConnectionState {
            gateway_manager: args.0,
            ws_sender: args.1,
            gateway_id: args.2,
            auth_token: args.3,
            authenticated: false,
        })
    }

    #[instrument(
        level = "debug",
        skip(self, state),
        fields(
            actor_id = ?myself.get_id(),
            message = message.kind(),
            authenticated = state.authenticated
        )
    )]
    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ConnectionMessage::IncomingWSMessage(msg) => match msg {
                GatewayMessage::Connect(req) => {
                    if !authenticate(state.auth_token.as_deref(), req.auth_token.as_deref()) {
                        warn!("connection authentication failed");
                        send_error(state, "AUTH_FAILED", "Authentication failed").await;
                        return Ok(());
                    }

                    state.authenticated = true;
                    info!("connection authenticated");

                    let reply = ractor::call_t!(
                        state.gateway_manager,
                        GatewayManagerMessage::GetAvailableModels,
                        1_000
                    )
                    .map_err(|error| ActorProcessingErr::from(error.to_string()))?;

                    match reply {
                        Ok(models) => {
                            info!(available_models = models.len(), "sending connected event");
                            let available_models = models
                                .into_iter()
                                .map(|model| AvailableModelInfo {
                                    model_name: model.model_name,
                                    capabilities: model.capabilities,
                                })
                                .collect();

                            let _ = state
                                .ws_sender
                                .send(GatewayEnvelope::new(GatewayMessage::Connected(
                                    ConnectedEvent {
                                        gateway_id: state.gateway_id.clone(),
                                        available_models,
                                    },
                                )))
                                .await;
                        }
                        Err(error) => {
                            send_error(state, "INTERNAL_ERROR", &error).await;
                        }
                    }
                }
                GatewayMessage::Chat(req) => {
                    if !state.authenticated {
                        warn!("chat request rejected because connection is unauthenticated");
                        send_error(
                            state,
                            "AUTH_REQUIRED",
                            "Connect before sending chat requests",
                        )
                        .await;
                        return Ok(());
                    }

                    let route = ractor::call_t!(
                        state.gateway_manager,
                        |reply| GatewayManagerMessage::ResolveModel(
                            req.canonical_model_name.clone(),
                            reply,
                        ),
                        5_000
                    )
                    .map_err(|error| ActorProcessingErr::from(error.to_string()))?;

                    let route = match route {
                        Ok(route) => route,
                        Err(error) => {
                            warn!(model = %req.canonical_model_name, error = %error, "model route resolve failed");
                            send_error(state, "MODEL_NOT_AVAILABLE", &error).await;
                            return Ok(());
                        }
                    };

                    info!(
                        model = %req.canonical_model_name,
                        provider = %route.provider_name,
                        provider_model = %route.provider_model_name,
                        "resolved model route for chat request"
                    );

                    let provider_config = ProviderRuntimeConfig {
                        id: route.provider_name,
                        provider_type: route.provider_type,
                        api_key: route.api_key,
                        base_url: route.base_url,
                    };

                    start_provider_stream(myself, req, route.provider_model_name, provider_config)
                        .await?;
                }
                _ => {}
            },
            ConnectionMessage::ProviderChunk(chunk) => {
                let _ = state
                    .ws_sender
                    .send(GatewayEnvelope::new(GatewayMessage::ChatResponseChunk(
                        ChatResponseChunk { chunk },
                    )))
                    .await;
            }
            ConnectionMessage::ProviderError(err) => {
                send_error(state, "PROVIDER_ERROR", &err).await;
            }
        }

        Ok(())
    }
}

fn authenticate(expected: Option<&str>, provided: Option<&str>) -> bool {
    match expected {
        Some(expected) => provided == Some(expected),
        None => true,
    }
}

#[instrument(
    level = "info",
    skip(connection_ref, request, provider_config),
    fields(
        provider = %provider_config.id,
        provider_type = ?provider_config.provider_type,
        provider_model = %provider_model_name,
        message_count = request.messages.len()
    )
)]
async fn start_provider_stream(
    connection_ref: ActorRef<ConnectionMessage>,
    request: crate::protocol::ChatRequest,
    provider_model_name: String,
    provider_config: ProviderRuntimeConfig,
) -> Result<(), ActorProcessingErr> {
    info!("starting provider stream");
    let (provider_ref, provider_handle) = Actor::spawn(None, ProviderActor, provider_config)
        .await
        .map_err(|error| ActorProcessingErr::from(error.to_string()))?;

    let provider_request = ProviderChatRequest {
        model: provider_model_name,
        messages: request.messages,
    };

    let stream = ractor::call_t!(
        provider_ref,
        |reply| ProviderMessage::ChatRequest(provider_request, reply),
        30_000
    )
    .map_err(|error| ActorProcessingErr::from(error.to_string()))?
    .map_err(ActorProcessingErr::from)?;

    tokio::spawn(
        async move {
            let mut stream = stream;
            while let Some(item) = stream.next().await {
                let cast_result = match item {
                    Ok(chunk) => connection_ref.cast(ConnectionMessage::ProviderChunk(chunk)),
                    Err(error) => {
                        warn!(error = %error, "provider stream returned error chunk");
                        connection_ref.cast(ConnectionMessage::ProviderError(error))
                    }
                };

                if cast_result.is_err() {
                    break;
                }
            }

            provider_ref.stop(None);
            let _ = provider_handle.await;
            debug!("provider stream task finished");
        }
        .instrument(info_span!("provider_stream_forwarder")),
    );

    Ok(())
}

#[instrument(level = "debug", skip(state, message), fields(code = code))]
async fn send_error(state: &ConnectionState, code: &str, message: &str) {
    let _ = state
        .ws_sender
        .send(GatewayEnvelope::new(GatewayMessage::Error(ErrorEvent {
            code: code.to_string(),
            message: message.to_string(),
        })))
        .await;
}
