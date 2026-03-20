use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::{LMModelInfo, LMResponsePart, LanguageModelChatMessage};

/// Gateway Message Envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "camelCase")]
pub enum GatewayMessage {
    /// Client -> Gateway: Connect request
    Connect(ConnectRequest),
    /// Gateway -> Client: Connected event
    Connected(ConnectedEvent),
    /// Client -> Gateway: Select route group request
    SelectRoute(SelectRouteRequest),
    /// Gateway -> Client: Route selected event
    RouteSelected(RouteSelectedEvent),
    /// Client -> Gateway: Chat request
    Chat(ChatRequest),
    /// Gateway -> Client: Stream chunk
    ChatResponseChunk(ChatResponseChunk),
    /// Gateway -> Client: Error event
    Error(ErrorEvent),
}

/// The outer frame if we need a request ID and timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayEnvelope {
    pub request_id: Option<String>,
    pub timestamp: i64,
    pub message: GatewayMessage,
}

impl GatewayEnvelope {
    pub fn new(message: GatewayMessage) -> Self {
        Self {
            request_id: Some(Uuid::new_v4().to_string()),
            timestamp: time::OffsetDateTime::now_utc().unix_timestamp(),
            message,
        }
    }

    pub fn with_request_id(request_id: String, message: GatewayMessage) -> Self {
        Self {
            request_id: Some(request_id),
            timestamp: time::OffsetDateTime::now_utc().unix_timestamp(),
            message,
        }
    }
}

// -----------------------------------------------------------------------------
// Payloads
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    pub auth_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteGroupInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedEvent {
    pub gateway_id: String,
    pub route_groups: Vec<RouteGroupInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectRouteRequest {
    pub route_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteSelectedEvent {
    pub route_id: String,
    pub capabilities: LMModelInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub messages: Vec<LanguageModelChatMessage>,
    pub route_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResponseChunk {
    pub chunk: LMResponsePart,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorEvent {
    pub code: String,
    pub message: String,
}
