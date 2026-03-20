use axum::{
    Extension, Router,
    extract::ws::{Message as AxumWsMessage, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use ractor::{Actor, ActorRef};
use tokio::sync::mpsc;
use tracing::{Instrument, debug, error, info, info_span, instrument, warn};

use crate::actors::connection::{ConnectionActor, ConnectionMessage};
use crate::actors::gateway_manager::GatewayManagerMessage;
use crate::protocol::GatewayEnvelope;

/// Server state shared across requests
#[derive(Clone)]
pub struct AppState {
    pub gateway_manager: ActorRef<GatewayManagerMessage>,
    pub gateway_id: String,
    pub auth_token: Option<String>,
}

#[instrument(
    level = "info",
    skip(state),
    fields(
        gateway_id = %state.gateway_id,
        host = %host,
        port,
        auth_required = state.auth_token.is_some()
    )
)]
pub async fn start_server(state: AppState, host: &str, port: u16) -> Result<(), std::io::Error> {
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .layer(Extension(state));

    let addr = format!("{}:{}", host, port);
    info!("Starting WebSocket server on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await
}

#[instrument(
    level = "debug",
    skip(ws, state),
    fields(gateway_id = %state.gateway_id, auth_required = state.auth_token.is_some())
)]
async fn ws_handler(
    ws: WebSocketUpgrade,
    Extension(state): Extension<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

#[instrument(
    level = "info",
    skip(socket, state),
    fields(gateway_id = %state.gateway_id, auth_required = state.auth_token.is_some())
)]
async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Channel for the actor to send messages back to the WebSocket
    let (ws_tx, mut ws_rx) = mpsc::channel::<GatewayEnvelope>(100);

    // Spawn the ConnectionActor for this specific socket
    let (actor_ref, actor_handle) = match Actor::spawn(
        None,
        ConnectionActor,
        (
            state.gateway_manager.clone(),
            ws_tx,
            state.gateway_id.clone(),
            state.auth_token.clone(),
        ),
    )
    .await
    {
        Ok(res) => res,
        Err(e) => {
            error!("Failed to spawn ConnectionActor: {}", e);
            return;
        }
    };

    let connection_id = actor_ref.get_id();
    let connection_span = info_span!("ws_connection", actor_id = ?connection_id);
    let _connection_guard = connection_span.enter();
    info!(
        "New WebSocket connection established, Actor ID: {}",
        connection_id
    );

    // Task 1: Read from Actor's channel and write to WebSocket (Downstream)
    let send_task = tokio::spawn(
        async move {
            while let Some(envelope) = ws_rx.recv().await {
                match serde_json::to_string(&envelope) {
                    Ok(json) => {
                        if sender.send(AxumWsMessage::Text(json.into())).await.is_err() {
                            warn!("Failed to send message to websocket client");
                            break;
                        }
                        debug!("sent gateway envelope to websocket client");
                    }
                    Err(e) => {
                        error!("Failed to serialize message envelope: {}", e);
                    }
                }
            }
        }
        .instrument(info_span!("ws_downstream", actor_id = ?connection_id)),
    );

    // Task 2: Read from WebSocket and send to Actor (Upstream)
    let actor_ref_clone = actor_ref.clone();
    let recv_task = tokio::spawn(
        async move {
            while let Some(Ok(msg)) = receiver.next().await {
                if let AxumWsMessage::Text(text) = msg {
                    match serde_json::from_str::<GatewayEnvelope>(&text) {
                        Ok(envelope) => {
                            debug!("received websocket message and forwarding to connection actor");
                            let _ = actor_ref_clone
                                .cast(ConnectionMessage::IncomingWSMessage(envelope.message));
                        }
                        Err(e) => {
                            warn!("Failed to parse incoming WS message: {}. Raw: {}", e, text);
                        }
                    }
                } else if let AxumWsMessage::Close(_) = msg {
                    break;
                }
            }
        }
        .instrument(info_span!("ws_upstream", actor_id = ?connection_id)),
    );

    // Wait for either the read or write task to finish/fail
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    info!(
        "WebSocket connection closed, stopping Actor ID: {}",
        connection_id
    );
    actor_ref.stop(Some("Client disconnected".to_string()));
    let _ = actor_handle.await;
}
