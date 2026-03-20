use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::{LMModelInfo, LMResponsePart, LanguageModelChatMessage};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "camelCase")]
pub enum GatewayMessage {
    Connect(ConnectRequest),
    Connected(ConnectedEvent),
    Chat(ChatRequest),
    ChatResponseChunk(ChatResponseChunk),
    Error(ErrorEvent),
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    pub auth_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableModelInfo {
    pub model_name: String,
    pub capabilities: LMModelInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedEvent {
    pub gateway_id: String,
    pub available_models: Vec<AvailableModelInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub canonical_model_name: String,
    pub messages: Vec<LanguageModelChatMessage>,
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
