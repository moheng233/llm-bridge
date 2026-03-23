use axum::{
    Extension, Json, Router,
    extract::Path,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get},
};
use serde::{Deserialize, Serialize};

use crate::config::models::ProviderType;
use crate::db::{CatalogModelRecord, DbError, ProviderModelRecord, ProviderRecord};
use crate::server::ws::AppState;
use crate::types::LMModelInfo;

pub fn admin_routes() -> Router {
    Router::new()
        .route("/api/v1/models", get(list_catalog_models))
        .route("/api/v1/models/available", get(list_available_models))
        .route(
            "/api/v1/providers",
            get(list_providers).post(create_provider),
        )
        .route(
            "/api/v1/providers/{provider_name}",
            get(get_provider).put(update_provider).delete(delete_provider),
        )
        .route(
            "/api/v1/providers/{provider_name}/models",
            get(list_provider_models).post(create_provider_model),
        )
        .route(
            "/api/v1/providers/{provider_name}/models/{model_name}",
            delete(delete_provider_model_binding),
        )
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

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

// ── Models ────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
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
    Extension(state): Extension<AppState>,
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

#[derive(Serialize)]
struct AvailableModelResponse {
    model_name: String,
    capabilities: LMModelInfo,
}

/// List only models that have at least one active provider binding.
async fn list_available_models(
    Extension(state): Extension<AppState>,
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

#[derive(Serialize)]
struct ProviderResponse {
    provider_name: String,
    provider_type: ProviderType,
    base_url: Option<String>,
    keyring_service: String,
    keyring_account: String,
}

impl From<ProviderRecord> for ProviderResponse {
    fn from(r: ProviderRecord) -> Self {
        Self {
            provider_name: r.provider_name,
            provider_type: r.provider_type,
            base_url: r.base_url,
            keyring_service: r.keyring_service,
            keyring_account: r.keyring_account,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateProviderRequest {
    provider_name: String,
    provider_type: ProviderType,
    base_url: Option<String>,
    keyring_service: String,
    keyring_account: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProviderRequest {
    provider_type: ProviderType,
    base_url: Option<String>,
    keyring_service: String,
    keyring_account: String,
}

async fn list_providers(
    Extension(state): Extension<AppState>,
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
    Extension(state): Extension<AppState>,
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
        provider_type: req.provider_type,
        base_url: req.base_url,
        keyring_service: req.keyring_service,
        keyring_account: req.keyring_account,
    };

    match state.db.put_provider(&record) {
        Ok(()) => (StatusCode::CREATED, Json(ProviderResponse::from(record))).into_response(),
        Err(e) => db_err(e),
    }
}

async fn get_provider(
    Extension(state): Extension<AppState>,
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
    Extension(state): Extension<AppState>,
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
        provider_type: req.provider_type,
        base_url: req.base_url,
        keyring_service: req.keyring_service,
        keyring_account: req.keyring_account,
    };

    match state.db.put_provider(&record) {
        Ok(()) => Json(ProviderResponse::from(record)).into_response(),
        Err(e) => db_err(e),
    }
}

async fn delete_provider(
    Extension(state): Extension<AppState>,
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

// ── Provider-model bindings ───────────────────────────────────────────────────

#[derive(Serialize)]
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateProviderModelRequest {
    model_name: String,
    provider_model_name: String,
    priority: u32,
}

async fn list_provider_models(
    Extension(state): Extension<AppState>,
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
    Extension(state): Extension<AppState>,
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
    Extension(state): Extension<AppState>,
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
