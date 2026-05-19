//! Admin REST API（Phase 3 重写）— Session + Admin 认证。
//!
//! | 端点 | 方法 | 认证 | 说明 |
//! |------|------|------|------|
//! | `/api/v1/models` | GET | Session | 浏览全部模型 |
//! | `/api/v1/models/available` | GET | Session | 仅返回已启用提供者的模型 |
//! | `/api/v1/providers` | GET | Session + Admin | 列出所有提供者 |
//! | `/api/v1/providers/{provider_name}` | PUT | Session + Admin | 更新/创建提供者 |
//! | `/api/v1/providers/{provider_name}` | DELETE | Session + Admin | 删除提供者 |

use axfetchum::ApiRouter;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::config::models::{ApiKeyEntry, CompatibilitySettings};
use crate::db::models;
use crate::middleware::session_auth::{AdminAuth, SessionAuth};
use crate::server::openai_api::AppState;
use crate::store::{self, ApiKeyDisplay, AvailableModel};

pub fn all_routes() -> (Router<AppState>, axfetchum::RouteCollection) {
    ApiRouter::<AppState>::new()
        .group("models")
        .get("/api/v1/models", list_all_models)
            .response::<Vec<ModelResponse>>()
            .auth()
            .done()
        .get("/api/v1/models/available", list_available_models)
            .response::<Vec<ModelResponse>>()
            .auth()
            .done()
        .group("providers")
        .get("/api/v1/providers", list_providers)
            .response::<Vec<ProviderResponse>>()
            .auth()
            .done()
        .put("/api/v1/providers/{provider_name}", update_provider)
            .json::<UpdateProviderRequest, ProviderResponse>()
            .auth()
            .done()
        .delete("/api/v1/providers/{provider_name}", delete_provider)
            .auth()
            .done()
        .build()
}

fn db_err(e: impl std::fmt::Display) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response()
}

// ── Models ──

#[derive(Serialize, TS)]
#[ts(export)]
struct ModelResponse {
    model_name: String,
    description: Option<String>,
    max_input_tokens: u32,
    max_output_tokens: u32,
    tool_calling: bool,
    vision: bool,
    thinking: Option<bool>,
    adaptive_thinking: Option<bool>,
    input_price_per_1m: Option<f64>,
    output_price_per_1m: Option<f64>,
    cache_read_price_per_1m: Option<f64>,
    provider_ids: Vec<String>,
}

impl From<AvailableModel> for ModelResponse {
    fn from(m: AvailableModel) -> Self {
        Self {
            model_name: m.model_name,
            description: m.description,
            max_input_tokens: m.capabilities.max_input_tokens,
            max_output_tokens: m.capabilities.max_output_tokens,
            tool_calling: m.capabilities.tool_calling,
            vision: m.capabilities.vision,
            thinking: m.capabilities.thinking,
            adaptive_thinking: m.capabilities.adaptive_thinking,
            input_price_per_1m: m.input_price_per_1m,
            output_price_per_1m: m.output_price_per_1m,
            cache_read_price_per_1m: m.cache_read_price_per_1m,
            provider_ids: m.provider_ids,
        }
    }
}

async fn list_all_models(
    State(state): State<AppState>,
    SessionAuth(_user): SessionAuth,
) -> Result<Json<Vec<ModelResponse>>, Response> {
    let models = state.store.list_all_models().await.map_err(db_err)?;
    Ok(Json(models.into_iter().map(ModelResponse::from).collect()))
}

async fn list_available_models(
    State(state): State<AppState>,
    SessionAuth(_user): SessionAuth,
) -> Result<Json<Vec<ModelResponse>>, Response> {
    let models = state.store.list_available_models().await.map_err(db_err)?;
    Ok(Json(models.into_iter().map(ModelResponse::from).collect()))
}

// ── Providers ──

#[derive(Serialize, TS)]
#[ts(export)]
struct ProviderResponse {
    id: u64,
    provider_id: String,
    display_name: String,
    npm: Option<String>,
    base_url: Option<String>,
    api_keys: Vec<ApiKeyDisplay>,
    compat_settings: Option<CompatibilitySettings>,
    enabled: bool,
    priority: i64,
    created_at: i64,
}

impl From<models::Provider> for ProviderResponse {
    fn from(p: models::Provider) -> Self {
        let api_keys: Vec<ApiKeyEntry> = serde_json::from_str(&p.api_keys).unwrap_or_default();
        let compat_settings: Option<CompatibilitySettings> = p
            .compat_settings
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok());

        Self {
            id: p.id,
            provider_id: p.provider_id,
            display_name: p.display_name,
            npm: p.npm,
            base_url: p.base_url,
            api_keys: api_keys.into_iter().map(|k| ApiKeyDisplay {
                label: k.label,
                weight: k.weight,
                masked_key: store::mask_key(&k.key),
            }).collect(),
            compat_settings,
            enabled: p.enabled,
            priority: p.priority,
            created_at: p.created_at.as_millisecond() / 1000,
        }
    }
}

#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct UpdateProviderRequest {
    #[serde(default)]
    display_name: String,
    npm: Option<String>,
    base_url: Option<String>,
    #[serde(default)]
    api_keys: Vec<ApiKeyEntry>,
    compat_settings: Option<CompatibilitySettings>,
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    priority: i64,
}

async fn list_providers(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
) -> Result<Json<Vec<ProviderResponse>>, Response> {
    let providers = state.store.list_providers().await.map_err(db_err)?;
    Ok(Json(providers.into_iter().map(ProviderResponse::from).collect()))
}

async fn update_provider(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path(provider_name): Path<String>,
    Json(req): Json<UpdateProviderRequest>,
) -> Result<Json<ProviderResponse>, Response> {
    let api_keys_json = serde_json::to_string(&req.api_keys).map_err(db_err)?;
    let compat_settings_json = req.compat_settings.as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(db_err)?;

    let provider = state
        .store
        .upsert_provider(
            provider_name.clone(),
            if req.display_name.is_empty() { provider_name.clone() } else { req.display_name },
            req.npm,
            req.base_url,
            api_keys_json,
            compat_settings_json,
            req.enabled,
            req.priority,
        )
        .await
        .map_err(db_err)?;

    Ok(Json(ProviderResponse::from(provider)))
}

async fn delete_provider(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path(provider_name): Path<String>,
) -> Result<Response, Response> {
    let existed = state.store.delete_provider(&provider_name).await.map_err(db_err)?;
    if existed {
        Ok(StatusCode::NO_CONTENT.into_response())
    } else {
        Ok((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "provider not found"}))).into_response())
    }
}
