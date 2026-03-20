pub mod openai;

use crate::config::models::ProviderType;

use super::{ProviderChatRequest, ProviderResponseSender, ProviderState};

pub async fn stream_chat(
    state: &ProviderState,
    request: ProviderChatRequest,
    tx: ProviderResponseSender,
) -> Result<(), String> {
    match state.provider_type {
        ProviderType::OpenAI => openai::stream_chat(state, request, tx).await,
        ProviderType::Anthropic => Err("provider anthropic is not implemented yet".to_string()),
        ProviderType::Gemini => Err("provider gemini is not implemented yet".to_string()),
    }
}
