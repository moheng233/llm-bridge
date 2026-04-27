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

use crate::config::models::{ProviderCompatibility, ProviderCompatConfig};
use crate::db::{CatalogModelRecord, DbError, ProviderModelRecord, ProviderRecord};
use crate::server::ws::{AppState, ws_handler};
use crate::types::LMModelInfo;

pub fn all_routes() -> (Router<AppState>, axfetchum::RouteCollection) {
    ApiRouter::<AppState>::new()
        .group("ws")
        .get("/ws", ws_handler)
            .as_("connect")
        .group("models")
        .get("/api/v1/models", list_catalog_models)
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
        .post("/api/v1/providers", create_provider)
            .json::<CreateProviderRequest, ProviderResponse>()
            .auth()
            .done()
        .get("/api/v1/providers/{provider_name}", get_provider)
            .response::<ProviderResponse>()
            .auth()
            .done()
        .put("/api/v1/providers/{provider_name}", update_provider)
            .json::<UpdateProviderRequest, ProviderResponse>()
            .auth()
            .done()
        .delete("/api/v1/providers/{provider_name}", delete_provider)
            .auth()
            .done()
        .put("/api/v1/providers/{provider_name}/secret", update_provider_secret)
            .body::<UpdateProviderSecretRequest>()
            .auth()
            .done()
        .get("/api/v1/providers/{provider_name}/models", list_provider_models)
            .response::<Vec<ProviderModelResponse>>()
            .auth()
            .done()
        .post("/api/v1/providers/{provider_name}/models", create_provider_model)
            .json::<CreateProviderModelRequest, ProviderModelResponse>()
            .auth()
            .done()
        .delete("/api/v1/providers/{provider_name}/models/{model_name}", delete_provider_model_binding)
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

fn db_err(e: DbError) -> Response {
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

// ── Models ────────────────────────────────────────────────────────────────────

#[derive(Serialize, TS)]
#[ts(export)]
struct CatalogModelResponse {
    model_name: String,
    capabilities: LMModelInfo,
}

impl From<CatalogModelRecord> for CatalogModelResponse {
    fn from(r: CatalogModelRecord) -> Self {
        Self {
            model_name: r.model_name,
            capabilities: r.capabilities,
        }
    }
}

/// List every model in the OpenRouter catalog snapshot.
async fn list_catalog_models(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(e) = check_auth(&state, &headers) {
        return e;
    }
    match state.db.list_catalog_models() {
        Ok(models) => {
            let body: Vec<CatalogModelResponse> = models.into_iter().map(Into::into).collect();
            Json(body).into_response()
        }
        Err(e) => db_err(e),
    }
}

#[derive(Serialize, TS)]
#[ts(export)]
struct AvailableModelResponse {
    model_name: String,
    capabilities: LMModelInfo,
}

/// List only models that have at least one active provider binding.
async fn list_available_models(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(e) = check_auth(&state, &headers) {
        return e;
    }
    match state.db.list_available_models() {
        Ok(models) => {
            let body: Vec<AvailableModelResponse> = models
                .into_iter()
                .map(|m| AvailableModelResponse {
                    model_name: m.model_name,
                    capabilities: m.capabilities,
                })
                .collect();
            Json(body).into_response()
        }
        Err(e) => db_err(e),
    }
}

// ── Providers ─────────────────────────────────────────────────────────────────

#[derive(Serialize, TS)]
#[ts(export)]
struct ProviderResponse {
    provider_name: String,
    compatibilities: HashMap<ProviderCompatibility, ProviderCompatConfig>,
    base_url: Option<String>,
}

impl From<ProviderRecord> for ProviderResponse {
    fn from(r: ProviderRecord) -> Self {
        Self {
            provider_name: r.provider_name,
            compatibilities: r.compatibilities,
            base_url: r.base_url,
        }
    }
}

#[derive(Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct CreateProviderRequest {
    provider_name: String,
    compatibilities: HashMap<ProviderCompatibility, ProviderCompatConfig>,
    base_url: Option<String>,
    api_key: String,
}

#[derive(Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct UpdateProviderRequest {
    compatibilities: HashMap<ProviderCompatibility, ProviderCompatConfig>,
    base_url: Option<String>,
    api_key: String,
}

#[derive(Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct UpdateProviderSecretRequest {
    api_key: String,
}

async fn list_providers(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(e) = check_auth(&state, &headers) {
        return e;
    }
    match state.db.list_providers() {
        Ok(providers) => {
            let body: Vec<ProviderResponse> = providers.into_iter().map(Into::into).collect();
            Json(body).into_response()
        }
        Err(e) => db_err(e),
    }
}

async fn create_provider(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateProviderRequest>,
) -> Response {
    if let Err(e) = check_auth(&state, &headers) {
        return e;
    }

    if state
        .db
        .get_provider(&req.provider_name)
        .ok()
        .flatten()
        .is_some()
    {
        return (
            StatusCode::CONFLICT,
            Json(ErrorBody {
                error: format!("provider '{}' already exists", req.provider_name),
            }),
        )
            .into_response();
    }

    let record = ProviderRecord {
        provider_name: req.provider_name,
        compatibilities: req.compatibilities,
        base_url: req.base_url,
    };

    match state.db.put_provider(&record) {
        Ok(()) => {
            if let Err(e) = state.db.put_provider_secret(&record.provider_name, &req.api_key) {
                return db_err(e);
            }
            (StatusCode::CREATED, Json(ProviderResponse::from(record))).into_response()
        }
        Err(e) => db_err(e),
    }
}

async fn get_provider(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(provider_name): Path<String>,
) -> Response {
    if let Err(e) = check_auth(&state, &headers) {
        return e;
    }
    match state.db.get_provider(&provider_name) {
        Ok(Some(record)) => Json(ProviderResponse::from(record)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorBody {
                error: format!("provider '{}' not found", provider_name),
            }),
        )
            .into_response(),
        Err(e) => db_err(e),
    }
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

    if state.db.get_provider(&provider_name).ok().flatten().is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorBody {
                error: format!("provider '{}' not found", provider_name),
            }),
        )
            .into_response();
    }

    let record = ProviderRecord {
        provider_name: provider_name.clone(),
        compatibilities: req.compatibilities,
        base_url: req.base_url,
    };

    match state.db.put_provider(&record) {
        Ok(()) => {
            if let Err(e) = state.db.put_provider_secret(&provider_name, &req.api_key) {
                return db_err(e);
            }
            Json(ProviderResponse::from(record)).into_response()
        }
        Err(e) => db_err(e),
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
    match state.db.delete_provider(&provider_name) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorBody {
                error: format!("provider '{}' not found", provider_name),
            }),
        )
            .into_response(),
        Err(e) => db_err(e),
    }
}

async fn update_provider_secret(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(provider_name): Path<String>,
    Json(req): Json<UpdateProviderSecretRequest>,
) -> Response {
    if let Err(e) = check_auth(&state, &headers) {
        return e;
    }

    if state.db.get_provider(&provider_name).ok().flatten().is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorBody {
                error: format!("provider '{}' not found", provider_name),
            }),
        )
            .into_response();
    }

    match state.db.put_provider_secret(&provider_name, &req.api_key) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => db_err(e),
    }
}

// ── Provider-model bindings ───────────────────────────────────────────────────

#[derive(Serialize, TS)]
#[ts(export)]
struct ProviderModelResponse {
    model_name: String,
    provider_name: String,
    provider_model_name: String,
    priority: u32,
}

impl From<ProviderModelRecord> for ProviderModelResponse {
    fn from(r: ProviderModelRecord) -> Self {
        Self {
            model_name: r.model_name,
            provider_name: r.provider_name,
            provider_model_name: r.provider_model_name,
            priority: r.priority,
        }
    }
}

#[derive(Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct CreateProviderModelRequest {
    model_name: String,
    provider_model_name: String,
    priority: u32,
}

async fn list_provider_models(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(provider_name): Path<String>,
) -> Response {
    if let Err(e) = check_auth(&state, &headers) {
        return e;
    }

    if state.db.get_provider(&provider_name).ok().flatten().is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorBody {
                error: format!("provider '{}' not found", provider_name),
            }),
        )
            .into_response();
    }

    match state.db.list_provider_models_by_provider(&provider_name) {
        Ok(records) => {
            let body: Vec<ProviderModelResponse> = records.into_iter().map(Into::into).collect();
            Json(body).into_response()
        }
        Err(e) => db_err(e),
    }
}

async fn create_provider_model(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(provider_name): Path<String>,
    Json(req): Json<CreateProviderModelRequest>,
) -> Response {
    if let Err(e) = check_auth(&state, &headers) {
        return e;
    }

    let record = ProviderModelRecord {
        model_name: req.model_name.clone(),
        provider_name: provider_name.clone(),
        provider_model_name: req.provider_model_name,
        priority: req.priority,
    };

    match state.db.put_provider_model(&record) {
        Ok(()) => (StatusCode::CREATED, Json(ProviderModelResponse::from(record))).into_response(),
        Err(DbError::ProviderNotFound(_)) => (
            StatusCode::NOT_FOUND,
            Json(ErrorBody {
                error: format!("provider '{}' not found", provider_name),
            }),
        )
            .into_response(),
        Err(DbError::CatalogModelNotFound(_)) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorBody {
                error: format!("catalog model '{}' not found", req.model_name),
            }),
        )
            .into_response(),
        Err(e) => db_err(e),
    }
}

async fn delete_provider_model_binding(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((provider_name, model_name)): Path<(String, String)>,
) -> Response {
    if let Err(e) = check_auth(&state, &headers) {
        return e;
    }
    match state.db.delete_provider_model(&model_name, &provider_name) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => db_err(e),
    }
}
