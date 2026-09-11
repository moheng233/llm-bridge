//! 目录业务键 upsert。调用方持有同一事务及导入锁；不覆盖管理员凭据和调度设置。
use super::ModelInput;
use crate::{
    config::models::ApiKeyEntry,
    db::models::{LLMModel, ModelProvider, Provider, ProviderProtocol},
    server::models_dev::{CatalogLink, CatalogProvider},
};
use toasty::Transaction as Tx;

pub async fn upsert_provider(
    tx: &mut Tx<'_>,
    input: &CatalogProvider,
) -> Result<(u64, bool), String> {
    let existing = Provider::filter(
        Provider::fields()
            .provider_id()
            .eq(input.provider_id.clone()),
    )
    .exec(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    if let Some(row) = existing.first() {
        Provider::filter(Provider::fields().id().eq(row.id))
            .update()
            .display_name(input.display_name.clone())
            .exec(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        return Ok((row.id, false));
    }
    let row = toasty::create!(Provider {
        provider_id: input.provider_id.clone(),
        display_name: input.display_name.clone(),
        api_keys: toasty::Json(Vec::<ApiKeyEntry>::new()),
        enabled: true,
        priority: 100,
        quota_adapter: None,
        quota_adapter_config: None,
    })
    .exec(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    Ok((row.id, true))
}

pub async fn upsert_protocol_by_key(
    tx: &mut Tx<'_>,
    provider_id: u64,
    input: &CatalogProvider,
) -> Result<(u64, bool), String> {
    let existing =
        ProviderProtocol::filter(ProviderProtocol::fields().provider_id().eq(provider_id))
            .exec(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    if let Some(row) = existing
        .iter()
        .filter(|row| row.protocol == input.compat && row.base_url == input.base_url)
        .min_by_key(|row| row.id)
    {
        return Ok((row.id, false));
    }
    let row = toasty::create!(ProviderProtocol {
        provider_id,
        protocol: input.compat.clone(),
        base_url: input.base_url.clone(),
        compat_settings: None,
        enabled: true,
        priority: 100,
    })
    .exec(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    Ok((row.id, true))
}

pub async fn upsert_model_by_name(
    tx: &mut Tx<'_>,
    input: &ModelInput,
) -> Result<(u64, bool), String> {
    let existing = LLMModel::filter(LLMModel::fields().model_name().eq(input.model_name.clone()))
        .exec(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(row) = existing.first() {
        LLMModel::filter(LLMModel::fields().id().eq(row.id))
            .update()
            .display_name(input.display_name.clone())
            .description(input.description.clone())
            .max_input_tokens(input.max_input_tokens)
            .max_output_tokens(input.max_output_tokens)
            .tool_calling(input.tool_calling)
            .vision(input.vision)
            .thinking(input.thinking)
            .adaptive_thinking(input.adaptive_thinking)
            .status(input.status.clone())
            .exec(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        return Ok((row.id, false));
    }
    let row = toasty::create!(LLMModel {
        model_name: input.model_name.clone(),
        display_name: input.display_name.clone(),
        description: input.description.clone(),
        max_input_tokens: input.max_input_tokens,
        max_output_tokens: input.max_output_tokens,
        tool_calling: input.tool_calling,
        vision: input.vision,
        thinking: input.thinking,
        adaptive_thinking: input.adaptive_thinking,
        status: input.status.clone(),
    })
    .exec(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    Ok((row.id, true))
}

pub async fn upsert_model_provider(
    tx: &mut Tx<'_>,
    model_id: u64,
    provider_id: u64,
    protocol_id: u64,
    input: &CatalogLink,
) -> Result<bool, String> {
    let existing = ModelProvider::filter(
        ModelProvider::fields()
            .model_id()
            .eq(model_id)
            .and(ModelProvider::fields().protocol_id().eq(protocol_id))
            .and(
                ModelProvider::fields()
                    .provider_model_id()
                    .eq(input.provider_model_id.clone()),
            ),
    )
    .exec(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    if let Some(row) = existing.iter().min_by_key(|row| row.id) {
        ModelProvider::filter(ModelProvider::fields().id().eq(row.id))
            .update()
            .provider_id(provider_id)
            .max_input_tokens(input.max_input_tokens)
            .max_output_tokens(input.max_output_tokens)
            .tool_calling(input.tool_calling)
            .vision(input.vision)
            .thinking(input.thinking)
            .adaptive_thinking(input.adaptive_thinking)
            .input_price_per_1m(input.input_price_per_1m)
            .output_price_per_1m(input.output_price_per_1m)
            .cache_read_price_per_1m(input.cache_read_price_per_1m)
            .enabled(input.enabled)
            .exec(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(false);
    }
    toasty::create!(ModelProvider {
        model_id,
        provider_id,
        protocol_id,
        provider_model_id: input.provider_model_id.clone(),
        display_name: input.provider_model_id.clone(),
        max_input_tokens: input.max_input_tokens,
        max_output_tokens: input.max_output_tokens,
        tool_calling: input.tool_calling,
        vision: input.vision,
        thinking: input.thinking,
        adaptive_thinking: input.adaptive_thinking,
        input_price_per_1m: input.input_price_per_1m,
        output_price_per_1m: input.output_price_per_1m,
        cache_read_price_per_1m: input.cache_read_price_per_1m,
        enabled: input.enabled,
        priority: 100,
    })
    .exec(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    Ok(true)
}
