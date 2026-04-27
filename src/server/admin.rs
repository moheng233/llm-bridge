use axfetchum::ApiRouter;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

use crate::config::models::{ApiKeyEntry, CompatibilitySettings, ProviderCompatConfig, ProviderCompatibility, ProviderConfig};
use crate::server::openai_api::AppState;
use crate::store::{ProviderInfo, StoreError};
use crate::types::LMModelInfo;

pub fn all_routes() -> (Router<AppState>, axfetchum::RouteCollection) {
    ApiRouter::<AppState>::new()
        .group("models")
        .get("/api/v1/models", list_all_models)
            .response::<Vec<CatalogModelResponse>>()
            .auth()
            .done()
        .get("/api/v1/models/available", list_available_models)
            .response::<Vec<AvailableModelResponse>>()
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

fn check_auth(state: &AppState, headers: &HeaderMap) -> Result<(), Response> {
    let Some(expected) = &state.auth_token else {
        return Ok(());
    };
    let provided = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match provided {
        Some(token) if token == expected => Ok(()),
        _ => Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorBody {
                error: "unauthorized".into(),
            }),
        )
            .into_response()),
    }
}

fn store_err(e: StoreError) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorBody {
            error: e.to_string(),
        }),
    )
        .into_response()
}

#[derive(Serialize, TS)]
#[ts(export)]
struct ErrorBody {
    error: String,
}

// ── Models ──

#[derive(Serialize, TS)]
#[ts(export)]
struct CatalogModelResponse {
    model_name: String,
    capabilities: LMModelInfo,
    provider_ids: Vec<String>,
}

async fn list_all_models(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(e) = check_auth(&state, &headers) {
        return e;
    }
    let models: Vec<CatalogModelResponse> = state
        .store
        .list_all_models()
        .into_iter()
        .map(|m| CatalogModelResponse {
            model_name: m.model_name,
            capabilities: m.capabilities,
            provider_ids: m.provider_ids,
        })
        .collect();
    Json(models).into_response()
}

#[derive(Serialize, TS)]
#[ts(export)]
struct AvailableModelResponse {
    model_name: String,
    capabilities: LMModelInfo,
}

async fn list_available_models(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(e) = check_auth(&state, &headers) {
        return e;
    }
    let models: Vec<AvailableModelResponse> = state
        .store
        .list_available_models()
        .into_iter()
        .map(|m| AvailableModelResponse {
            model_name: m.model_name,
            capabilities: m.capabilities,
        })
        .collect();
    Json(models).into_response()
}

// ── Providers ──

#[derive(Serialize, TS)]
#[ts(export)]
struct ProviderResponse {
    provider_name: String,
    name: String,
    enabled: bool,
    priority: u32,
    api_keys: Vec<ApiKeyDisplay>,
    compatibilities: HashMap<ProviderCompatibility, ProviderCompatConfig>,
    compat_settings: Option<CompatibilitySettings>,
    base_url_override: Option<String>,
    model_count: usize,
}

#[derive(Serialize, TS)]
#[ts(export)]
struct ApiKeyDisplay {
    label: String,
    weight: u32,
    masked_key: String,
}

impl From<ProviderInfo> for ProviderResponse {
    fn from(p: ProviderInfo) -> Self {
        Self {
            provider_name: p.id.clone(),
            name: p.name,
            enabled: p.enabled,
            priority: p.priority,
            api_keys: p.api_keys.into_iter().map(|k| ApiKeyDisplay {
                label: k.label,
                weight: k.weight,
                masked_key: k.masked_key,
            }).collect(),
            compatibilities: p.compatibilities,
            compat_settings: p.compat_settings,
            base_url_override: p.base_url_override,
            model_count: p.model_count,
        }
    }
}

#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct UpdateProviderRequest {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    priority: u32,
    base_url_override: Option<String>,
    #[serde(default)]
    api_keys: Vec<ApiKeyEntry>,
    compat_settings: Option<CompatibilitySettings>,
}

async fn list_providers(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(e) = check_auth(&state, &headers) {
        return e;
    }
    let providers: Vec<ProviderResponse> = state
        .store
        .list_providers()
        .into_iter()
        .map(Into::into)
        .collect();
    Json(providers).into_response()
}

async fn update_provider(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(provider_name): Path<String>,
    Json(req): Json<UpdateProviderRequest>,
) -> Response {
    if let Err(e) = check_auth(&state, &headers) {
        return e;
    }

    let existing = state.store.get_provider_config(&provider_name);
    let config = ProviderConfig {
        enabled: req.enabled,
        priority: req.priority,
        base_url_override: req.base_url_override.or_else(|| {
            existing.as_ref().and_then(|c| c.base_url_override.clone())
        }),
        api_keys: req.api_keys,
        compat_settings: req.compat_settings.or_else(|| {
            existing.as_ref().and_then(|c| c.compat_settings.clone())
        }),
    };

    match state.store.upsert_provider(&provider_name, config) {
        Ok(()) => {
            match state.store.list_providers().into_iter().find(|p| p.id == provider_name) {
                Some(info) => Json(ProviderResponse::from(info)).into_response(),
                None => (
                    StatusCode::NOT_FOUND,
                    Json(ErrorBody { error: format!("provider '{}' not found after update", provider_name) }),
                ).into_response(),
            }
        }
        Err(e) => store_err(e),
    }
}

async fn delete_provider(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(provider_name): Path<String>,
) -> Response {
    if let Err(e) = check_auth(&state, &headers) {
        return e;
    }
    match state.store.delete_provider(&provider_name) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorBody {
                error: format!("provider '{}' not found", provider_name),
            }),
        )
            .into_response(),
        Err(e) => store_err(e),
    }
}
