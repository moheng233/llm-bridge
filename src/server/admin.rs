//! Admin REST API（Phase 4）— Session + Admin 认证。
//!
//! ## Model browsing (Session auth — any logged-in user)
//! | 端点 | 方法 | 说明 |
//! |------|------|------|
//! | `/api/v1/models` | GET | 浏览全部模型（含未启用） |
//! | `/api/v1/models/available` | GET | 仅返回已启用提供者的模型 |
//!
//! ## Admin API (Session + Admin auth)
//! | 端点 | 方法 | 说明 |
//! |------|------|------|
//! | `/api/v1/admin/providers` | GET | 列出所有提供者 |
//! | `/api/v1/admin/providers` | POST | 创建提供者 |
//! | `/api/v1/admin/providers/{id}` | GET | 获取单个提供者 |
//! | `/api/v1/admin/providers/{id}` | PUT | 更新提供者 |
//! | `/api/v1/admin/providers/{id}` | DELETE | 删除提供者 |
//! | `/api/v1/admin/providers/{id}/models` | GET | 列出提供者下的模型 |
//! | `/api/v1/admin/providers/{id}/models` | POST | 添加模型 |
//! | `/api/v1/admin/providers/{id}/models/{model_id}` | PUT | 更新模型 |
//! | `/api/v1/admin/providers/{id}/models/{model_id}` | DELETE | 删除模型 |
//! | `/api/v1/admin/models-dev/search` | GET | 搜索 models.dev 缓存 |
//! | `/api/v1/admin/models-dev/import` | POST | 从 models.dev 导入提供者 |
//! | `/api/v1/admin/users` | GET | 列出所有用户 |
//! | `/api/v1/admin/users/{id}/role` | PATCH | 修改用户角色 |

use axfetchum::ApiRouter;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::config::models::{ApiKeyEntry, CompatibilitySettings, ProviderCompatibility};
use crate::db::models;
use crate::middleware::session_auth::{AdminAuth, SessionAuth};
use crate::server::AppState;
use crate::store::{self, ApiKeyDisplay, AvailableModel, CatalogProviderSummary, ImportedProvider};

// ── Model browsing routes (axfetchum, for auto-generated TS client) ──

pub fn model_browse_routes() -> ApiRouter<AppState> {
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
}

/// Admin CRUD routes (ApiRouter for auto-generated TS client).
pub fn admin_crud_routes() -> ApiRouter<AppState> {
    ApiRouter::<AppState>::new()
        .group("admin")
        // Providers
        .get("/api/v1/admin/providers", list_providers)
            .response::<Vec<ProviderResponse>>()
            .auth()
            .done()
        .post("/api/v1/admin/providers", create_provider)
            .json::<CreateProviderRequest, ProviderResponse>()
            .auth()
            .done()
        .get("/api/v1/admin/providers/{id}", get_provider)
            .response::<ProviderResponse>()
            .auth()
            .done()
        .put("/api/v1/admin/providers/{id}", update_provider)
            .json::<UpdateProviderRequest, ProviderResponse>()
            .auth()
            .done()
        .delete("/api/v1/admin/providers/{id}", delete_provider)
            .auth()
            .done()
        // Provider models
        .get("/api/v1/admin/providers/{id}/models", list_provider_models)
            .response::<Vec<ProviderModelResponse>>()
            .auth()
            .done()
        .post("/api/v1/admin/providers/{id}/models", add_provider_model)
            .json::<AddModelRequest, ProviderModelResponse>()
            .auth()
            .done()
        .put("/api/v1/admin/providers/{id}/models/{model_id}", update_provider_model)
            .json::<UpdateModelRequest, ProviderModelResponse>()
            .auth()
            .done()
        .delete("/api/v1/admin/providers/{id}/models/{model_id}", delete_provider_model)
            .auth()
            .done()
        // models.dev discovery
        .get("/api/v1/admin/models-dev/search", search_models_dev)
            .response::<Vec<CatalogProviderSummary>>()
            .auth()
            .done()
        .post("/api/v1/admin/models-dev/import", import_models_dev)
            .json::<ImportModelsDevRequest, ImportedProvider>()
            .auth()
            .done()
        // Users
        .get("/api/v1/admin/users", list_users)
            .response::<Vec<UserResponse>>()
            .auth()
            .done()
        .patch("/api/v1/admin/users/{id}/role", update_user_role)
            .json::<UpdateRoleRequest, UserResponse>()
            .auth()
            .done()
}

fn db_err(e: impl std::fmt::Display) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response()
}

// ── Models ──

#[derive(Serialize, TS)]
#[ts(export)]
struct ModelResponse {
    model_name: String,
    display_name: String,
    description: Option<String>,
    /// 标称能力
    max_input_tokens: u32,
    max_output_tokens: u32,
    tool_calling: bool,
    vision: bool,
    thinking: Option<bool>,
    adaptive_thinking: Option<bool>,
    /// 提供者列表（含各自的定价和能力覆盖）
    providers: Vec<ModelProviderSummary>,
}

#[derive(Serialize, TS)]
#[ts(export)]
struct ModelProviderSummary {
    provider_id: String,
    provider_display_name: String,
    provider_model_id: String,
    compatibility: String,
    max_input_tokens: Option<i64>,
    max_output_tokens: Option<i64>,
    tool_calling: Option<bool>,
    vision: Option<bool>,
    thinking: Option<bool>,
    adaptive_thinking: Option<bool>,
    input_price_per_1m: Option<f64>,
    output_price_per_1m: Option<f64>,
    cache_read_price_per_1m: Option<f64>,
    enabled: bool,
    priority: i64,
}

impl From<AvailableModel> for ModelResponse {
    fn from(m: AvailableModel) -> Self {
        let providers = m.providers.into_iter().map(|p| ModelProviderSummary {
            provider_id: p.provider_id,
            provider_display_name: p.provider_display_name,
            provider_model_id: p.provider_model_id,
            compatibility: format!("{:?}", p.compatibility),
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
        }).collect();

        Self {
            model_name: m.model_name,
            display_name: m.display_name,
            description: m.description,
            max_input_tokens: m.nominal_capabilities.max_input_tokens,
            max_output_tokens: m.nominal_capabilities.max_output_tokens,
            tool_calling: m.nominal_capabilities.tool_calling,
            vision: m.nominal_capabilities.vision,
            thinking: m.nominal_capabilities.thinking,
            adaptive_thinking: m.nominal_capabilities.adaptive_thinking,
            providers,
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

// ── Provider responses ──

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
    model_count: usize,
}

fn provider_to_response(p: &models::Provider, model_count: usize) -> ProviderResponse {
    let api_keys: Vec<ApiKeyEntry> = serde_json::from_str(&p.api_keys).unwrap_or_default();
    let compat_settings: Option<CompatibilitySettings> = p
        .compat_settings
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok());

    ProviderResponse {
        id: p.id,
        provider_id: p.provider_id.clone(),
        display_name: p.display_name.clone(),
        npm: p.npm.clone(),
        base_url: p.base_url.clone(),
        api_keys: api_keys.into_iter().map(|k| ApiKeyDisplay {
            label: k.label,
            weight: k.weight,
            masked_key: store::mask_key(&k.key),
        }).collect(),
        compat_settings,
        enabled: p.enabled,
        priority: p.priority,
        created_at: p.created_at.as_millisecond() / 1000,
        model_count,
    }
}

// ── Provider Model response (ModelProvider) ──

#[derive(Serialize, TS)]
#[ts(export)]
struct ProviderModelResponse {
    id: u64,
    model_id: u64,
    provider_id: u64,
    /// 关联的规范模型名（需要 JOIN 获取）
    model_name: String,
    provider_model_id: String,
    compatibility: String,
    display_name: String,
    max_input_tokens: Option<i64>,
    max_output_tokens: Option<i64>,
    tool_calling: Option<bool>,
    vision: Option<bool>,
    thinking: Option<bool>,
    adaptive_thinking: Option<bool>,
    input_price_per_1m: Option<f64>,
    output_price_per_1m: Option<f64>,
    cache_read_price_per_1m: Option<f64>,
    enabled: bool,
    priority: i64,
}

/// 将 ModelProvider 转为响应，需要传入 model_name（从 Model 表获取）。
fn model_provider_to_response(mp: &models::ModelProvider, model_name: String) -> ProviderModelResponse {
    ProviderModelResponse {
        id: mp.id,
        model_id: mp.model_id,
        provider_id: mp.provider_id,
        model_name,
        provider_model_id: mp.provider_model_id.clone(),
        compatibility: format!("{:?}", mp.compatibility),
        display_name: mp.display_name.clone(),
        max_input_tokens: mp.max_input_tokens,
        max_output_tokens: mp.max_output_tokens,
        tool_calling: mp.tool_calling,
        vision: mp.vision,
        thinking: mp.thinking,
        adaptive_thinking: mp.adaptive_thinking,
        input_price_per_1m: mp.input_price_per_1m,
        output_price_per_1m: mp.output_price_per_1m,
        cache_read_price_per_1m: mp.cache_read_price_per_1m,
        enabled: mp.enabled,
        priority: mp.priority,
    }
}

// ── Request types ──

#[derive(Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct CreateProviderRequest {
    provider_id: String,
    #[serde(default)]
    display_name: String,
    npm: Option<String>,
    base_url: Option<String>,
    #[serde(default)]
    api_keys: Vec<ApiKeyEntry>,
    compat_settings: Option<CompatibilitySettings>,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default = "default_priority")]
    priority: i64,
}

fn default_true() -> bool { true }
fn default_priority() -> i64 { 100 }

#[derive(Deserialize, TS)]
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

#[derive(Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct AddModelRequest {
    /// 规范模型名（如 "openai/gpt-4o"）
    model_name: String,
    /// 提供者侧的模型 ID
    provider_model_id: String,
    #[serde(default = "default_compat")]
    compatibility: String,
    #[serde(default)]
    display_name: String,
    #[serde(default = "default_4096")]
    max_input_tokens: i64,
    #[serde(default = "default_4096")]
    max_output_tokens: i64,
    #[serde(default)]
    tool_calling: bool,
    #[serde(default)]
    vision: bool,
    #[serde(default)]
    thinking: bool,
    #[serde(default)]
    adaptive_thinking: bool,
    /// 提供者特定输入价格
    input_price_per_1m: Option<f64>,
    /// 提供者特定输出价格
    output_price_per_1m: Option<f64>,
    cache_read_price_per_1m: Option<f64>,
}

fn default_compat() -> String { "OpenAiChatCompletions".into() }
fn default_4096() -> i64 { 4096 }

#[derive(Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct UpdateModelRequest {
    provider_model_id: String,
    compatibility: String,
    #[serde(default)]
    display_name: String,
    max_input_tokens: i64,
    max_output_tokens: i64,
    #[serde(default)]
    tool_calling: bool,
    #[serde(default)]
    vision: bool,
    #[serde(default)]
    thinking: bool,
    #[serde(default)]
    adaptive_thinking: bool,
    input_price_per_1m: Option<f64>,
    output_price_per_1m: Option<f64>,
    cache_read_price_per_1m: Option<f64>,
    #[serde(default = "default_true")]
    enabled: bool,
}

#[derive(Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct ImportModelsDevRequest {
    provider_id: String,
    /// 可选：导入时设置 API Key（label + key）
    #[serde(default)]
    api_keys: Vec<ApiKeyEntry>,
}

#[derive(Deserialize, TS)]
#[ts(export)]
struct UpdateRoleRequest {
    role: String,
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

// ── Providers CRUD ──

async fn list_providers(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
) -> Result<Json<Vec<ProviderResponse>>, Response> {
    let providers = state.store.list_providers().await.map_err(db_err)?;
    let mut result = Vec::new();
    for p in &providers {
        let models = state.store.list_provider_models(p.id).await.unwrap_or_default();
        result.push(provider_to_response(p, models.len()));
    }
    Ok(Json(result))
}

async fn get_provider(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path(id): Path<u64>,
) -> Result<Json<ProviderResponse>, Response> {
    let provider = state.store.get_provider_by_id(id).await.map_err(db_err)?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "provider not found"}))).into_response())?;
    let models = state.store.list_provider_models(provider.id).await.unwrap_or_default();
    Ok(Json(provider_to_response(&provider, models.len())))
}

async fn create_provider(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Json(req): Json<CreateProviderRequest>,
) -> Result<(StatusCode, Json<ProviderResponse>), Response> {
    let api_keys_json = serde_json::to_string(&req.api_keys).map_err(db_err)?;
    let compat_settings_json = req.compat_settings.as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(db_err)?;

    let display_name = if req.display_name.is_empty() { req.provider_id.clone() } else { req.display_name };

    let provider = state.store.upsert_provider(
        req.provider_id,
        display_name,
        req.npm,
        req.base_url,
        api_keys_json,
        compat_settings_json,
        req.enabled,
        req.priority,
    ).await.map_err(db_err)?;

    let models = state.store.list_provider_models(provider.id).await.unwrap_or_default();
    Ok((StatusCode::CREATED, Json(provider_to_response(&provider, models.len()))))
}

async fn update_provider(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path(id): Path<u64>,
    Json(req): Json<UpdateProviderRequest>,
) -> Result<Json<ProviderResponse>, Response> {
    let provider = state.store.get_provider_by_id(id).await.map_err(db_err)?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "provider not found"}))).into_response())?;

    let api_keys_json = serde_json::to_string(&req.api_keys).map_err(db_err)?;
    let compat_settings_json = req.compat_settings.as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(db_err)?;

    let display_name = if req.display_name.is_empty() { provider.display_name.clone() } else { req.display_name };

    let updated = state.store.update_provider_by_id(
        id,
        display_name,
        req.npm,
        req.base_url,
        api_keys_json,
        compat_settings_json,
        req.enabled,
        req.priority,
    ).await.map_err(db_err)?;

    let models = state.store.list_provider_models(updated.id).await.unwrap_or_default();
    Ok(Json(provider_to_response(&updated, models.len())))
}

async fn delete_provider(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path(id): Path<u64>,
) -> Result<Response, Response> {
    let existed = state.store.delete_provider_by_id(id).await.map_err(db_err)?;
    if existed {
        Ok(StatusCode::NO_CONTENT.into_response())
    } else {
        Ok((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "provider not found"}))).into_response())
    }
}

// ── Provider Models CRUD ──

async fn list_provider_models(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path(id): Path<u64>,
) -> Result<Json<Vec<ProviderModelResponse>>, Response> {
    // Verify provider exists
    state.store.get_provider_by_id(id).await.map_err(db_err)?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "provider not found"}))).into_response())?;

    let mps = state.store.list_provider_models(id).await.map_err(db_err)?;
    let mut result = Vec::new();
    for mp in &mps {
        let model_name = crate::db::models::LLMModel::get_by_id(&mut state.db.clone(), &mp.model_id)
            .await
            .map(|m| m.model_name)
            .unwrap_or_default();
        result.push(model_provider_to_response(mp, model_name));
    }
    Ok(Json(result))
}

async fn add_provider_model(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path(id): Path<u64>,
    Json(req): Json<AddModelRequest>,
) -> Result<(StatusCode, Json<ProviderModelResponse>), Response> {
    // Verify provider exists
    state.store.get_provider_by_id(id).await.map_err(db_err)?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "provider not found"}))).into_response())?;

    let compatibility: ProviderCompatibility = serde_json::from_str(&format!("\"{}\"", req.compatibility))
        .unwrap_or(ProviderCompatibility::OpenAiChatCompletions);

    let display_name = if req.display_name.is_empty() { req.model_name.clone() } else { req.display_name };

    let mp = state.store.add_provider_model(
        id,
        req.model_name.clone(),
        req.provider_model_id,
        compatibility,
        display_name,
        None, // 描述属于 Model，通过 ensure_model 自动管理
        req.max_input_tokens,
        req.max_output_tokens,
        req.tool_calling,
        req.vision,
        req.thinking,
        req.adaptive_thinking,
        req.input_price_per_1m,
        req.output_price_per_1m,
        req.cache_read_price_per_1m,
    ).await.map_err(db_err)?;

    Ok((StatusCode::CREATED, Json(model_provider_to_response(&mp, req.model_name))))
}

async fn update_provider_model(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path((_provider_id, model_id)): Path<(u64, u64)>,
    Json(req): Json<UpdateModelRequest>,
) -> Result<Json<ProviderModelResponse>, Response> {
    let compatibility: ProviderCompatibility = serde_json::from_str(&format!("\"{}\"", req.compatibility))
        .unwrap_or(ProviderCompatibility::OpenAiChatCompletions);

    let updated = state.store.update_provider_model(
        model_id,
        req.provider_model_id,
        compatibility,
        if req.display_name.is_empty() { "".into() } else { req.display_name },
        None, // description 属于 Model，不在此更新
        req.max_input_tokens,
        req.max_output_tokens,
        req.tool_calling,
        req.vision,
        req.thinking,
        req.adaptive_thinking,
        req.input_price_per_1m,
        req.output_price_per_1m,
        req.cache_read_price_per_1m,
        req.enabled,
    ).await.map_err(db_err)?;

    let model_name = crate::db::models::LLMModel::get_by_id(&mut state.db.clone(), &updated.model_id)
        .await
        .map(|m| m.model_name)
        .unwrap_or_default();
    Ok(Json(model_provider_to_response(&updated, model_name)))
}

async fn delete_provider_model(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path((_provider_id, model_id)): Path<(u64, u64)>,
) -> Result<Response, Response> {
    state.store.delete_provider_model(model_id).await.map_err(db_err)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

// ── models.dev discovery ──

async fn search_models_dev(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<CatalogProviderSummary>>, Response> {
    Ok(Json(state.store.search_catalog_providers(&query.q)))
}

async fn import_models_dev(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Json(req): Json<ImportModelsDevRequest>,
) -> Result<(StatusCode, Json<ImportedProvider>), Response> {
    let result = state.store.import_from_models_dev(&req.provider_id, req.api_keys).await.map_err(db_err)?;
    Ok((StatusCode::CREATED, Json(result)))
}

// ── User management ──

#[derive(Serialize, TS)]
#[ts(export)]
struct UserResponse {
    id: u64,
    oidc_sub: String,
    name: String,
    email: Option<String>,
    role: String,
    active: bool,
    created_at: i64,
}

impl From<models::User> for UserResponse {
    fn from(u: models::User) -> Self {
        Self {
            id: u.id,
            oidc_sub: u.oidc_sub,
            name: u.name,
            email: u.email,
            role: format!("{:?}", u.role).to_lowercase(),
            active: u.active,
            created_at: u.created_at.as_millisecond() / 1000,
        }
    }
}

async fn list_users(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
) -> Result<Json<Vec<UserResponse>>, Response> {
    let users = state.store.list_users().await.map_err(db_err)?;
    Ok(Json(users.into_iter().map(UserResponse::from).collect()))
}

async fn update_user_role(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path(id): Path<u64>,
    Json(req): Json<UpdateRoleRequest>,
) -> Result<Json<UserResponse>, Response> {
    let role = match req.role.to_lowercase().as_str() {
        "admin" => models::UserRole::Admin,
        "member" => models::UserRole::Member,
        _ => return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "invalid role, must be 'admin' or 'member'"}))).into_response()),
    };

    state.store.update_user_role(id, role.clone()).await.map_err(db_err)?;

    // Re-fetch to return updated user
    let users = state.store.list_users().await.map_err(db_err)?;
    let user = users.into_iter().find(|u| u.id == id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "user not found"}))).into_response())?;

    Ok(Json(UserResponse::from(user)))
}
