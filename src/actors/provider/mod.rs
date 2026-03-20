pub mod adapters;

use std::pin::Pin;

use crate::config::models::{ProviderConfig, ProviderType};
use crate::types::{LMResponsePart, LanguageModelChatMessage};
use ractor::{Actor, ActorProcessingErr, ActorRef};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

pub type ProviderStreamItem = Result<LMResponsePart, String>;
pub type ProviderStream =
    Pin<Box<dyn tokio_stream::Stream<Item = ProviderStreamItem> + Send + Sync>>;
pub type ProviderResponseSender = mpsc::Sender<ProviderStreamItem>;

pub struct ProviderActor;

#[derive(Debug, Clone)]
pub struct ProviderState {
    pub provider_id: String,
    pub provider_type: ProviderType,
    pub api_key: String,
    pub base_url: Option<String>,
    pub client: reqwest::Client,
}

#[derive(Debug, Clone)]
pub struct ProviderChatRequest {
    pub model: String,
    pub messages: Vec<LanguageModelChatMessage>,
}

pub enum ProviderMessage {
    ChatRequest(
        ProviderChatRequest,
        ractor::RpcReplyPort<Result<ProviderStream, String>>,
    ),
}

#[ractor::async_trait]
impl Actor for ProviderActor {
    type Msg = ProviderMessage;
    type State = ProviderState;
    type Arguments = ProviderConfig;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let client = reqwest::Client::builder()
            .user_agent(concat!(
                env!("CARGO_PKG_NAME"),
                "/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .map_err(|error| ActorProcessingErr::from(error.to_string()))?;

        Ok(ProviderState {
            provider_id: args.id,
            provider_type: args.provider_type,
            api_key: args.api_key,
            base_url: args.base_url,
            client,
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ProviderMessage::ChatRequest(request, reply) => {
                let (tx, rx) = mpsc::channel(32);
                let stream = Box::pin(ReceiverStream::new(rx)) as ProviderStream;
                let _ = reply.send(Ok(stream));

                let provider_state = state.clone();
                tokio::spawn(async move {
                    if let Err(error) =
                        adapters::stream_chat(&provider_state, request, tx.clone()).await
                    {
                        let _ = tx.send(Err(error)).await;
                    }
                });
            }
        }
        Ok(())
    }
}
