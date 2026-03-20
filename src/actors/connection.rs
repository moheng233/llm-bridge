use std::sync::Arc;

use ractor::{Actor, ActorProcessingErr, ActorRef};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use crate::actors::gateway_manager::GatewayManagerMessage;
use crate::actors::provider::{ProviderActor, ProviderChatRequest, ProviderMessage};
use crate::config::models::ProviderConfig;
use crate::protocol::{
    ChatResponseChunk, ConnectedEvent, ErrorEvent, GatewayEnvelope, GatewayMessage, RouteGroupInfo,
    RouteSelectedEvent,
};
use crate::routing::models::{ProviderCandidate, RouteGroup};

pub struct ConnectionActor;

pub struct ConnectionState {
    pub gateway_manager: ActorRef<GatewayManagerMessage>,
    pub ws_sender: mpsc::Sender<GatewayEnvelope>,
    pub authenticated: bool,
    pub current_route_group: Option<std::sync::Arc<RouteGroup>>,
}

#[derive(Debug, Clone)]
pub enum ConnectionMessage {
    /// WS msg received from client
    IncomingWSMessage(GatewayMessage),
    /// Provider response chunk
    ProviderChunk(crate::types::LMResponsePart),
    /// Provider error
    ProviderError(String),
}

#[ractor::async_trait]
impl Actor for ConnectionActor {
    type Msg = ConnectionMessage;
    type State = ConnectionState;
    type Arguments = (
        ActorRef<GatewayManagerMessage>,
        mpsc::Sender<GatewayEnvelope>,
    );

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(ConnectionState {
            gateway_manager: args.0,
            ws_sender: args.1,
            authenticated: false,
            current_route_group: None,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ConnectionMessage::IncomingWSMessage(msg) => {
                match msg {
                    GatewayMessage::Connect(_req) => {
                        // For now, always authenticate
                        state.authenticated = true;

                        // Get route groups from gateway manager
                        let reply = ractor::call_t!(
                            state.gateway_manager,
                            GatewayManagerMessage::GetRouteGroups,
                            100
                        );

                        if let Ok(groups) = reply {
                            let route_groups = groups
                                .into_iter()
                                .map(|g| RouteGroupInfo {
                                    id: g.id.clone(),
                                    name: g.name.clone(),
                                })
                                .collect();

                            let _ = state
                                .ws_sender
                                .send(GatewayEnvelope::new(GatewayMessage::Connected(
                                    ConnectedEvent {
                                        gateway_id: "llm-bridge-v1".to_string(),
                                        route_groups,
                                    },
                                )))
                                .await;
                        } else {
                            let _ = state
                                .ws_sender
                                .send(GatewayEnvelope::new(GatewayMessage::Error(ErrorEvent {
                                    code: "INTERNAL_ERROR".to_string(),
                                    message: "Failed to fetch route groups".to_string(),
                                })))
                                .await;
                        }
                    }
                    GatewayMessage::SelectRoute(req) => {
                        if !state.authenticated {
                            return Ok(()); // Ignore or send error
                        }

                        let reply = ractor::call_t!(
                            state.gateway_manager,
                            |reply| GatewayManagerMessage::GetRouteGroup(
                                req.route_id.clone(),
                                reply
                            ),
                            100
                        );

                        if let Ok(Some(group)) = reply {
                            state.current_route_group = Some(group.clone());

                            // Return the capabilities of the primary model (first one)
                            if let Some(primary_model) =
                                group.route_policy.fallback_chain.models.first()
                            {
                                let _ = state
                                    .ws_sender
                                    .send(GatewayEnvelope::new(GatewayMessage::RouteSelected(
                                        RouteSelectedEvent {
                                            route_id: group.id.clone(),
                                            capabilities: primary_model.capabilities.clone(),
                                        },
                                    )))
                                    .await;
                            } else {
                                let _ = state
                                    .ws_sender
                                    .send(GatewayEnvelope::new(GatewayMessage::Error(ErrorEvent {
                                        code: "ROUTE_ERROR".to_string(),
                                        message: "Selected route group has no models".to_string(),
                                    })))
                                    .await;
                            }
                        } else {
                            let _ = state
                                .ws_sender
                                .send(GatewayEnvelope::new(GatewayMessage::Error(ErrorEvent {
                                    code: "ROUTE_NOT_FOUND".to_string(),
                                    message: "Route group not found".to_string(),
                                })))
                                .await;
                        }
                    }
                    GatewayMessage::Chat(req) => {
                        if !state.authenticated {
                            return Ok(());
                        }

                        let Some(group) = resolve_route_group(state, &req).await? else {
                            send_error(state, "NO_ROUTE", "No route group selected").await;
                            return Ok(());
                        };

                        let Some(primary_model) =
                            group.route_policy.fallback_chain.models.first().cloned()
                        else {
                            send_error(state, "ROUTE_ERROR", "Selected route group has no models")
                                .await;
                            return Ok(());
                        };

                        let Some(provider_candidate) =
                            primary_model.provider_candidates.first().cloned()
                        else {
                            send_error(
                                state,
                                "ROUTE_ERROR",
                                "Selected canonical model has no provider candidates",
                            )
                            .await;
                            return Ok(());
                        };

                        let Some(provider_config) =
                            fetch_provider_config(state, &provider_candidate.provider_id).await?
                        else {
                            send_error(
                                state,
                                "PROVIDER_NOT_FOUND",
                                &format!("Provider {} not found", provider_candidate.provider_id),
                            )
                            .await;
                            return Ok(());
                        };

                        start_provider_stream(
                            myself.clone(),
                            req,
                            provider_candidate,
                            provider_config,
                        )
                        .await?;
                    }
                    _ => {}
                }
            }
            ConnectionMessage::ProviderChunk(chunk) => {
                let _ = state
                    .ws_sender
                    .send(GatewayEnvelope::new(GatewayMessage::ChatResponseChunk(
                        ChatResponseChunk { chunk },
                    )))
                    .await;
            }
            ConnectionMessage::ProviderError(err) => {
                let _ = state
                    .ws_sender
                    .send(GatewayEnvelope::new(GatewayMessage::Error(ErrorEvent {
                        code: "PROVIDER_ERROR".to_string(),
                        message: err,
                    })))
                    .await;
            }
        }
        Ok(())
    }
}

async fn resolve_route_group(
    state: &mut ConnectionState,
    request: &crate::protocol::ChatRequest,
) -> Result<Option<Arc<RouteGroup>>, ActorProcessingErr> {
    if let Some(route_id) = &request.route_id {
        let group = ractor::call_t!(
            state.gateway_manager,
            |reply| GatewayManagerMessage::GetRouteGroup(route_id.clone(), reply),
            1000
        )
        .map_err(|error| ActorProcessingErr::from(error.to_string()))?;

        if let Some(group) = group {
            state.current_route_group = Some(group.clone());
            return Ok(Some(group));
        }

        return Ok(None);
    }

    Ok(state.current_route_group.clone())
}

async fn fetch_provider_config(
    state: &ConnectionState,
    provider_id: &str,
) -> Result<Option<ProviderConfig>, ActorProcessingErr> {
    ractor::call_t!(
        state.gateway_manager,
        |reply| GatewayManagerMessage::GetProviderConfig(provider_id.to_string(), reply),
        1000
    )
    .map_err(|error| ActorProcessingErr::from(error.to_string()))
}

async fn start_provider_stream(
    connection_ref: ActorRef<ConnectionMessage>,
    request: crate::protocol::ChatRequest,
    model: ProviderCandidate,
    provider_config: ProviderConfig,
) -> Result<(), ActorProcessingErr> {
    let (provider_ref, provider_handle) = Actor::spawn(None, ProviderActor, provider_config)
        .await
        .map_err(|error| ActorProcessingErr::from(error.to_string()))?;

    let provider_request = ProviderChatRequest {
        model: model.resolved_model_name,
        messages: request.messages,
    };

    let stream = ractor::call_t!(
        provider_ref,
        |reply| ProviderMessage::ChatRequest(provider_request, reply),
        30_000
    )
    .map_err(|error| ActorProcessingErr::from(error.to_string()))?
    .map_err(ActorProcessingErr::from)?;

    tokio::spawn(async move {
        let mut stream = stream;
        while let Some(item) = stream.next().await {
            let cast_result = match item {
                Ok(chunk) => connection_ref.cast(ConnectionMessage::ProviderChunk(chunk)),
                Err(error) => connection_ref.cast(ConnectionMessage::ProviderError(error)),
            };

            if cast_result.is_err() {
                break;
            }
        }

        provider_ref.stop(None);
        let _ = provider_handle.await;
    });

    Ok(())
}

async fn send_error(state: &ConnectionState, code: &str, message: &str) {
    let _ = state
        .ws_sender
        .send(GatewayEnvelope::new(GatewayMessage::Error(ErrorEvent {
            code: code.to_string(),
            message: message.to_string(),
        })))
        .await;
}
