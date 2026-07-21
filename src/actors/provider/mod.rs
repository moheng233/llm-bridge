pub mod adapters;

use std::pin::Pin;

use crate::config::models::{CompatibilitySettings, ProviderCompatibility};
use crate::types::{
    LMResponsePart, LanguageModelChatMessage, LanguageModelReasoningConfig,
    LanguageModelResponseFormat, LanguageModelTool,
};
use ractor::{Actor, ActorProcessingErr, ActorRef};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{Instrument, info_span, instrument};

/// 结构化 provider 错误：携带可选上游 HTTP 状态码与错误码，供南向透传。
#[derive(Debug, Clone)]
pub struct ProviderError {
    /// 上游返回的 HTTP 状态码（非 2xx）；网络/解析类错误为 None
    pub status: Option<u16>,
    /// 上游错误码（如 `rate_limit_exceeded`、`insufficient_quota`）
    pub code: Option<String>,
    pub message: String,
}

impl ProviderError {
    /// 无状态码的普通错误（网络失败、流解析失败等）
    pub fn plain(message: impl Into<String>) -> Self {
        Self {
            status: None,
            code: None,
            message: message.into(),
        }
    }

    /// 上游非 2xx 响应
    pub fn upstream(status: u16, code: Option<String>, message: impl Into<String>) -> Self {
        Self {
            status: Some(status),
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<String> for ProviderError {
    fn from(message: String) -> Self {
        Self::plain(message)
    }
}

pub type ProviderStreamItem = Result<LMResponsePart, String>;
pub type ProviderStream =
    Pin<Box<dyn tokio_stream::Stream<Item = ProviderStreamItem> + Send + Sync>>;
pub type ProviderResponseSender = mpsc::Sender<ProviderStreamItem>;

/// 启动阶段信号：HTTP 请求已发出且拿到上游响应头（或提前失败）后触发。
/// Ok(status) = 上游 HTTP 状态码；Err = 请求未成功发出（网络错误等）。
pub type ProviderStartSignal = Result<u16, ProviderError>;

pub struct ProviderActor;

#[derive(Debug, Clone)]
pub struct ProviderRuntimeConfig {
    pub id: String,
    pub compatibility: ProviderCompatibility,
    pub api_key: String,
    pub base_url: Option<String>,
    pub compat_settings: Option<CompatibilitySettings>,
}

#[derive(Debug, Clone)]
pub struct ProviderState {
    pub provider_id: String,
    pub compatibility: ProviderCompatibility,
    pub api_key: String,
    pub base_url: Option<String>,
    pub compat_settings: Option<CompatibilitySettings>,
    pub client: reqwest::Client,
}

#[derive(Debug, Clone)]
pub struct ProviderChatRequest {
    pub model: String,
    pub messages: Vec<LanguageModelChatMessage>,
    /// 客户端声明的可用工具（协议无关），由适配器序列化为上游格式
    pub tools: Option<Vec<LanguageModelTool>>,
    /// 工具选择策略，OpenAI 格式原样透传（"auto" | "none" | {"type":"function",...}）
    pub tool_choice: Option<serde_json::Value>,
    /// 采样温度（0-2）
    pub temperature: Option<f64>,
    /// 最大输出 token 数
    pub max_tokens: Option<u32>,
    /// nucleus 采样参数（0-1）
    pub top_p: Option<f64>,
    /// 停止序列
    pub stop: Option<Vec<String>>,
    /// 结构化输出 / JSON mode
    pub response_format: Option<LanguageModelResponseFormat>,
    /// 推理配置（effort / max_tokens）
    pub reasoning: Option<LanguageModelReasoningConfig>,
    /// 确定性采样种子（仅 OpenAI 系支持）
    pub seed: Option<i64>,
    /// 频率惩罚（仅 OpenAI 系支持）
    pub frequency_penalty: Option<f64>,
    /// 存在惩罚（仅 OpenAI 系支持）
    pub presence_penalty: Option<f64>,
    /// token logit 偏置（仅 OpenAI 系支持）
    pub logit_bias: Option<std::collections::HashMap<String, f64>>,
    /// 推理模型的最大补全 token 数；Anthropic/Responses 映射到 max_tokens/max_output_tokens
    pub max_completion_tokens: Option<u32>,
}

/// 由适配器在上游响应中捕获的元数据，用于回填 OpenAI 响应/分块。
#[derive(Debug, Clone, Default)]
pub struct ProviderResponseMetadata {
    pub id: Option<String>,
    pub created: Option<u64>,
}

pub enum ProviderMessage {
    ChatRequest(
        ProviderChatRequest,
        ractor::RpcReplyPort<Result<ProviderStream, String>>,
        tokio::sync::oneshot::Sender<ProviderResponseMetadata>,
        tokio::sync::oneshot::Sender<ProviderStartSignal>,
    ),
}

impl ProviderMessage {
    fn kind(&self) -> &'static str {
        match self {
            Self::ChatRequest(_, _, _, _) => "chat_request",
        }
    }
}

#[ractor::async_trait]
impl Actor for ProviderActor {
    type Msg = ProviderMessage;
    type State = ProviderState;
    type Arguments = ProviderRuntimeConfig;

    #[instrument(level = "info", skip(self, args))]
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
            compatibility: args.compatibility,
            api_key: args.api_key,
            base_url: args.base_url,
            compat_settings: args.compat_settings,
            client,
        })
    }

    #[instrument(
        level = "debug",
        skip(self, state),
        fields(actor_id = ?_myself.get_id(), message = message.kind())
    )]
    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ProviderMessage::ChatRequest(request, reply, metadata_tx, started_tx) => {
                let (tx, rx) = mpsc::channel(32);
                let stream = Box::pin(ReceiverStream::new(rx)) as ProviderStream;
                let _ = reply.send(Ok(stream));

                let provider_state = state.clone();
                let stream_span = info_span!(
                    "provider_adapter_stream",
                    provider = %provider_state.provider_id,
                    compatibility = ?provider_state.compatibility,
                    model = %request.model,
                    message_count = request.messages.len()
                );

                tokio::spawn(
                    async move {
                        if let Err(error) = adapters::stream_chat(
                            &provider_state,
                            request,
                            tx.clone(),
                            metadata_tx,
                            started_tx,
                        )
                        .await
                        {
                            let _ = tx.send(Err(error)).await;
                        }
                    }
                    .instrument(stream_span),
                );
            }
        }
        Ok(())
    }
}
