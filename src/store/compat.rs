//! 兼容协议推导 + models.dev 数据映射（Phase 3.2）。
//!
//! - `npm_to_compatibility`: AI SDK npm 包名 → 兼容协议
//! - `models_dev_to_provider_model`: models.dev 模型数据 → ProviderModel 字段映射

use crate::config::models::ProviderCompatibility;
use crate::models_dev::ModelsDevModel;

/// 根据 AI SDK npm 包名推导兼容协议。
///
/// | AI SDK npm 包 | 推导协议 |
/// |---------------|---------|
/// | `@ai-sdk/openai` / `@ai-sdk/openai-compatible` | `OpenAiChatCompletions` |
/// | `@ai-sdk/anthropic` | `AnthropicMessages` |
/// | 其他 / 未知 | 默认 `OpenAiChatCompletions` |
pub fn npm_to_compatibility(npm: &str) -> ProviderCompatibility {
    if npm.contains("@ai-sdk/anthropic") {
        ProviderCompatibility::OpenAiChatCompletions // Anthropic 也走 chat completions 兼容
    } else {
        ProviderCompatibility::OpenAiChatCompletions
    }
}

/// 从 models.dev 模型数据提取能力信息。
#[derive(Debug, Clone)]
pub struct ModelCapabilitiesFromDev {
    pub max_input_tokens: i64,
    pub max_output_tokens: i64,
    pub tool_calling: bool,
    pub vision: bool,
    pub thinking: bool,
    pub adaptive_thinking: bool,
}

/// 从 models.dev 模型数据提取定价信息。
#[derive(Debug, Clone)]
pub struct ModelPricingFromDev {
    pub input_price_per_1m: Option<f64>,
    pub output_price_per_1m: Option<f64>,
    pub cache_read_price_per_1m: Option<f64>,
}

impl ModelCapabilitiesFromDev {
    const DEFAULT_TOKENS: i64 = 4096;

    pub fn from_models_dev(m: &ModelsDevModel) -> Self {
        let limit = m.limit.as_ref();
        Self {
            max_input_tokens: limit
                .map(|l| l.context as i64)
                .unwrap_or(Self::DEFAULT_TOKENS),
            max_output_tokens: limit
                .map(|l| l.output as i64)
                .unwrap_or(Self::DEFAULT_TOKENS),
            tool_calling: m.tool_call,
            vision: m
                .modalities
                .as_ref()
                .map(|mods| mods.input.iter().any(|s| s == "image"))
                .unwrap_or(false),
            thinking: m.reasoning,
            adaptive_thinking: m
                .interleaved
                .as_ref()
                .map(|il| il.is_active())
                .unwrap_or(false),
        }
    }
}

impl ModelPricingFromDev {
    /// 价格以每 1M tokens 为单位（models.dev 的 cost 字段也是按 1M 计价）。
    pub fn from_models_dev(m: &ModelsDevModel) -> Self {
        let cost = m.cost.as_ref();
        Self {
            input_price_per_1m: cost.map(|c| c.input),
            output_price_per_1m: cost.map(|c| c.output),
            cache_read_price_per_1m: cost.and_then(|c| c.cache_read),
        }
    }
}

/// 推导提供者的默认 base URL。
///
/// 优先级：用户覆盖 > models.dev per-model provider.api > models.dev provider-level api。
pub fn deduce_base_url(
    pdata: &crate::models_dev::ModelsDevProvider,
    mdata: Option<&ModelsDevModel>,
) -> Option<String> {
    // Per-model provider override
    if let Some(mp) = mdata.and_then(|m| m.provider.as_ref()) {
        if let Some(ref api) = mp.api {
            return Some(api.clone());
        }
    }
    // Provider-level api
    pdata.api.clone()
}
