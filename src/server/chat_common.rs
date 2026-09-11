//! HTTP/WS 共用的准入、回退、流生命周期与唯一结算点。
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use ractor::{Actor, ActorRef};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
use tracing::Instrument;

use super::AppState;
use crate::actors::provider::{
    ProviderActor, ProviderChatRequest, ProviderMessage, ProviderResponseMetadata,
    ProviderRuntimeConfig, ProviderStream,
};
use crate::auth::quota;
use crate::db::models::{Token, TraceInterface, TraceStatus};
use crate::observability::trace_writer::{BeginTrace, FinalizeTrace, TraceEvent};
use crate::store::router::ResolvedProviderRoute;
use crate::types::{LMResponsePart, LanguageModelUsagePart};

#[derive(Debug, Clone)]
pub struct ChatPrepareError {
    pub status: u16,
    pub code: &'static str,
    pub message: String,
}

impl ChatPrepareError {
    fn new(status: u16, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
    fn retryable(&self) -> bool {
        self.status == 429 || self.status >= 500
    }
}

#[derive(Debug, Default, Clone)]
pub struct UsageAccumulator {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub cached_tokens: Option<u64>,
    pub finish_reason: Option<String>,
}

impl UsageAccumulator {
    pub fn merge(&mut self, usage: &LanguageModelUsagePart) {
        if usage.input_tokens.is_some() {
            self.input_tokens = usage.input_tokens;
        }
        if usage.output_tokens.is_some() {
            self.output_tokens = usage.output_tokens;
        }
        if usage.total_tokens.is_some() {
            self.total_tokens = usage.total_tokens;
        }
        if usage.reasoning_tokens.is_some() {
            self.reasoning_tokens = usage.reasoning_tokens;
        }
        if usage.cached_tokens.is_some() {
            self.cached_tokens = usage.cached_tokens;
        }
        if usage.finish_reason.is_some() {
            self.finish_reason = usage.finish_reason.clone();
        }
    }
    pub fn total(&self) -> Option<u64> {
        self.total_tokens
            .or_else(|| self.input_tokens?.checked_add(self.output_tokens?))
    }
    pub fn to_openai_usage(&self) -> Option<serde_json::Value> {
        let input = self.input_tokens?;
        let output = self.output_tokens.unwrap_or(0);
        let mut value = serde_json::json!({
            "prompt_tokens": input, "completion_tokens": output,
            "total_tokens": self.total().unwrap_or(input.saturating_add(output)),
        });
        if let Some(reasoning) = self.reasoning_tokens {
            value["completion_tokens_details"] = serde_json::json!({"reasoning_tokens": reasoning});
        }
        if let Some(cached) = self.cached_tokens {
            value["prompt_tokens_details"] = serde_json::json!({"cached_tokens": cached});
        }
        Some(value)
    }
}

pub struct ChatOutcome {
    pub usage: UsageAccumulator,
    pub metadata: ProviderResponseMetadata,
    pub status: TraceStatus,
    pub error: Option<String>,
}

/// 丢弃 stream 会通知 supervisor 终止上游；done 仅在结算完成后返回。
/// WS cancel 应丢弃 stream 后等待 done，不应 abort supervisor。
pub struct ChatSession {
    pub stream: ReceiverStream<Result<LMResponsePart, String>>,
    pub metadata: oneshot::Receiver<ProviderResponseMetadata>,
    pub done: oneshot::Receiver<ChatOutcome>,
    pub ready: oneshot::Receiver<Result<(), ChatPrepareError>>,
}

struct ActorLease(ActorRef<ProviderMessage>);
impl Drop for ActorLease {
    fn drop(&mut self) {
        self.0.stop(None);
    }
}

struct ActiveProvider {
    _actor: ActorLease,
    stream: ProviderStream,
    metadata: oneshot::Receiver<ProviderResponseMetadata>,
}

async fn start_provider(
    route: &ResolvedProviderRoute,
    mut request: ProviderChatRequest,
) -> Result<ActiveProvider, ChatPrepareError> {
    request.model = route.provider_model_name.clone();
    let config = ProviderRuntimeConfig {
        id: route.provider_name.clone(),
        compatibility: route.compatibility.clone(),
        api_key: route.api_key.clone(),
        base_url: route.base_url.clone(),
        compat_settings: route.compat_settings.clone(),
    };
    let (actor, _handle) = Actor::spawn(None, ProviderActor, config)
        .await
        .map_err(|e| ChatPrepareError::new(500, "internal_error", e.to_string()))?;
    let lease = ActorLease(actor);
    let (metadata_tx, metadata) = oneshot::channel();
    let (started_tx, started) = oneshot::channel();
    let mut stream = ractor::call_t!(
        lease.0,
        |reply| { ProviderMessage::ChatRequest(request, reply, metadata_tx, started_tx) },
        30_000
    )
    .map_err(|e| ChatPrepareError::new(502, "provider_error", e.to_string()))?
    .map_err(|e| ChatPrepareError::new(502, "provider_error", e))?;
    match tokio::time::timeout(Duration::from_secs(30), started).await {
        Ok(Ok(Ok(status))) if (200..300).contains(&status) => {}
        Ok(Ok(Ok(status))) => {
            let message = match tokio::time::timeout(Duration::from_secs(2), stream.next()).await {
                Ok(Some(Err(message))) => message,
                _ => format!("upstream returned status {status}"),
            };
            return Err(ChatPrepareError::new(status, "provider_error", message));
        }
        Ok(Ok(Err(error))) => {
            return Err(ChatPrepareError::new(
                error.status.unwrap_or(502),
                "provider_error",
                error.message,
            ));
        }
        Ok(Err(_)) => {
            return Err(ChatPrepareError::new(
                502,
                "provider_error",
                "upstream closed before startup signal",
            ));
        }
        Err(_) => {
            return Err(ChatPrepareError::new(
                504,
                "provider_error",
                "upstream startup timed out",
            ));
        }
    }
    Ok(ActiveProvider {
        _actor: lease,
        stream,
        metadata,
    })
}

/// 只计序列化长度，不分配完整请求副本。估算不是对上游实际用量的保证。
pub fn estimate_token_count(request: &ProviderChatRequest) -> i64 {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    let _ = serde_json::to_writer(&mut counter, &request.messages);
    let _ = serde_json::to_writer(&mut counter, &request.tools);
    let input = i64::try_from(counter.0.div_ceil(4)).unwrap_or(i64::MAX);
    input
        .saturating_add(i64::from(
            request
                .max_completion_tokens
                .or(request.max_tokens)
                .unwrap_or(0),
        ))
        .max(1)
}

#[tracing::instrument(name = "chat", skip_all, fields(
    request_id = %request_id, model = %request.model, trace_id = tracing::field::Empty,
    gen_ai.operation.name = "chat", gen_ai.request.model = %request.model,
    gen_ai.provider.name = tracing::field::Empty, gen_ai.response.model = tracing::field::Empty,
    gen_ai.response.finish_reasons = tracing::field::Empty,
    gen_ai.usage.input_tokens = tracing::field::Empty, gen_ai.usage.output_tokens = tracing::field::Empty,
    gen_ai.response.time_to_first_chunk = tracing::field::Empty, error.type = tracing::field::Empty,
))]
pub async fn prepare_chat_request(
    state: &AppState,
    token: &Token,
    request: ProviderChatRequest,
    request_id: String,
    interface: TraceInterface,
) -> Result<ChatSession, ChatPrepareError> {
    let allowed: Vec<String> = serde_json::from_str(&token.allowed_models)
        .map_err(|e| ChatPrepareError::new(500, "internal_error", e.to_string()))?;
    if !allowed.is_empty() && !allowed.contains(&request.model) {
        return Err(ChatPrepareError::new(
            403,
            "model_not_allowed",
            "model is not allowed for this token",
        ));
    }
    let routes = state
        .store
        .resolve_model(&request.model)
        .await
        .map_err(|e| ChatPrepareError::new(500, "internal_error", e))?;
    if routes.is_empty() {
        return Err(ChatPrepareError::new(
            404,
            "model_not_found",
            "model has no available provider",
        ));
    }
    let estimated = estimate_token_count(&request);
    let (output, input) = mpsc::channel(32);
    let (metadata_tx, metadata) = oneshot::channel();
    let (done_tx, done) = oneshot::channel();
    let (ready_tx, ready) = oneshot::channel();
    let run = ChatRun {
        state: state.clone(),
        request_id,
        request,
        routes,
        token_id: token.id,
        estimated,
        user_id: token.user_id,
        token_prefix: token.token_prefix.clone(),
        interface,
    };
    tokio::spawn(
        run.run(output, metadata_tx, done_tx, ready_tx)
            .instrument(tracing::Span::current()),
    );
    Ok(ChatSession {
        stream: ReceiverStream::new(input),
        metadata,
        done,
        ready,
    })
}

struct ChatRun {
    state: AppState,
    request_id: String,
    request: ProviderChatRequest,
    routes: Vec<ResolvedProviderRoute>,
    token_id: u64,
    estimated: i64,
    user_id: u64,
    token_prefix: String,
    interface: TraceInterface,
}

impl ChatRun {
    async fn run(
        self,
        output: mpsc::Sender<Result<LMResponsePart, String>>,
        metadata_tx: oneshot::Sender<ProviderResponseMetadata>,
        done_tx: oneshot::Sender<ChatOutcome>,
        ready_tx: oneshot::Sender<Result<(), ChatPrepareError>>,
    ) {
        let start = Instant::now();
        if output.is_closed() {
            return;
        }
        // 不取消正在提交的准入事务；独立任务始终取得预留归属后再响应取消。
        let reservation =
            match quota::check_and_deduct(&self.state.db, self.token_id, self.estimated).await {
                Ok(reservation) => reservation,
                Err(failure) => {
                    let (status, code) = match &failure {
                        quota::QuotaError::Database(_) => (500, "internal_error"),
                        _ => (429, "quota_exceeded"),
                    };
                    let message = failure.to_string();
                    let _ =
                        ready_tx.send(Err(ChatPrepareError::new(status, code, message.clone())));
                    let _ = done_tx.send(ChatOutcome {
                        usage: UsageAccumulator::default(),
                        metadata: ProviderResponseMetadata::default(),
                        status: TraceStatus::Error,
                        error: Some(message),
                    });
                    return;
                }
            };
        let mut ready_tx = Some(ready_tx);
        let mut metadata_tx = Some(metadata_tx);
        let mut metadata = ProviderResponseMetadata::default();
        let mut usage = UsageAccumulator::default();
        let mut first_chunk_at = None;
        let mut ttft = None;
        let mut status = TraceStatus::Error;
        let mut error = None;
        let mut upstream_status = None;
        let mut active_index = 0;
        let mut accepted = false;
        let mut emitted = false;
        let mut response_parts = self.state.capture_content.then(Vec::new);
        let first = &self.routes[0];
        self.state
            .trace_writer
            .send(TraceEvent::Begin(Box::new(BeginTrace {
                request_id: self.request_id.clone(),
                trace_id: current_trace_id(),
                interface: self.interface,
                token_id: reservation.token_id,
                user_id: self.user_id,
                token_prefix: self.token_prefix,
                model: self.request.model.clone(),
                provider_id: first.provider_name.clone(),
                provider_model_id: first.provider_model_name.clone(),
                protocol: crate::observability::genai::provider_name(&first.compatibility).into(),
                estimated_tokens: self.estimated,
                request_messages: self
                    .state
                    .capture_content
                    .then(|| self.request.messages.clone()),
            })));
        for (index, route) in self.routes.iter().enumerate() {
            active_index = index;
            let provider_name = crate::observability::genai::provider_name(&route.compatibility);
            let span = tracing::Span::current();
            span.record("gen_ai.provider.name", provider_name);
            span.record("gen_ai.response.model", route.provider_model_name.as_str());
            self.state.trace_writer.send(TraceEvent::Route {
                request_id: self.request_id.clone(),
                provider_id: route.provider_name.clone(),
                provider_model_id: route.provider_model_name.clone(),
                protocol: provider_name.into(),
            });
            let started = tokio::select! {
                biased;
                _ = output.closed() => { status = TraceStatus::Cancelled; break; }
                started = start_provider(route, self.request.clone()) => started,
            };
            let mut active = match started {
                Ok(active) => active,
                Err(failure) => {
                    let retry = failure.retryable() && index + 1 < self.routes.len();
                    upstream_status = Some(failure.status);
                    error = Some(failure.message.clone());
                    if retry {
                        continue;
                    }
                    if let Some(ready) = ready_tx.take() {
                        let _ = ready.send(Err(failure));
                    }
                    break;
                }
            };
            accepted = true;
            error = None;
            upstream_status = None;
            if let Some(ready) = ready_tx.take() {
                let _ = ready.send(Ok(()));
            }
            status = TraceStatus::Success;
            loop {
                let item = tokio::select! {
                    biased;
                    _ = output.closed() => { status = TraceStatus::Cancelled; break; }
                    item = active.stream.next() => item,
                };
                if let Ok(value) = active.metadata.try_recv() {
                    metadata = value;
                    if let Some(sender) = metadata_tx.take() {
                        let _ = sender.send(metadata.clone());
                    }
                }
                match item {
                    None => break,
                    Some(Err(message)) => {
                        status = TraceStatus::Error;
                        error = Some(message);
                        break;
                    }
                    Some(Ok(part)) => {
                        if first_chunk_at.is_none() {
                            let now = jiff::Timestamp::now();
                            first_chunk_at = Some(now);
                            ttft = Some(start.elapsed());
                            self.state.trace_writer.send(TraceEvent::Streaming {
                                request_id: self.request_id.clone(),
                                first_chunk_at: now,
                            });
                        }
                        if let LMResponsePart::Usage(value) = &part {
                            usage.merge(value);
                        }
                        if let Some(parts) = &mut response_parts {
                            capture_part(parts, &part);
                        }
                        emitted = true;
                        if output.send(Ok(part)).await.is_err() {
                            status = TraceStatus::Cancelled;
                            break;
                        }
                    }
                }
            }
            drop(active);
            if status == TraceStatus::Error && !emitted && index + 1 < self.routes.len() {
                continue;
            }
            break;
        }
        if let Some(ready) = ready_tx.take() {
            let _ = ready.send(Err(ChatPrepareError::new(
                500,
                "internal_error",
                "chat cancelled before startup",
            )));
        }
        if let Some(sender) = metadata_tx.take() {
            let _ = sender.send(metadata.clone());
        }
        // 没有成功启动的失败请求不消耗 tokens；取消/流中断的未知 usage 仍保留预留。
        let actual = usage
            .total()
            .and_then(|n| i64::try_from(n).ok())
            .or_else(|| (!accepted).then_some(0));
        if let Some(actual) = actual
            && let Err(message) =
                quota::adjust_usage(&self.state.db, &reservation, actual - self.estimated).await
        {
            status = TraceStatus::Error;
            error = Some(format!("quota settlement failed: {message}"));
            tracing::error!(request_id = %self.request_id, %message, "quota settlement failed");
        }
        if let Some(message) = &error {
            let _ = output.send(Err(message.clone())).await;
        }
        let route = &self.routes[active_index];
        let provider_name = crate::observability::genai::provider_name(&route.compatibility);
        let span = tracing::Span::current();
        if let Some(value) = usage.input_tokens {
            span.record("gen_ai.usage.input_tokens", value);
        }
        if let Some(value) = usage.output_tokens {
            span.record("gen_ai.usage.output_tokens", value);
        }
        if let Some(value) = &usage.finish_reason {
            span.record("gen_ai.response.finish_reasons", value.as_str());
        }
        if let Some(value) = ttft {
            span.record("gen_ai.response.time_to_first_chunk", value.as_secs_f64());
        }
        let error_type = match status {
            TraceStatus::Error => Some("provider_error"),
            TraceStatus::Cancelled => Some("cancelled"),
            _ => None,
        };
        if let Some(value) = error_type {
            span.record("error.type", value);
        }
        crate::observability::genai::record_finalize(&crate::observability::genai::GenAiFinalize {
            provider_name,
            request_model: self.request.model.clone(),
            response_model: route.provider_model_name.clone(),
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            duration_s: start.elapsed().as_secs_f64(),
            ttft_s: ttft.map(|value| value.as_secs_f64()),
        });
        tracing::info!(request_id = %self.request_id, ?status, total_tokens = ?usage.total(), "chat finalized");
        self.state
            .trace_writer
            .send(TraceEvent::Finalize(Box::new(FinalizeTrace {
                request_id: self.request_id,
                status,
                error_type: error_type.map(str::to_owned),
                error_message: error.clone(),
                upstream_status,
                finish_reason: usage.finish_reason.clone(),
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                reasoning_tokens: usage.reasoning_tokens,
                cached_tokens: usage.cached_tokens,
                total_tokens: usage.total(),
                cost_usd: cost_usd(route, &usage),
                upstream_request_id: metadata.id.clone(),
                first_chunk_at,
                completed_at: jiff::Timestamp::now(),
                ttft_ms: ttft.map(|value| value.as_millis() as i64),
                latency_ms: Some(start.elapsed().as_millis() as i64),
                response_parts,
                day: crate::observability::trace_writer::current_day(),
                token_id: reservation.token_id,
                model: self.request.model,
            })));
        let _ = done_tx.send(ChatOutcome {
            usage,
            metadata,
            status,
            error,
        });
    }
}

fn capture_part(parts: &mut Vec<LMResponsePart>, part: &LMResponsePart) {
    if let (Some(LMResponsePart::Text(previous)), LMResponsePart::Text(next)) =
        (parts.last_mut(), part)
    {
        previous.value.push_str(&next.value);
    } else {
        parts.push(part.clone());
    }
}

pub fn cost_usd(route: &ResolvedProviderRoute, usage: &UsageAccumulator) -> Option<f64> {
    let input = usage.input_tokens?;
    let output = usage.output_tokens?;
    let cached = usage.cached_tokens.unwrap_or(0).min(input);
    fn component(tokens: u64, price: Option<f64>) -> Option<f64> {
        if tokens == 0 {
            return Some(0.0);
        }
        let price = price.filter(|price| price.is_finite() && *price >= 0.0)?;
        Some(tokens as f64 * price / 1_000_000.0)
    }
    if route.input_price_per_1m.is_none()
        && route.output_price_per_1m.is_none()
        && route.cache_read_price_per_1m.is_none()
    {
        return None;
    }
    // reasoning 是 output 的子集；cached 是 input 的子集，不能再额外计一次。
    Some(
        component(input - cached, route.input_price_per_1m)?
            + component(cached, route.cache_read_price_per_1m)?
            + component(output, route.output_price_per_1m)?,
    )
}

fn current_trace_id() -> Option<String> {
    #[cfg(feature = "otel")]
    {
        use opentelemetry::trace::TraceContextExt;
        use tracing_opentelemetry::OpenTelemetrySpanExt;
        let context = tracing::Span::current().context();
        let span = context.span();
        let id = span.span_context();
        if id.is_valid() {
            let value = id.trace_id().to_string();
            tracing::Span::current().record("trace_id", value.as_str());
            return Some(value);
        }
    }
    None
}
