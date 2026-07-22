//! OpenTelemetry GenAI 语义约定的 span 属性键与 metrics 投影（PLAN.md §5 O2）。
//!
//! 属性键常量无条件编译（供 span `record` 使用，无 otel 时 stdout 日志同样可携带
//! `gen_ai.*` 字段，便于三方互查）；metrics 仪器与投影仅在 `otel` feature 下编译——
//! 未启用时 `record_finalize` 为空函数，调用点零开销内联。

use crate::config::models::ProviderCompatibility;

// ── GenAI span 属性键（OpenTelemetry 语义约定，development 稳定性）──
pub const ATTR_OPERATION_NAME: &str = "gen_ai.operation.name";
pub const ATTR_PROVIDER_NAME: &str = "gen_ai.provider.name";
pub const ATTR_REQUEST_MODEL: &str = "gen_ai.request.model";
pub const ATTR_REQUEST_STREAM: &str = "gen_ai.request.stream";
pub const ATTR_RESPONSE_MODEL: &str = "gen_ai.response.model";
pub const ATTR_RESPONSE_FINISH_REASONS: &str = "gen_ai.response.finish_reasons";
pub const ATTR_USAGE_INPUT_TOKENS: &str = "gen_ai.usage.input_tokens";
pub const ATTR_USAGE_OUTPUT_TOKENS: &str = "gen_ai.usage.output_tokens";
pub const ATTR_RESPONSE_TIME_TO_FIRST_CHUNK: &str = "gen_ai.response.time_to_first_chunk";
pub const ATTR_ERROR_TYPE: &str = "error.type";

/// 将协议兼容性映射为语义约定的 `gen_ai.provider.name` 值。
pub const fn provider_name(compatibility: &ProviderCompatibility) -> &'static str {
    match compatibility {
        ProviderCompatibility::OpenAiChatCompletions | ProviderCompatibility::OpenAiResponses => {
            "openai"
        }
        ProviderCompatibility::AnthropicMessages => "anthropic",
    }
}

/// finalize 时投影的三个 GenAI metrics 所需的最小数据集。
///
/// 由 handler 在 `settle_quota_with_actual_usage` 汇集点构造（流式/非流式两路径均汇于此），
/// 与 trace 持久化（O3）共享同一 finalize 事件来源——单一事实源原则。
pub struct GenAiFinalize {
    pub provider_name: &'static str,
    /// 规范模型名（客户端请求值）。
    pub request_model: String,
    /// 上游真实模型名（`provider_model_name`）。
    pub response_model: String,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    /// 端到端时延（秒），从请求开始计时。
    pub duration_s: f64,
    /// 流式首 chunk 时延（秒）；非流式为 `None`。
    pub ttft_s: Option<f64>,
}

/// finalize 事件投影到三个 GenAI metrics。
///
/// 未启用 `otel` feature 时为空函数（零开销），调用点无需 cfg。
#[cfg(feature = "otel")]
pub fn record_finalize(f: &GenAiFinalize) {
    use opentelemetry::{KeyValue, global};

    let meter = global::meter(env!("CARGO_PKG_NAME"));
    let base_attrs = [
        KeyValue::new(ATTR_OPERATION_NAME, "chat"),
        KeyValue::new(ATTR_PROVIDER_NAME, f.provider_name),
        KeyValue::new(ATTR_REQUEST_MODEL, f.request_model.clone()),
        KeyValue::new(ATTR_RESPONSE_MODEL, f.response_model.clone()),
    ];

    // gen_ai.client.token.usage（histogram，by gen_ai.token.type=input/output）
    // 单位 {token}，边界对齐语义约定建议值。
    let token_usage = meter
        .u64_histogram("gen_ai.client.token.usage")
        .with_unit("{token}")
        .with_description("Number of input and output tokens used.")
        .with_boundaries(vec![
            1.0, 4.0, 16.0, 64.0, 256.0, 1024.0, 4096.0, 16384.0, 65536.0, 262144.0, 1048576.0,
            4194304.0, 16777216.0, 67108864.0,
        ])
        .build();
    if let Some(input) = f.input_tokens {
        let mut attrs = base_attrs.to_vec();
        attrs.push(KeyValue::new("gen_ai.token.type", "input"));
        token_usage.record(input, &attrs);
    }
    if let Some(output) = f.output_tokens {
        let mut attrs = base_attrs.to_vec();
        attrs.push(KeyValue::new("gen_ai.token.type", "output"));
        token_usage.record(output, &attrs);
    }

    // gen_ai.client.operation.duration（histogram，单位 s）
    let duration = meter
        .f64_histogram("gen_ai.client.operation.duration")
        .with_unit("s")
        .with_description("GenAI operation duration.")
        .with_boundaries(vec![
            0.01, 0.02, 0.04, 0.08, 0.16, 0.32, 0.64, 1.28, 2.56, 5.12, 10.24, 20.48, 40.96, 81.92,
        ])
        .build();
    duration.record(f.duration_s, &base_attrs);

    // gen_ai.client.operation.time_to_first_chunk（histogram，单位 s，仅流式）
    if let Some(ttft) = f.ttft_s {
        let ttft_h = meter
            .f64_histogram("gen_ai.client.operation.time_to_first_chunk")
            .with_unit("s")
            .with_description("Time to receive the first chunk in a streaming response.")
            .with_boundaries(vec![
                0.01, 0.02, 0.04, 0.08, 0.16, 0.32, 0.64, 1.28, 2.56, 5.12, 10.24, 20.48, 40.96,
                81.92,
            ])
            .build();
        ttft_h.record(ttft, &base_attrs);
    }
}

/// finalize 事件投影（无 otel 时的零开销空实现）。
#[cfg(not(feature = "otel"))]
#[inline(always)]
pub fn record_finalize(_f: &GenAiFinalize) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_name_mapping() {
        assert_eq!(
            provider_name(&ProviderCompatibility::OpenAiChatCompletions),
            "openai"
        );
        assert_eq!(
            provider_name(&ProviderCompatibility::OpenAiResponses),
            "openai"
        );
        assert_eq!(
            provider_name(&ProviderCompatibility::AnthropicMessages),
            "anthropic"
        );
    }

    #[cfg(feature = "otel")]
    #[test]
    fn record_finalize_does_not_panic() {
        // 全局 meter provider 未设置时为 no-op meter；验证 record 路径不 panic。
        record_finalize(&GenAiFinalize {
            provider_name: "openai",
            request_model: "gpt-4".into(),
            response_model: "gpt-4-0613".into(),
            input_tokens: Some(100),
            output_tokens: Some(50),
            duration_s: 1.5,
            ttft_s: Some(0.3),
        });
        record_finalize(&GenAiFinalize {
            provider_name: "anthropic",
            request_model: "claude".into(),
            response_model: "claude-3".into(),
            input_tokens: None,
            output_tokens: None,
            duration_s: 0.5,
            ttft_s: None,
        });
    }
}
