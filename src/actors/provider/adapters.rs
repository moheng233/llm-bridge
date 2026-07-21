pub mod anthropic_messages;
pub mod openai_chat_completions;
pub mod openai_responses;

use crate::config::models::ProviderCompatibility;

use super::{
    ProviderChatRequest, ProviderResponseMetadata, ProviderResponseSender, ProviderStartSignal,
    ProviderState,
};

pub async fn stream_chat(
    state: &ProviderState,
    request: ProviderChatRequest,
    tx: ProviderResponseSender,
    metadata_tx: tokio::sync::oneshot::Sender<ProviderResponseMetadata>,
    started_tx: tokio::sync::oneshot::Sender<ProviderStartSignal>,
) -> Result<(), String> {
    match state.compatibility {
        ProviderCompatibility::OpenAiChatCompletions => {
            openai_chat_completions::stream_chat(state, request, tx, metadata_tx, started_tx).await
        }
        ProviderCompatibility::OpenAiResponses => {
            openai_responses::stream_chat(state, request, tx, metadata_tx, started_tx).await
        }
        ProviderCompatibility::AnthropicMessages => {
            anthropic_messages::stream_chat(state, request, tx, metadata_tx, started_tx).await
        }
    }
}
