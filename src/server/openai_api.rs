use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
};
use futures_util::stream::Stream;
use ractor::Actor;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;
use tracing::{Instrument, instrument};

use crate::actors::provider::adapters::openai_chat_completions::flatten_thinking_value_for_sse;
use crate::actors::provider::{
    ProviderActor, ProviderChatRequest, ProviderMessage, ProviderResponseMetadata,
    ProviderRuntimeConfig, ProviderStartSignal,
};
use crate::middleware::token_auth::TokenAuth;
use crate::server::AppState;
use crate::types::{
    LMResponsePart, LanguageModelChatMessage, LanguageModelChatMessageRole, LanguageModelInputPart,
    LanguageModelTextPart, LanguageModelTool, LanguageModelToolCallPart,
    LanguageModelToolResultContent, LanguageModelToolResultPart,
};

// ── Auth ── (legacy — used by check_auth only)

#[allow(dead_code)]
#[allow(clippy::result_large_err)]
fn check_auth(state: &AppState, headers: &HeaderMap) -> Result<(), Response> {
    let Some(expected) = &state.auth_token else {
        return Ok(());
    };
    let provided = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    if provided == Some(expected) {
        Ok(())
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "unauthorized"})),
        )
            .into_response())
    }
}

// ── GET /v1/models ──

#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiModelList {
    object: &'static str,
    data: Vec<OpenAiModelEntry>,
}

/// 单个模型的 API 条目（增强版，包含提供者列表和各自的定价/能力）。
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct OpenAiModelEntry {
    id: String,
    object: &'static str,
    created: i64,
    /// 主要提供者（第一个可用提供者）
    owned_by: String,
    /// 模型的标称能力
    capabilities: OpenAiModelCapabilities,
    /// 各提供者的定价和能力覆盖
    providers: Vec<OpenAiModelProviderInfo>,
}

#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct OpenAiModelCapabilities {
    max_input_tokens: u32,
    max_output_tokens: u32,
    tool_calling: bool,
    vision: bool,
    thinking: Option<bool>,
    adaptive_thinking: Option<bool>,
}

#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct OpenAiModelProviderInfo {
    provider_id: String,
    provider_display_name: String,
    /// 提供者覆盖的能力（nullable = 使用模型标称值）
    max_input_tokens: Option<i64>,
    max_output_tokens: Option<i64>,
    tool_calling: Option<bool>,
    vision: Option<bool>,
    thinking: Option<bool>,
    adaptive_thinking: Option<bool>,
    /// 提供者特定定价（每 1M tokens，美元）
    input_price_per_1m: Option<f64>,
    output_price_per_1m: Option<f64>,
    cache_read_price_per_1m: Option<f64>,
    enabled: bool,
    priority: i64,
}

#[instrument(level = "debug", skip(state))]
pub async fn list_models(
    State(state): State<AppState>,
    TokenAuth(token): TokenAuth,
) -> Result<Json<OpenAiModelList>, Response> {
    let all_models = state.store.list_available_models().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": {
                    "message": e,
                    "type": "internal_error",
                    "code": "internal_error"
                }
            })),
        )
            .into_response()
    })?;

    // Filter models based on token's allowed_models
    let allowed: Vec<String> = serde_json::from_str(&token.allowed_models).unwrap_or_default();

    let data = all_models
        .into_iter()
        .filter(|m| {
            if allowed.is_empty() {
                true
            } else {
                allowed.iter().any(|a| a == &m.model_name)
            }
        })
        .map(|m| {
            let owned_by = m
                .providers
                .first()
                .map(|p| p.provider_id.clone())
                .unwrap_or_default();

            let providers = m
                .providers
                .into_iter()
                .map(|p| OpenAiModelProviderInfo {
                    provider_id: p.provider_id,
                    provider_display_name: p.provider_display_name,
                    max_input_tokens: p.max_input_tokens,
                    max_output_tokens: p.max_output_tokens,
                    tool_calling: p.tool_calling,
                    vision: p.vision,
                    thinking: p.thinking,
                    adaptive_thinking: p.adaptive_thinking,
                    input_price_per_1m: p.input_price_per_1m,
                    output_price_per_1m: p.output_price_per_1m,
                    cache_read_price_per_1m: p.cache_read_price_per_1m,
                    enabled: p.enabled,
                    priority: p.priority,
                })
                .collect();

            OpenAiModelEntry {
                id: m.model_name,
                object: "model",
                created: 0,
                owned_by,
                capabilities: OpenAiModelCapabilities {
                    max_input_tokens: m.nominal_capabilities.max_input_tokens,
                    max_output_tokens: m.nominal_capabilities.max_output_tokens,
                    tool_calling: m.nominal_capabilities.tool_calling,
                    vision: m.nominal_capabilities.vision,
                    thinking: m.nominal_capabilities.thinking,
                    adaptive_thinking: m.nominal_capabilities.adaptive_thinking,
                },
                providers,
            }
        })
        .collect();

    Ok(Json(OpenAiModelList {
        object: "list",
        data,
    }))
}

// ── POST /v1/chat/completions ──

#[derive(Debug, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<OpenAiMessage>,
    #[serde(default)]
    pub stream: bool,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f64>,
    #[serde(default)]
    pub stream_options: Option<OpenAiStreamOptions>,
    /// OpenAI 标准工具声明：[{"type":"function","function":{"name","description","parameters"}}]
    #[serde(default)]
    pub tools: Option<Vec<OpenAiTool>>,
    /// "auto" | "none" | {"type":"function","function":{"name":...}}
    #[serde(default)]
    pub tool_choice: Option<serde_json::Value>,
    /// 停止序列：string | string[]
    #[serde(default, deserialize_with = "deserialize_stop")]
    pub stop: Option<Vec<String>>,
    /// 结构化输出 / JSON mode：{"type":"json_object"} | {"type":"json_schema","json_schema":{...}}
    #[serde(default)]
    pub response_format: Option<OpenAiResponseFormat>,
    /// OpenAI 推理强度（o 系列 / gpt-5）："low" | "medium" | "high" | "minimal" | "none" | "xhigh"
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    /// OpenRouter 扩展推理配置：{"effort":...} | {"max_tokens":...} | {"enabled":true} | {"exclude":...}
    #[serde(default)]
    pub reasoning: Option<OpenAiReasoning>,
    /// 确定性采样种子
    #[serde(default)]
    pub seed: Option<i64>,
    /// 频率惩罚（-2.0 ~ 2.0）
    #[serde(default)]
    pub frequency_penalty: Option<f64>,
    /// 存在惩罚（-2.0 ~ 2.0）
    #[serde(default)]
    pub presence_penalty: Option<f64>,
    /// token logit 偏置：{"token_id": bias}
    #[serde(default)]
    pub logit_bias: Option<std::collections::HashMap<String, f64>>,
    /// 推理模型的最大补全 token 数（含 reasoning tokens）
    #[serde(default)]
    pub max_completion_tokens: Option<u32>,
}

/// `response_format` 仅支持 OpenAI 官方两种形态；`text` 视为缺省不单独处理。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAiResponseFormat {
    JsonObject,
    JsonSchema { json_schema: serde_json::Value },
}

/// OpenRouter `reasoning` 对象：effort / max_tokens / enabled / exclude 四选若干。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OpenAiReasoning {
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub exclude: Option<bool>,
}

/// OpenAI `stop` 允许单个字符串或字符串数组。
fn deserialize_stop<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(vec![s])),
        Some(serde_json::Value::Array(arr)) => arr
            .into_iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| D::Error::custom("stop array must contain only strings"))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        _ => Err(D::Error::custom(
            "stop must be a string or an array of strings",
        )),
    }
}

/// OpenAI 工具声明（`type: "function"`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiTool {
    pub r#type: String,
    pub function: OpenAiToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiToolFunction {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parameters: serde_json::Value,
}

impl OpenAiTool {
    /// 转换为协议无关的内部工具定义。
    fn into_internal(self) -> LanguageModelTool {
        LanguageModelTool {
            name: self.function.name,
            description: self.function.description,
            input_schema: self.function.parameters,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAiMessage {
    pub role: String,
    #[serde(default)]
    pub content: OpenAiContent,
    pub name: Option<String>,
    /// assistant 消息携带的工具调用列表
    #[serde(default)]
    pub tool_calls: Option<Vec<OpenAiMessageToolCall>>,
    /// role=tool 时携带，对应要回填的 tool_call id
    #[serde(default)]
    pub tool_call_id: Option<String>,
}

/// assistant 消息中的单个工具调用（OpenAI 格式）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiMessageToolCall {
    pub id: String,
    pub r#type: String,
    pub function: OpenAiMessageToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiMessageToolCallFunction {
    pub name: String,
    /// JSON 字符串形式的参数
    pub arguments: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAiContent {
    String(String),
    Array(Vec<OpenAiContentPart>),
}

impl Default for OpenAiContent {
    fn default() -> Self {
        OpenAiContent::String(String::new())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAiContentPart {
    Text { text: String },
    ImageUrl { image_url: OpenAiImageUrl },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAiImageUrl {
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAiStreamOptions {
    pub include_usage: Option<bool>,
}

#[instrument(
    name = "chat_completions",
    level = "info",
    skip(state, token),
    fields(
        model = %req.model,
        stream = req.stream,
        request_id = tracing::field::Empty,
        // ── GenAI 语义约定属性（PLAN.md §5 O2）──
        gen_ai.operation.name = "chat",
        gen_ai.provider.name = tracing::field::Empty,
        gen_ai.request.model = %req.model,
        gen_ai.request.stream = req.stream,
        gen_ai.response.model = tracing::field::Empty,
        gen_ai.response.finish_reasons = tracing::field::Empty,
        gen_ai.usage.input_tokens = tracing::field::Empty,
        gen_ai.usage.output_tokens = tracing::field::Empty,
        gen_ai.response.time_to_first_chunk = tracing::field::Empty,
        error.type = tracing::field::Empty,
    )
)]
pub async fn chat_completions(
    State(state): State<AppState>,
    TokenAuth(token): TokenAuth,
    request_id: crate::middleware::request_id::RequestId,
    Json(req): Json<ChatCompletionRequest>,
) -> Result<Response, Response> {
    // 将 request_id 记录到本 handler 的请求 span（中间件记录的是连接级 span）。
    tracing::Span::current().record("request_id", request_id.as_str());
    // Check model access
    let allowed: Vec<String> = serde_json::from_str(&token.allowed_models).unwrap_or_default();
    if !allowed.is_empty() && !allowed.iter().any(|a| a == &req.model) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": {
                    "message": format!("model '{}' is not allowed for this token", req.model),
                    "type": "model_access_denied",
                    "code": "model_access_denied"
                }
            })),
        )
            .into_response());
    }

    let routes = state.store.resolve_model(&req.model).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": {
                    "message": e,
                    "type": "internal_error",
                    "code": "internal_error"
                }
            })),
        )
            .into_response()
    })?;
    if routes.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": {
                    "message": format!("model '{}' is not available", req.model),
                    "type": "model_not_found",
                    "code": "model_not_found"
                }
            })),
        )
            .into_response());
    }

    // Take the first (highest priority) route.
    let route = &routes[0];

    // GenAI span 属性：provider 与上游模型名在路由选定后即可确定。
    let genai_provider = crate::observability::genai::provider_name(&route.compatibility);
    let genai_response_model = route.provider_model_name.clone();
    tracing::Span::current().record("gen_ai.provider.name", genai_provider);
    tracing::Span::current().record("gen_ai.response.model", genai_response_model.as_str());

    // 请求开始计时（duration / TTFT 基准）。
    let request_start = std::time::Instant::now();

    // Phase 2: Quota check and deduct (before making upstream call)
    let estimated_tokens = estimate_token_count(&req.messages, req.tools.as_deref());
    if let Err(quota_err) =
        crate::auth::quota::check_and_deduct(&state.db, &token, estimated_tokens).await
    {
        let msg = quota_err.to_string();
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": {
                    "message": msg,
                    "type": "quota_exceeded",
                    "code": "quota_exceeded"
                }
            })),
        )
            .into_response());
    }

    // Convert OpenAI messages to our internal format.
    let messages = convert_messages(&req.messages).await?;

    // ── 请求追踪：INSERT pending 行（PLAN.md §5 O3）──
    // 在 messages move 进 provider_request 前按 Opt-In 开关克隆内容快照。
    let trace_request_messages = if state.capture_content {
        Some(messages.clone())
    } else {
        None
    };
    state
        .trace_writer
        .send(crate::observability::trace_writer::TraceEvent::Begin(
            Box::new(crate::observability::trace_writer::BeginTrace {
                request_id: request_id.as_str().to_string(),
                trace_id: None, // O1 中间件已记录到 span；此处 trace_id 双写留待 otel 集成
                interface: crate::db::models::TraceInterface::OpenAiHttp,
                token_id: token.id,
                user_id: token.user_id,
                token_prefix: token.token_prefix.clone(),
                model: req.model.clone(),
                provider_id: route.provider_name.clone(),
                provider_model_id: route.provider_model_name.clone(),
                protocol: genai_provider.to_string(),
                estimated_tokens,
                request_messages: trace_request_messages,
            }),
        ));

    let provider_config = ProviderRuntimeConfig {
        id: route.provider_name.clone(),
        compatibility: route.compatibility.clone(),
        api_key: route.api_key.clone(),
        base_url: route.base_url.clone(),
        compat_settings: route.compat_settings.clone(),
    };

    let reasoning = merge_reasoning(req.reasoning_effort.as_deref(), req.reasoning.as_ref());

    let provider_request = ProviderChatRequest {
        model: route.provider_model_name.clone(),
        messages,
        tools: req
            .tools
            .map(|tools| tools.into_iter().map(OpenAiTool::into_internal).collect()),
        tool_choice: req.tool_choice.clone(),
        temperature: req.temperature,
        max_tokens: req.max_tokens,
        top_p: req.top_p,
        stop: req.stop.clone(),
        response_format: req.response_format.clone().map(Into::into),
        reasoning,
        seed: req.seed,
        frequency_penalty: req.frequency_penalty,
        presence_penalty: req.presence_penalty,
        logit_bias: req.logit_bias.clone(),
        max_completion_tokens: req.max_completion_tokens,
    };

    // Spawn provider actor and get stream.
    let (provider_ref, provider_handle) = Actor::spawn(None, ProviderActor, provider_config)
        .await
        .map_err(|e| internal_error(&e.to_string()))?;

    let (metadata_tx, metadata_rx) = tokio::sync::oneshot::channel::<ProviderResponseMetadata>();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel::<ProviderStartSignal>();

    let stream = ractor::call_t!(
        provider_ref,
        |reply| { ProviderMessage::ChatRequest(provider_request, reply, metadata_tx, started_tx) },
        30_000
    )
    .map_err(|e| internal_error(&e.to_string()))?
    .map_err(|e| internal_error(&e))?;

    // Clean up provider actor when stream ends.
    let cleanup_handle = provider_handle;
    let cleanup_ref = provider_ref;

    // #13：等待启动阶段信号——上游非 2xx 或请求未发出时，直接返回对应状态码而非 500。
    match started_rx.await {
        Ok(Ok(status)) if (200..300).contains(&(status as usize)) => {}
        Ok(Ok(status)) => {
            // 上游非 2xx：错误体会以 Err 进入流内；这里先取出再返回透传响应。
            let mut stream = stream;
            let mut message = format!("upstream returned status {status}");
            let mut code: Option<String> = None;
            while let Some(item) = stream.next().await {
                match item {
                    Err(e) => {
                        message = e;
                        break;
                    }
                    Ok(_) => continue,
                }
            }
            cleanup_ref.stop(None);
            let _ = cleanup_handle.await;
            // 请求追踪：error finalize（PLAN.md §5 O3）——避免 pending 行卡住。
            send_error_finalize(
                &state,
                &request_id,
                "upstream_error",
                Some(status),
                Some(&message),
                token.id,
                &req.model,
                request_start,
            );
            return Err(upstream_error_response(status, code.take(), &message));
        }
        Ok(Err(err)) => {
            // 请求未成功发出（网络错误等）：502 Bad Gateway
            cleanup_ref.stop(None);
            let _ = cleanup_handle.await;
            send_error_finalize(
                &state,
                &request_id,
                "network_error",
                Some(502),
                Some(&err.message),
                token.id,
                &req.model,
                request_start,
            );
            return Err(upstream_error_response(502, err.code.clone(), &err.message));
        }
        Err(_) => {
            // started_tx 被 drop（理论上不该发生）：回退按流内错误处理
        }
    }

    if req.stream {
        let usage_handle = UsageHandle::default();
        let ttft_slot: TtftSlot = Default::default();
        let sse_stream = stream_to_sse(
            stream,
            req.model.clone(),
            usage_handle.clone(),
            metadata_rx,
            ttft_slot.clone(),
        );

        // Spawn cleanup after stream is consumed.
        // span context 断点修复（PLAN.md §5 O1）：spawn 的流消费任务在响应返回后才被
        // poll，此时请求 span 已关闭；捕获当前 span 并 .instrument() 挂回，使 SSE 阶段
        // 的结算日志保留 request_id / model 等上下文字段。
        let settle_state = state.clone();
        let settle_ctx = crate::auth::quota::TokenQuotaContext::from_token(&token);
        let genai_request_model = req.model.clone();
        let trace_request_id = request_id.as_str().to_string();
        let genai_response_model_clone = genai_response_model.clone();
        tokio::spawn(
            async move {
                cleanup_ref.stop(None);
                let _ = cleanup_handle.await;
                let usage = usage_handle.lock().await.clone();
                settle_quota_with_actual_usage(
                    &settle_state,
                    &settle_ctx,
                    estimated_tokens,
                    &usage,
                )
                .await;

                // GenAI finalize：TTFT + usage 记录到请求 span 并投影 metrics。
                let ttft_s = ttft_slot
                    .lock()
                    .await
                    .map(|t| t.duration_since(request_start).as_secs_f64());
                let span = tracing::Span::current();
                if let Some(ttft) = ttft_s {
                    span.record("gen_ai.response.time_to_first_chunk", ttft);
                }
                if let Some(reason) = usage.finish_reason.as_deref() {
                    span.record("gen_ai.response.finish_reasons", reason);
                }
                if let Some(input) = usage.input_tokens {
                    span.record("gen_ai.usage.input_tokens", input);
                }
                if let Some(output) = usage.output_tokens {
                    span.record("gen_ai.usage.output_tokens", output);
                }
                crate::observability::genai::record_finalize(
                    &crate::observability::genai::GenAiFinalize {
                        provider_name: genai_provider,
                        request_model: genai_request_model.clone(),
                        response_model: genai_response_model_clone.clone(),
                        input_tokens: usage.input_tokens,
                        output_tokens: usage.output_tokens,
                        duration_s: request_start.elapsed().as_secs_f64(),
                        ttft_s,
                    },
                );

                // ── 请求追踪：UPDATE 终态 + usage_daily rollup（PLAN.md §5 O3）──
                // 流式响应 parts 不在热路径逐条聚合（增大内存与延迟），response_parts 留空；
                // 内容快照在 O5 详情页按需从上游重放或后续增强采集。此处仅落结构化事实。
                let completed_at = jiff::Timestamp::now();
                let latency_ms = request_start.elapsed().as_millis() as i64;
                let ttft_ms = ttft_s.map(|s| (s * 1000.0) as i64);
                // first_chunk_at 由 ttft_ms 反推（精度足够，避免额外 Instant→Timestamp 转换）。
                let first_chunk_at = if ttft_ms.is_some() {
                    Some(completed_at)
                } else {
                    None
                };
                settle_state.trace_writer.send(
                    crate::observability::trace_writer::TraceEvent::Finalize(Box::new(
                        crate::observability::trace_writer::FinalizeTrace {
                            request_id: trace_request_id,
                            status: crate::db::models::TraceStatus::Success,
                            error_type: None,
                            error_message: None,
                            upstream_status: None,
                            finish_reason: usage.finish_reason.clone(),
                            input_tokens: usage.input_tokens,
                            output_tokens: usage.output_tokens,
                            reasoning_tokens: usage.reasoning_tokens,
                            cached_tokens: usage.cached_tokens,
                            total_tokens: usage.total_tokens,
                            cost_usd: None,            // O4 成本计算后回填
                            upstream_request_id: None, // SSE 路径 metadata 已被 stream_to_sse 消费
                            first_chunk_at,
                            completed_at,
                            ttft_ms,
                            latency_ms: Some(latency_ms),
                            response_parts: None,
                            day: crate::observability::trace_writer::current_day(),
                            token_id: settle_ctx.token_id,
                            model: genai_request_model,
                        },
                    )),
                );
            }
            .instrument(tracing::Span::current()),
        );

        Ok(Sse::new(sse_stream).into_response())
    } else {
        // Non-streaming: collect all chunks and concatenate.
        let mut stream = stream;
        let mut content = String::new();
        let mut reasoning_content = String::new();
        let mut tool_calls: Vec<serde_json::Value> = Vec::new();
        let mut usage_acc = UsageAccumulator::default();
        while let Some(item) = stream.next().await {
            match item {
                Ok(LMResponsePart::Text(t)) => content.push_str(&t.value),
                Ok(LMResponsePart::Thinking(t)) => {
                    let text = crate::actors::provider::adapters::openai_chat_completions::flatten_thinking_value_for_sse(&t.value);
                    reasoning_content.push_str(&text);
                }
                Ok(LMResponsePart::ToolCall(tc)) => {
                    tool_calls.push(serde_json::json!({
                        "id": tc.call_id,
                        "type": "function",
                        "function": {
                            "name": tc.name,
                            "arguments": serde_json::to_string(&tc.input).unwrap_or_default(),
                        }
                    }));
                }
                Ok(LMResponsePart::Usage(u)) => usage_acc.merge(&u),
                Ok(_) => {}
                Err(e) => {
                    cleanup_ref.stop(None);
                    let _ = cleanup_handle.await;
                    // GenAI error.type（流内错误）：低基数错误类别标识。
                    tracing::Span::current().record("error.type", "stream_error");
                    // 请求追踪：error finalize（PLAN.md §5 O3）。
                    send_error_finalize(
                        &state,
                        &request_id,
                        "stream_error",
                        None,
                        Some(&e),
                        token.id,
                        &req.model,
                        request_start,
                    );
                    return Err(internal_error(&e));
                }
            }
        }
        cleanup_ref.stop(None);
        let _ = cleanup_handle.await;

        // 按真实 usage 结算配额：多退少补（相对预估）
        let settle_ctx = crate::auth::quota::TokenQuotaContext::from_token(&token);
        settle_quota_with_actual_usage(&state, &settle_ctx, estimated_tokens, &usage_acc).await;

        // GenAI finalize（非流式，无 TTFT）：usage/finish_reason 记录到 span 并投影 metrics。
        let span = tracing::Span::current();
        if let Some(reason) = usage_acc.finish_reason.as_deref() {
            span.record("gen_ai.response.finish_reasons", reason);
        }
        if let Some(input) = usage_acc.input_tokens {
            span.record("gen_ai.usage.input_tokens", input);
        }
        if let Some(output) = usage_acc.output_tokens {
            span.record("gen_ai.usage.output_tokens", output);
        }
        crate::observability::genai::record_finalize(&crate::observability::genai::GenAiFinalize {
            provider_name: genai_provider,
            request_model: req.model.clone(),
            response_model: genai_response_model.clone(),
            input_tokens: usage_acc.input_tokens,
            output_tokens: usage_acc.output_tokens,
            duration_s: request_start.elapsed().as_secs_f64(),
            ttft_s: None,
        });

        let upstream = metadata_rx.await.unwrap_or_default();
        let id = upstream
            .id
            .unwrap_or_else(|| "chatcmpl-llm-bridge".to_string());
        let created = upstream.created.unwrap_or(0);

        // ── 请求追踪：UPDATE 终态 + usage_daily rollup（PLAN.md §5 O3）──
        // 非流式 response_parts 不采集（完整响应已作为 JSON 返回客户端，可经 replay 获取）。
        let completed_at = jiff::Timestamp::now();
        let latency_ms = request_start.elapsed().as_millis() as i64;
        state
            .trace_writer
            .send(crate::observability::trace_writer::TraceEvent::Finalize(
                Box::new(crate::observability::trace_writer::FinalizeTrace {
                    request_id: request_id.as_str().to_string(),
                    status: crate::db::models::TraceStatus::Success,
                    error_type: None,
                    error_message: None,
                    upstream_status: None,
                    finish_reason: usage_acc.finish_reason.clone(),
                    input_tokens: usage_acc.input_tokens,
                    output_tokens: usage_acc.output_tokens,
                    reasoning_tokens: usage_acc.reasoning_tokens,
                    cached_tokens: usage_acc.cached_tokens,
                    total_tokens: usage_acc.total_tokens,
                    cost_usd: None,
                    upstream_request_id: Some(id.clone()),
                    first_chunk_at: None,
                    completed_at,
                    ttft_ms: None,
                    latency_ms: Some(latency_ms),
                    response_parts: None,
                    day: crate::observability::trace_writer::current_day(),
                    token_id: token.id,
                    model: req.model.clone(),
                }),
            ));

        let has_tool_calls = !tool_calls.is_empty();
        let mut message = serde_json::json!({
            "role": "assistant",
            "content": content,
        });
        if !reasoning_content.is_empty() {
            message["reasoning_content"] = serde_json::Value::String(reasoning_content);
        }
        if has_tool_calls {
            message["tool_calls"] = serde_json::Value::Array(tool_calls);
        }
        let finish_reason = usage_acc
            .finish_reason
            .as_deref()
            .unwrap_or(if has_tool_calls { "tool_calls" } else { "stop" });

        let mut response = serde_json::json!({
            "id": id,
            "object": "chat.completion",
            "created": created,
            "model": req.model,
            "choices": [{
                "index": 0,
                "message": message,
                "finish_reason": finish_reason
            }]
        });
        if let Some(usage_json) = usage_acc.to_openai_usage() {
            response["usage"] = usage_json;
        }

        Ok(Json(response).into_response())
    }
}

/// 合并 OpenAI `reasoning_effort` 与 OpenRouter `reasoning` 对象为协议无关配置。
fn merge_reasoning(
    reasoning_effort: Option<&str>,
    reasoning: Option<&OpenAiReasoning>,
) -> Option<crate::types::LanguageModelReasoningConfig> {
    let effort = reasoning_effort
        .map(str::to_string)
        .or_else(|| reasoning.and_then(|r| r.effort.clone()));
    let max_tokens = reasoning.and_then(|r| r.max_tokens);

    // OpenRouter reasoning.enabled=true 但未给 effort/max_tokens 时，默认 medium
    let effort = effort.or_else(|| match reasoning {
        Some(r) if r.enabled == Some(true) => Some("medium".to_string()),
        _ => None,
    });

    if effort.is_none() && max_tokens.is_none() {
        return None;
    }

    Some(crate::types::LanguageModelReasoningConfig { effort, max_tokens })
}

impl From<OpenAiResponseFormat> for crate::types::LanguageModelResponseFormat {
    fn from(value: OpenAiResponseFormat) -> Self {
        match value {
            OpenAiResponseFormat::JsonObject => {
                crate::types::LanguageModelResponseFormat::JsonObject
            }
            OpenAiResponseFormat::JsonSchema { json_schema } => {
                crate::types::LanguageModelResponseFormat::JsonSchema { json_schema }
            }
        }
    }
}

/// 跨多个 Usage part 聚合用量与 finish_reason。
#[derive(Default, Clone)]
struct UsageAccumulator {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
    reasoning_tokens: Option<u64>,
    cached_tokens: Option<u64>,
    finish_reason: Option<String>,
}

impl UsageAccumulator {
    fn merge(&mut self, u: &crate::types::LanguageModelUsagePart) {
        if u.input_tokens.is_some() {
            self.input_tokens = u.input_tokens;
        }
        if u.output_tokens.is_some() {
            self.output_tokens = u.output_tokens;
        }
        if u.total_tokens.is_some() {
            self.total_tokens = u.total_tokens;
        }
        if u.reasoning_tokens.is_some() {
            self.reasoning_tokens = u.reasoning_tokens;
        }
        if u.cached_tokens.is_some() {
            self.cached_tokens = u.cached_tokens;
        }
        if u.finish_reason.is_some() {
            self.finish_reason = u.finish_reason.clone();
        }
    }

    fn total(&self) -> Option<i64> {
        self.total_tokens
            .or(match (self.input_tokens, self.output_tokens) {
                (Some(i), Some(o)) => Some(i + o),
                _ => None,
            })
            .map(|t| t as i64)
    }

    fn to_openai_usage(&self) -> Option<serde_json::Value> {
        let input = self.input_tokens?;
        let output = self.output_tokens.unwrap_or(0);
        let mut usage = serde_json::json!({
            "prompt_tokens": input,
            "completion_tokens": output,
            "total_tokens": self.total_tokens.unwrap_or(input + output),
        });
        if let Some(reasoning) = self.reasoning_tokens {
            usage["completion_tokens_details"] =
                serde_json::json!({ "reasoning_tokens": reasoning });
        }
        if let Some(cached) = self.cached_tokens {
            usage["prompt_tokens_details"] = serde_json::json!({ "cached_tokens": cached });
        }
        Some(usage)
    }
}

/// 上游返回真实 usage 后，与预估扣减对账：多退少补。
async fn settle_quota_with_actual_usage(
    state: &AppState,
    ctx: &crate::auth::quota::TokenQuotaContext,
    estimated_tokens: i64,
    usage: &UsageAccumulator,
) {
    let Some(actual_total) = usage.total() else {
        return;
    };
    let delta = actual_total - estimated_tokens;
    if delta == 0 {
        return;
    }
    if let Err(e) = crate::auth::quota::adjust_usage(&state.db, ctx, delta).await {
        tracing::warn!(
            token_id = ctx.token_id,
            delta,
            "failed to settle quota with actual usage: {e}"
        );
    }
}

/// 流式路径共享的 usage 累积句柄。
type UsageHandle = std::sync::Arc<tokio::sync::Mutex<UsageAccumulator>>;

/// 流式路径共享的首 chunk 时间槽（`None` = 尚未产生首 chunk）。
type TtftSlot = std::sync::Arc<tokio::sync::Mutex<Option<std::time::Instant>>>;

/// 用于在流式 SSE 中共享上游 metadata（id/created）与角色发送状态。
struct SseSharedState {
    usage_acc: UsageHandle,
    metadata_rx: tokio::sync::oneshot::Receiver<ProviderResponseMetadata>,
    upstream: Option<ProviderResponseMetadata>,
    role_sent: bool,
}

impl SseSharedState {
    fn upstream_id(&mut self) -> String {
        if self.upstream.is_none() {
            self.upstream = self.metadata_rx.try_recv().ok();
        }
        self.upstream
            .as_ref()
            .and_then(|m| m.id.clone())
            .unwrap_or_else(|| "chatcmpl-llm-bridge".to_string())
    }

    fn upstream_created(&mut self) -> u64 {
        if self.upstream.is_none() {
            self.upstream = self.metadata_rx.try_recv().ok();
        }
        self.upstream.as_ref().and_then(|m| m.created).unwrap_or(0)
    }
}

fn stream_to_sse(
    stream: impl Stream<Item = Result<LMResponsePart, String>> + Send + 'static,
    model: String,
    usage_handle: UsageHandle,
    metadata_rx: tokio::sync::oneshot::Receiver<ProviderResponseMetadata>,
    ttft_slot: TtftSlot,
) -> impl Stream<Item = Result<Event, axum::Error>> + Send + 'static {
    let shared = std::sync::Arc::new(tokio::sync::Mutex::new(SseSharedState {
        usage_acc: usage_handle,
        metadata_rx,
        upstream: None,
        role_sent: false,
    }));

    // Map each item, then append a [DONE] sentinel at the end.
    let mapped = stream.then(move |item| {
        let shared = shared.clone();
        let model = model.clone();
        let ttft_slot = ttft_slot.clone();
        async move {
            // TTFT（PLAN.md §5 O2）：上游产生首个 item 即视为首 chunk，一次性写入。
            if let Ok(mut slot) = ttft_slot.try_lock()
                && slot.is_none()
            {
                *slot = Some(std::time::Instant::now());
            }
            map_part_to_sse(item, &model, shared).await
        }
    });

    use futures_util::stream;
    let done = stream::once(async { Ok(Event::default().data("[DONE]")) });
    mapped.chain(done)
}

async fn map_part_to_sse(
    item: Result<LMResponsePart, String>,
    model: &str,
    shared: std::sync::Arc<tokio::sync::Mutex<SseSharedState>>,
) -> Result<Event, axum::Error> {
    match item {
        Ok(part) => {
            let mut delta = serde_json::Map::new();
            let mut usage_json: Option<serde_json::Value> = None;

            let finish_reason = match &part {
                LMResponsePart::Text(t) => {
                    delta.insert(
                        "content".to_string(),
                        serde_json::Value::String(t.value.clone()),
                    );
                    None
                }
                LMResponsePart::Thinking(t) => {
                    // Reasoning/thinking content — exposed as `reasoning_content` per DeepSeek / OpenAI extended format.
                    let text = flatten_thinking_value_for_sse(&t.value);
                    delta.insert(
                        "reasoning_content".to_string(),
                        serde_json::Value::String(text),
                    );
                    None
                }
                LMResponsePart::ToolCall(tc) => {
                    // 语义基准：ToolCall 累积完整后一次性发射（含完整 arguments）。
                    // finish_reason 不由此处标注——上游 Usage part 会携带真实的
                    // "tool_calls" finish_reason 单独成 chunk。
                    delta.insert(
                        "tool_calls".to_string(),
                        serde_json::json!([{
                            "index": 0,
                            "id": tc.call_id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": serde_json::to_string(&tc.input)
                                    .unwrap_or_default(),
                            }
                        }]),
                    );
                    None
                }
                LMResponsePart::Usage(u) => {
                    // 聚合供流后结算
                    let guard = shared.lock().await;
                    guard.usage_acc.lock().await.merge(u);
                    // OpenAI include_usage 格式：choices 为空数组的 usage-only chunk
                    if u.input_tokens.is_some() {
                        usage_json = Some(serde_json::json!({
                            "prompt_tokens": u.input_tokens,
                            "completion_tokens": u.output_tokens.unwrap_or(0),
                            "total_tokens": u.total_tokens,
                        }));
                    }
                    // finish_reason 由下方 chunk 的 choices 携带
                    u.finish_reason.as_deref()
                }
                _ => None,
            };

            // usage-only chunk（OpenAI 格式：choices 为空）
            if let Some(usage) = usage_json {
                let mut guard = shared.lock().await;
                let id = guard.upstream_id();
                let created = guard.upstream_created();
                let chunk = serde_json::json!({
                    "id": id,
                    "object": "chat.completion.chunk",
                    "created": created,
                    "model": model,
                    "choices": [],
                    "usage": usage,
                });
                return Ok(Event::default().data(serde_json::to_string(&chunk).unwrap_or_default()));
            }

            let mut guard = shared.lock().await;
            let id = guard.upstream_id();
            let created = guard.upstream_created();

            // OpenAI 规范：首包必须携带 role: "assistant"
            if !guard.role_sent {
                delta.insert(
                    "role".to_string(),
                    serde_json::Value::String("assistant".to_string()),
                );
                guard.role_sent = true;
            }

            let chunk = serde_json::json!({
                "id": id,
                "object": "chat.completion.chunk",
                "created": created,
                "model": model,
                "choices": [{
                    "index": 0,
                    "delta": delta,
                    "finish_reason": finish_reason,
                }]
            });

            Ok(Event::default().data(serde_json::to_string(&chunk).unwrap_or_default()))
        }
        Err(e) => {
            // #13：流式错误 chunk 携带 code 字段（P3 #18 错误格式部分）
            let error_chunk = serde_json::json!({
                "error": {
                    "message": e,
                    "type": "provider_error",
                    "code": "provider_error"
                }
            });
            Ok(Event::default().data(serde_json::to_string(&error_chunk).unwrap_or_default()))
        }
    }
}

/// #11：单张图片抓取上限 10 MiB，每条消息最多 8 个图片 part。
const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const MAX_IMAGE_PARTS_PER_MESSAGE: usize = 8;

async fn convert_messages(
    messages: &[OpenAiMessage],
) -> Result<Vec<LanguageModelChatMessage>, Response> {
    let mut out = Vec::with_capacity(messages.len());
    for msg in messages {
        out.push(convert_single_message(msg).await?);
    }
    Ok(out)
}

#[allow(clippy::result_large_err)]
async fn convert_single_message(msg: &OpenAiMessage) -> Result<LanguageModelChatMessage, Response> {
    let role = match msg.role.as_str() {
        "user" => LanguageModelChatMessageRole::User,
        "assistant" => LanguageModelChatMessageRole::Assistant,
        "system" => LanguageModelChatMessageRole::System,
        "developer" => LanguageModelChatMessageRole::Developer,
        _ => LanguageModelChatMessageRole::User,
    };

    // role=tool → ToolResult part
    if msg.role == "tool" {
        let call_id = msg.tool_call_id.clone().unwrap_or_default();
        let text = content_to_text(&msg.content);
        return Ok(LanguageModelChatMessage {
            role: LanguageModelChatMessageRole::User,
            content: vec![LanguageModelInputPart::ToolResult(
                LanguageModelToolResultPart {
                    call_id,
                    content: vec![LanguageModelToolResultContent::Text(
                        LanguageModelTextPart { value: text },
                    )],
                },
            )],
            name: msg.name.clone(),
        });
    }

    let mut parts: Vec<LanguageModelInputPart> = Vec::new();

    // assistant 携带 tool_calls → ToolCall parts
    if let Some(tool_calls) = &msg.tool_calls {
        for tc in tool_calls {
            let input = serde_json::from_str(&tc.function.arguments)
                .unwrap_or(serde_json::Value::String(tc.function.arguments.clone()));
            parts.push(LanguageModelInputPart::ToolCall(
                LanguageModelToolCallPart {
                    call_id: tc.id.clone(),
                    name: tc.function.name.clone(),
                    input,
                },
            ));
        }
    }

    // 文本 + 多模态图片 part（保持原始顺序：先文本再图片会丢序，因此改为按 part 顺序展开）
    match &msg.content {
        OpenAiContent::String(s) => {
            if !s.is_empty() || parts.is_empty() {
                parts.insert(
                    0,
                    LanguageModelInputPart::Text(LanguageModelTextPart { value: s.clone() }),
                );
            }
        }
        OpenAiContent::Array(content_parts) => {
            let mut image_count = 0usize;
            for part in content_parts {
                match part {
                    OpenAiContentPart::Text { text } => {
                        parts.push(LanguageModelInputPart::Text(LanguageModelTextPart {
                            value: text.clone(),
                        }));
                    }
                    OpenAiContentPart::ImageUrl { image_url } => {
                        image_count += 1;
                        if image_count > MAX_IMAGE_PARTS_PER_MESSAGE {
                            return Err(bad_request(&format!(
                                "too many image parts in one message (max {MAX_IMAGE_PARTS_PER_MESSAGE})"
                            )));
                        }
                        let data_part = resolve_image_url(&image_url.url).await?;
                        parts.push(LanguageModelInputPart::Data(data_part));
                    }
                }
            }
            if parts.is_empty() {
                parts.push(LanguageModelInputPart::Text(LanguageModelTextPart {
                    value: String::new(),
                }));
            }
        }
    }

    Ok(LanguageModelChatMessage {
        role,
        content: parts,
        name: msg.name.clone(),
    })
}

/// #11：将 OpenAI image_url 解析为内部 LanguageModelDataPart。
/// 支持 `data:<mime>;base64,<data>` 与 http(s) URL（抓取字节流）。
async fn resolve_image_url(url: &str) -> Result<crate::types::LanguageModelDataPart, Response> {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD as BASE64;

    if let Some(data_uri) = url.strip_prefix("data:") {
        // data:<mime>;base64,<payload>
        let (meta, payload) = data_uri
            .split_once(',')
            .ok_or_else(|| bad_request("invalid data URI for image_url"))?;
        let mime_type = meta.strip_suffix(";base64").unwrap_or(meta).to_string();
        if !mime_type.starts_with("image/") {
            return Err(bad_request(&format!(
                "unsupported data URI mime type for image_url: {mime_type}"
            )));
        }
        let data = BASE64
            .decode(payload.trim())
            .map_err(|e| bad_request(&format!("invalid base64 in image_url data URI: {e}")))?;
        if data.len() > MAX_IMAGE_BYTES {
            return Err(bad_request(&format!(
                "image exceeds {} MiB limit",
                MAX_IMAGE_BYTES / 1024 / 1024
            )));
        }
        return Ok(crate::types::LanguageModelDataPart { mime_type, data });
    }

    if url.starts_with("http://") || url.starts_with("https://") {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .user_agent(concat!(
                env!("CARGO_PKG_NAME"),
                "/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .map_err(|e| internal_error(&format!("failed to build http client: {e}")))?;
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| bad_request(&format!("failed to fetch image_url: {e}")))?;
        if !response.status().is_success() {
            return Err(bad_request(&format!(
                "failed to fetch image_url: HTTP {}",
                response.status()
            )));
        }
        let mime_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(';').next().unwrap_or(s).trim().to_string())
            .filter(|s| s.starts_with("image/"))
            .unwrap_or_else(|| "image/png".to_string());
        let data = response
            .bytes()
            .await
            .map_err(|e| bad_request(&format!("failed to read image_url body: {e}")))?;
        if data.len() > MAX_IMAGE_BYTES {
            return Err(bad_request(&format!(
                "image exceeds {} MiB limit",
                MAX_IMAGE_BYTES / 1024 / 1024
            )));
        }
        return Ok(crate::types::LanguageModelDataPart {
            mime_type,
            data: data.to_vec(),
        });
    }

    Err(bad_request(
        "unsupported image_url scheme (expected data: or http(s):)",
    ))
}

fn bad_request(msg: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({
            "error": {
                "message": msg,
                "type": "invalid_request_error",
                "code": "invalid_request_error"
            }
        })),
    )
        .into_response()
}

fn content_to_text(content: &OpenAiContent) -> String {
    match content {
        OpenAiContent::String(s) => s.clone(),
        OpenAiContent::Array(parts) => parts
            .iter()
            .filter_map(|p| match p {
                OpenAiContentPart::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn internal_error(msg: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "error": {
                "message": msg,
                "type": "internal_error",
                "code": "internal_error"
            }
        })),
    )
        .into_response()
}

/// 请求追踪 error finalize（PLAN.md §5 O3）。
///
/// 在上游错误 / 网络错误 / 流内错误路径调用，将 pending 行 UPDATE 为 error 终态，
/// 避免崩溃或上游失败留下「卡住」的 pending 记录。不含 usage（错误请求通常无 usage）。
#[allow(clippy::too_many_arguments)]
fn send_error_finalize(
    state: &AppState,
    request_id: &crate::middleware::request_id::RequestId,
    error_type: &str,
    upstream_status: Option<u16>,
    error_message: Option<&str>,
    token_id: u64,
    model: &str,
    request_start: std::time::Instant,
) {
    let completed_at = jiff::Timestamp::now();
    let latency_ms = request_start.elapsed().as_millis() as i64;
    state
        .trace_writer
        .send(crate::observability::trace_writer::TraceEvent::Finalize(
            Box::new(crate::observability::trace_writer::FinalizeTrace {
                request_id: request_id.as_str().to_string(),
                status: crate::db::models::TraceStatus::Error,
                error_type: Some(error_type.to_string()),
                error_message: error_message.map(str::to_string),
                upstream_status,
                finish_reason: None,
                input_tokens: None,
                output_tokens: None,
                reasoning_tokens: None,
                cached_tokens: None,
                total_tokens: None,
                cost_usd: None,
                upstream_request_id: None,
                first_chunk_at: None,
                completed_at,
                ttft_ms: None,
                latency_ms: Some(latency_ms),
                response_parts: None,
                day: crate::observability::trace_writer::current_day(),
                token_id,
                model: model.to_string(),
            }),
        ));
}

/// #13：构造透传上游语义状态码的错误响应。
fn upstream_error_response(status: u16, code: Option<String>, message: &str) -> Response {
    let status_code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    let error_type = match status {
        401 => "authentication_error",
        402 => "billing_error",
        429 => "rate_limit_exceeded",
        s if s >= 500 => "upstream_error",
        _ => "invalid_request_error",
    };
    (
        status_code,
        Json(serde_json::json!({
            "error": {
                "message": message,
                "type": error_type,
                "code": code.unwrap_or_else(|| error_type.to_string()),
            }
        })),
    )
        .into_response()
}

/// Rough token count estimate for quota pre-check.
/// Uses character count / 4 as a rough heuristic (common for English text).
/// 工具定义 JSON 长度也计入，避免带 tools 时低估。
fn estimate_token_count(messages: &[OpenAiMessage], tools: Option<&[OpenAiTool]>) -> i64 {
    let message_chars: usize = messages
        .iter()
        .map(|m| match &m.content {
            OpenAiContent::String(s) => s.len(),
            OpenAiContent::Array(parts) => parts
                .iter()
                .filter_map(|p| match p {
                    OpenAiContentPart::Text { text } => Some(text.len()),
                    _ => None,
                })
                .sum(),
        })
        .sum();
    let tool_chars: usize = tools
        .map(|t| {
            t.iter()
                .map(|tool| {
                    tool.function.name.len()
                        + tool.function.description.as_deref().unwrap_or("").len()
                        + tool.function.parameters.to_string().len()
                })
                .sum()
        })
        .unwrap_or(0);
    ((message_chars + tool_chars) / 4) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD as BASE64;

    #[tokio::test]
    async fn resolve_image_url_decodes_data_uri() {
        let raw = b"\x89PNG\r\n\x1a\n";
        let url = format!("data:image/png;base64,{}", BASE64.encode(raw));
        let part = resolve_image_url(&url).await.unwrap();
        assert_eq!(part.mime_type, "image/png");
        assert_eq!(part.data, raw);
    }

    #[tokio::test]
    async fn resolve_image_url_rejects_non_image_data_uri() {
        let url = format!("data:text/html;base64,{}", BASE64.encode(b"<html>"));
        let result = resolve_image_url(&url).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn resolve_image_url_rejects_bad_base64() {
        let result = resolve_image_url("data:image/png;base64,!!!not-base64!!!").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn resolve_image_url_rejects_unknown_scheme() {
        let result = resolve_image_url("file:///etc/passwd").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn convert_messages_maps_image_url_part() {
        let raw = b"fake-image";
        let messages = vec![OpenAiMessage {
            role: "user".to_string(),
            content: OpenAiContent::Array(vec![
                OpenAiContentPart::Text {
                    text: "describe this".to_string(),
                },
                OpenAiContentPart::ImageUrl {
                    image_url: OpenAiImageUrl {
                        url: format!("data:image/jpeg;base64,{}", BASE64.encode(raw)),
                    },
                },
            ]),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }];
        let converted = convert_messages(&messages).await.unwrap();
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].content.len(), 2);
        assert!(matches!(
            &converted[0].content[0],
            LanguageModelInputPart::Text(t) if t.value == "describe this"
        ));
        match &converted[0].content[1] {
            LanguageModelInputPart::Data(d) => {
                assert_eq!(d.mime_type, "image/jpeg");
                assert_eq!(d.data, raw);
            }
            other => panic!("expected Data part, got {other:?}"),
        }
    }
}
