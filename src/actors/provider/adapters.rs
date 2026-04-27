pub mod anthropic_messages;
pub mod openai_chat_completions;
pub mod openai_responses;

use crate::config::models::ProviderCompatibility;

use super::{ProviderChatRequest, ProviderResponseSender, ProviderState};

pub async fn stream_chat(
    state: &ProviderState,
    request: ProviderChatRequest,
    tx: ProviderResponseSender,
) -> Result<(), String> {
    match state.compatibility {
        ProviderCompatibility::OpenAiChatCompletions => {
            openai_chat_completions::stream_chat(state, request, tx).await
        }
        ProviderCompatibility::OpenAiResponses => {
            openai_responses::stream_chat(state, request, tx).await
        }
        ProviderCompatibility::AnthropicMessages => {
            anthropic_messages::stream_chat(state, request, tx).await
        }
    }
}
