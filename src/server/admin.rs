//! Admin REST API（Phase 4 + 多协议架构）— Session + Admin 认证。
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
//! | `/api/v1/admin/users` | GET | 列出所有用户 |
//! | `/api/v1/admin/users/{id}/role` | PATCH | 修改用户角色 |

use axfetchum::ApiRouter;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use ractor::Actor;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;
use ts_rs::TS;

use crate::actors::provider::{
    ProviderActor, ProviderChatRequest, ProviderMessage, ProviderRuntimeConfig,
};
use crate::config::models::{ApiKeyEntry, CompatibilitySettings, ProviderQuotaAdapter};
use crate::db::models;
use crate::middleware::session_auth::{AdminAuth, SessionAuth};
use crate::quota::fetch_provider_quota;
use crate::server::AppState;
use crate::store::{self, ApiKeyDisplay, AvailableModel, ModelInput, ProtocolInput};
use crate::types::{LanguageModelChatMessage, LanguageModelInputPart, LanguageModelTextPart};

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
        // Provider protocols
        .get(
            "/api/v1/admin/providers/{id}/protocols",
            list_provider_protocols,
        )
        .response::<Vec<ProtocolView>>()
        .auth()
        .done()
        .put(
            "/api/v1/admin/providers/{id}/protocols",
            replace_provider_protocols,
        )
        .json::<Vec<ProtocolInput>, Vec<ProtocolView>>()
        .auth()
        .done()
        // Provider quota (实时查询上游额度)
        .get("/api/v1/admin/providers/{id}/quota", get_provider_quota)
        .response::<crate::quota::ProviderQuotaResponse>()
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
        .put(
            "/api/v1/admin/providers/{id}/models/{model_id}",
            update_provider_model,
        )
        .json::<UpdateModelRequest, ProviderModelResponse>()
        .auth()
        .done()
        .delete(
            "/api/v1/admin/providers/{id}/models/{model_id}",
            delete_provider_model,
        )
        .auth()
        .done()
        // LLMModels (標称能力 CRUD)
        .get("/api/v1/admin/models", list_admin_models)
        .response::<Vec<AdminModelResponse>>()
        .auth()
        .done()
        .post("/api/v1/admin/models", create_admin_model)
        .json::<ModelInput, AdminModelResponse>()
        .auth()
        .done()
        .get("/api/v1/admin/models/{id}", get_admin_model)
        .response::<AdminModelResponse>()
        .auth()
        .done()
        .put("/api/v1/admin/models/{id}", update_admin_model)
        .json::<ModelInput, AdminModelResponse>()
        .auth()
        .done()
        .delete("/api/v1/admin/models/{id}", delete_admin_model)
        .auth()
        .done()
        // LLMModel 下的 provider 连接（ModelProvider 关联，从模型视角）
        .get("/api/v1/admin/models/{id}/providers", list_model_providers)
        .response::<Vec<ModelLinkView>>()
        .auth()
        .done()
        .post("/api/v1/admin/models/{id}/providers", add_model_provider)
        .json::<AddModelProviderRequest, ModelLinkView>()
        .auth()
        .done()
        .put(
            "/api/v1/admin/models/{id}/providers/{link_id}",
            update_model_provider,
        )
        .json::<UpdateModelProviderRequest, ModelLinkView>()
        .auth()
        .done()
        .post(
            "/api/v1/admin/models/{id}/providers/{link_id}/test",
            test_model_provider_reply,
        )
        .json::<TestModelProviderRequest, TestModelProviderResponse>()
        .auth()
        .done()
        .delete(
            "/api/v1/admin/models/{id}/providers/{link_id}",
            delete_model_provider,
        )
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
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": e.to_string()})),
    )
        .into_response()
}

// ── Models ──

#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
struct ModelProviderSummary {
    provider_id: String,
    provider_display_name: String,
    provider_model_id: String,
    compatibility: crate::config::models::ProviderCompatibility,
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
        let providers = m
            .providers
            .into_iter()
            .map(|p| ModelProviderSummary {
                provider_id: p.provider_id,
                provider_display_name: p.provider_display_name,
                provider_model_id: p.provider_model_id,
                compatibility: p.compatibility,
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
#[serde(rename_all = "camelCase")]
struct ProviderResponse {
    id: u64,
    provider_id: String,
    display_name: String,
    api_keys: Vec<ApiKeyDisplay>,
    enabled: bool,
    priority: i64,
    created_at: i64,
    model_count: usize,
    protocols: Vec<ProtocolView>,
    /// 额度适配器类型；None 表示不查询上游额度。
    quota_adapter: Option<ProviderQuotaAdapter>,
    /// 额度适配器配置（JSON 字符串）。
    quota_adapter_config: Option<String>,
}

fn provider_to_response(
    p: &models::Provider,
    model_count: usize,
    protocols: Vec<ProtocolView>,
) -> ProviderResponse {
    // toasty::Json 通过 Deref 直接获取内部 Vec<ApiKeyEntry>
    let api_keys: &Vec<ApiKeyEntry> = &p.api_keys;

    ProviderResponse {
        id: p.id,
        provider_id: p.provider_id.clone(),
        display_name: p.display_name.clone(),
        api_keys: api_keys
            .iter()
            .map(|k| ApiKeyDisplay {
                label: k.label.clone(),
                weight: k.weight,
                masked_key: store::mask_key(&k.key),
            })
            .collect(),
        enabled: p.enabled,
        priority: p.priority,
        created_at: p.created_at.as_millisecond() / 1000,
        model_count,
        protocols,
        quota_adapter: p.quota_adapter.clone(),
        quota_adapter_config: p.quota_adapter_config.clone(),
    }
}

/// ProviderProtocol 响应。
#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct ProtocolView {
    id: u64,
    provider_id: u64,
    protocol: crate::config::models::ProviderCompatibility,
    base_url: String,
    compat_settings: Option<String>,
    enabled: bool,
    priority: i64,
}

fn protocol_to_view(p: &models::ProviderProtocol) -> ProtocolView {
    ProtocolView {
        id: p.id,
        provider_id: p.provider_id,
        protocol: p.protocol.clone(),
        base_url: p.base_url.clone(),
        compat_settings: p.compat_settings.clone(),
        enabled: p.enabled,
        priority: p.priority,
    }
}

// ── Provider Model response (ModelProvider) ──

#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct ProviderModelResponse {
    id: u64,
    model_id: u64,
    provider_id: u64,
    /// 关联的规范模型名（需要 JOIN 获取）
    model_name: String,
    provider_model_id: String,
    /// 关联的协议 ID（FK → provider_protocols）
    protocol_id: u64,
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
fn model_provider_to_response(
    mp: &models::ModelProvider,
    model_name: String,
) -> ProviderModelResponse {
    ProviderModelResponse {
        id: mp.id,
        model_id: mp.model_id,
        provider_id: mp.provider_id,
        model_name,
        provider_model_id: mp.provider_model_id.clone(),
        protocol_id: mp.protocol_id,
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
    #[serde(default)]
    api_keys: Vec<ApiKeyEntry>,
    /// 协议列表。传入则同步替换该提供者的所有协议（语义与 PUT /protocols/ 一致）；不传或为空 [] 表示该提供者暂时不带任何协议。
    #[serde(default)]
    protocols: Vec<ProtocolInput>,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default = "default_priority")]
    priority: i64,
    /// 额度适配器类型；None = 不查询上游额度。
    #[serde(default)]
    quota_adapter: Option<ProviderQuotaAdapter>,
    /// 额度适配器配置（JSON 字符串）。序列化的 [`crate::config::models::QuotaAdapterConfig`]。
    #[serde(default)]
    quota_adapter_config: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_priority() -> i64 {
    100
}

#[derive(Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct UpdateProviderRequest {
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    api_keys: Vec<ApiKeyEntry>,
    /// 同 CreateProviderRequest.protocols语义：传入则同步替换；为空 [] 表示清空所有协议。
    #[serde(default)]
    protocols: Vec<ProtocolInput>,
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    priority: i64,
    /// 额度适配器类型；未传或为 null 表示不查询上游额度。
    #[serde(default)]
    quota_adapter: Option<ProviderQuotaAdapter>,
    /// 额度适配器配置（JSON 字符串）。
    #[serde(default)]
    quota_adapter_config: Option<String>,
}

#[derive(Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct AddModelRequest {
    /// 规范模型名（如 "openai/gpt-4o"）
    model_name: String,
    /// 提供者侧的模型 ID
    provider_model_id: String,
    /// 协议 ID（FK → provider_protocols）
    protocol_id: u64,
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

fn default_4096() -> i64 {
    4096
}

#[derive(Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct UpdateModelRequest {
    provider_model_id: String,
    protocol_id: u64,
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
struct UpdateRoleRequest {
    role: String,
}

// ── User management ──

async fn list_providers(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
) -> Result<Json<Vec<ProviderResponse>>, Response> {
    let providers = state.store.list_providers().await.map_err(db_err)?;
    let mut result = Vec::new();
    for p in &providers {
        let models = state
            .store
            .list_provider_models(p.id)
            .await
            .unwrap_or_default();
        let protos = state
            .store
            .list_provider_protocols(p.id)
            .await
            .unwrap_or_default();
        let proto_views = protos.iter().map(protocol_to_view).collect();
        result.push(provider_to_response(p, models.len(), proto_views));
    }
    Ok(Json(result))
}

async fn get_provider(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path(id): Path<u64>,
) -> Result<Json<ProviderResponse>, Response> {
    let provider = state
        .store
        .get_provider_by_id(id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "provider not found"})),
            )
                .into_response()
        })?;
    let models = state
        .store
        .list_provider_models(provider.id)
        .await
        .unwrap_or_default();
    let protos = state
        .store
        .list_provider_protocols(provider.id)
        .await
        .map_err(db_err)?;
    let proto_views = protos.iter().map(protocol_to_view).collect();
    Ok(Json(provider_to_response(
        &provider,
        models.len(),
        proto_views,
    )))
}

async fn create_provider(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Json(req): Json<CreateProviderRequest>,
) -> Result<(StatusCode, Json<ProviderResponse>), Response> {
    let display_name = if req.display_name.is_empty() {
        req.provider_id.clone()
    } else {
        req.display_name
    };

    let provider = state
        .store
        .upsert_provider(
            req.provider_id,
            display_name,
            req.api_keys,
            req.enabled,
            req.priority,
            req.quota_adapter,
            req.quota_adapter_config,
        )
        .await
        .map_err(db_err)?;

    // 同步协议：传入则全量替换；传入 [] 表示清空
    let protos = state
        .store
        .replace_provider_protocols(provider.id, req.protocols)
        .await
        .map_err(db_err)?;
    let proto_views = protos.iter().map(protocol_to_view).collect();

    let models = state
        .store
        .list_provider_models(provider.id)
        .await
        .unwrap_or_default();
    Ok((
        StatusCode::CREATED,
        Json(provider_to_response(&provider, models.len(), proto_views)),
    ))
}

async fn update_provider(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path(id): Path<u64>,
    Json(req): Json<UpdateProviderRequest>,
) -> Result<Json<ProviderResponse>, Response> {
    let provider = state
        .store
        .get_provider_by_id(id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "provider not found"})),
            )
                .into_response()
        })?;

    let display_name = if req.display_name.is_empty() {
        provider.display_name.clone()
    } else {
        req.display_name
    };

    // 编辑模式下，前端传空 key 表示「保留原值」：同 label 的现有 key 被保留。
    // 通过 label 匹配回填。完全新增的 key 才直接写入明文。
    let merged_api_keys: Vec<ApiKeyEntry> = req
        .api_keys
        .into_iter()
        .map(|k| {
            if k.key.is_empty() {
                provider
                    .api_keys
                    .iter()
                    .find(|existing| existing.label == k.label)
                    .map(|existing| ApiKeyEntry {
                        label: k.label.clone(),
                        key: existing.key.clone(),
                        weight: k.weight,
                    })
                    .unwrap_or(k)
            } else {
                k
            }
        })
        .collect();

    let updated = state
        .store
        .update_provider_by_id(
            id,
            display_name,
            merged_api_keys,
            req.enabled,
            req.priority,
            req.quota_adapter,
            req.quota_adapter_config,
        )
        .await
        .map_err(db_err)?;

    // 同步协议：传入则全量替换；传入 [] 表示清空
    let protos = state
        .store
        .replace_provider_protocols(updated.id, req.protocols)
        .await
        .map_err(db_err)?;
    let proto_views = protos.iter().map(protocol_to_view).collect();

    let models = state
        .store
        .list_provider_models(updated.id)
        .await
        .unwrap_or_default();
    Ok(Json(provider_to_response(
        &updated,
        models.len(),
        proto_views,
    )))
}

async fn delete_provider(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path(id): Path<u64>,
) -> Result<Response, Response> {
    let existed = state
        .store
        .delete_provider_by_id(id)
        .await
        .map_err(db_err)?;
    if existed {
        Ok(StatusCode::NO_CONTENT.into_response())
    } else {
        Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "provider not found"})),
        )
            .into_response())
    }
}

// ── Provider Protocols CRUD ──

async fn list_provider_protocols(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path(id): Path<u64>,
) -> Result<Json<Vec<ProtocolView>>, Response> {
    // Verify provider exists
    state
        .store
        .get_provider_by_id(id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "provider not found"})),
            )
                .into_response()
        })?;

    let protos = state
        .store
        .list_provider_protocols(id)
        .await
        .map_err(db_err)?;
    Ok(Json(protos.iter().map(protocol_to_view).collect()))
}

async fn replace_provider_protocols(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path(id): Path<u64>,
    Json(inputs): Json<Vec<ProtocolInput>>,
) -> Result<Json<Vec<ProtocolView>>, Response> {
    // Verify provider exists
    state
        .store
        .get_provider_by_id(id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "provider not found"})),
            )
                .into_response()
        })?;

    let protos = state
        .store
        .replace_provider_protocols(id, inputs)
        .await
        .map_err(db_err)?;
    Ok(Json(protos.iter().map(protocol_to_view).collect()))
}

// ── Provider Quota (实时查询上游额度) ──

/// 查询 Provider 的实时额度（per API Key）。
///
/// - Provider 不存在 → `404`
/// - Provider 未配置 `quota_adapter` → `400 Bad Request`
/// - 上游查询失败不影响响应，错误信息放在每个 Key 的 `error` 字段中
async fn get_provider_quota(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path(id): Path<u64>,
) -> Result<Json<crate::quota::ProviderQuotaResponse>, Response> {
    let provider = state
        .store
        .get_provider_by_id(id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "provider not found"})),
            )
                .into_response()
        })?;

    if provider.quota_adapter.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "provider has no quota_adapter configured"
            })),
        )
            .into_response());
    }

    let resp = fetch_provider_quota(&provider)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response()
        })?
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "no adapter registered for this quota_adapter"
                })),
            )
                .into_response()
        })?;

    Ok(Json(resp))
}

// ── Provider Models CRUD ──

async fn list_provider_models(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path(id): Path<u64>,
) -> Result<Json<Vec<ProviderModelResponse>>, Response> {
    // Verify provider exists
    state
        .store
        .get_provider_by_id(id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "provider not found"})),
            )
                .into_response()
        })?;

    let mps = state.store.list_provider_models(id).await.map_err(db_err)?;
    let mut result = Vec::new();
    for mp in &mps {
        let model_name =
            crate::db::models::LLMModel::get_by_id(&mut state.db.clone(), &mp.model_id)
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
    state
        .store
        .get_provider_by_id(id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "provider not found"})),
            )
                .into_response()
        })?;

    let display_name = if req.display_name.is_empty() {
        req.model_name.clone()
    } else {
        req.display_name
    };

    let mp = state
        .store
        .add_provider_model(
            id,
            req.model_name.clone(),
            req.provider_model_id,
            req.protocol_id,
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
        )
        .await
        .map_err(db_err)?;

    Ok((
        StatusCode::CREATED,
        Json(model_provider_to_response(&mp, req.model_name)),
    ))
}

async fn update_provider_model(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path((_provider_id, model_id)): Path<(u64, u64)>,
    Json(req): Json<UpdateModelRequest>,
) -> Result<Json<ProviderModelResponse>, Response> {
    let updated = state
        .store
        .update_provider_model(
            model_id,
            req.provider_model_id,
            req.protocol_id,
            if req.display_name.is_empty() {
                "".into()
            } else {
                req.display_name
            },
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
        )
        .await
        .map_err(db_err)?;

    let model_name =
        crate::db::models::LLMModel::get_by_id(&mut state.db.clone(), &updated.model_id)
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
    state
        .store
        .delete_provider_model(model_id)
        .await
        .map_err(db_err)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

// ── LLMModel management（标称能力 CRUD）──

/// LLMModel 响应（管理视角）— 标称能力 + 该模型下的所有 ModelProvider 连接。
#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct AdminModelResponse {
    id: u64,
    model_name: String,
    display_name: String,
    description: Option<String>,
    // 标称能力
    max_input_tokens: i64,
    max_output_tokens: i64,
    tool_calling: bool,
    vision: bool,
    thinking: bool,
    adaptive_thinking: bool,
    status: Option<String>,
    created_at: i64,
    /// 模型下的连接数（用于列表卡片快速概览）
    provider_count: usize,
}

fn admin_model_to_response(m: &models::LLMModel, provider_count: usize) -> AdminModelResponse {
    AdminModelResponse {
        id: m.id,
        model_name: m.model_name.clone(),
        display_name: m.display_name.clone(),
        description: m.description.clone(),
        max_input_tokens: m.max_input_tokens,
        max_output_tokens: m.max_output_tokens,
        tool_calling: m.tool_calling,
        vision: m.vision,
        thinking: m.thinking,
        adaptive_thinking: m.adaptive_thinking,
        status: m.status.clone(),
        created_at: m.created_at.as_millisecond() / 1000,
        provider_count,
    }
}

/// ModelProvider 从模型视角的关联视图（带 provider 与 protocol 信息）。
#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct ModelLinkView {
    id: u64,
    /// 关联的提供者 row id
    provider_id: u64,
    /// 提供者显示名（JOIN 获取）
    provider_display_name: String,
    /// 提供者侧的模型 ID
    provider_model_id: String,
    /// 关联的协议配置
    protocol_id: u64,
    /// 协议枚举
    protocol: crate::config::models::ProviderCompatibility,
    /// 协议端点 URL（JOIN 获取）
    base_url: String,
    display_name: String,
    // 提供者侧的能力覆盖（None = 使用模型标称值）
    max_input_tokens: Option<i64>,
    max_output_tokens: Option<i64>,
    tool_calling: Option<bool>,
    vision: Option<bool>,
    thinking: Option<bool>,
    adaptive_thinking: Option<bool>,
    // 定价
    input_price_per_1m: Option<f64>,
    output_price_per_1m: Option<f64>,
    cache_read_price_per_1m: Option<f64>,
    enabled: bool,
    priority: i64,
}

async fn model_link_to_view(
    state: &AppState,
    mp: &models::ModelProvider,
) -> Result<ModelLinkView, Response> {
    let provider = crate::db::models::Provider::get_by_id(&mut state.db.clone(), &mp.provider_id)
        .await
        .map_err(db_err)?;
    let protocol =
        crate::db::models::ProviderProtocol::get_by_id(&mut state.db.clone(), &mp.protocol_id)
            .await
            .map_err(db_err)?;

    Ok(ModelLinkView {
        id: mp.id,
        provider_id: mp.provider_id,
        provider_display_name: provider.display_name.clone(),
        provider_model_id: mp.provider_model_id.clone(),
        protocol_id: mp.protocol_id,
        protocol: protocol.protocol.clone(),
        base_url: protocol.base_url.clone(),
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
    })
}

async fn list_admin_models(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
) -> Result<Json<Vec<AdminModelResponse>>, Response> {
    let models = state.store.list_models().await.map_err(db_err)?;
    let mut result = Vec::new();
    for m in &models {
        let count = state
            .store
            .list_model_links(m.id)
            .await
            .unwrap_or_default()
            .len();
        result.push(admin_model_to_response(m, count));
    }
    // 按 model_name 字典序排列，便于查找
    result.sort_by(|a, b| a.model_name.cmp(&b.model_name));
    Ok(Json(result))
}

async fn get_admin_model(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path(id): Path<u64>,
) -> Result<Json<AdminModelResponse>, Response> {
    let m = state
        .store
        .get_model_by_id(id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "model not found"})),
            )
                .into_response()
        })?;
    let count = state
        .store
        .list_model_links(m.id)
        .await
        .unwrap_or_default()
        .len();
    Ok(Json(admin_model_to_response(&m, count)))
}

async fn create_admin_model(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Json(input): Json<ModelInput>,
) -> Result<(StatusCode, Json<AdminModelResponse>), Response> {
    let display_name = if input.display_name.is_empty() {
        input.model_name.clone()
    } else {
        input.display_name.clone()
    };
    let mut input = input;
    input.display_name = display_name;
    let m = state.store.create_model(input).await.map_err(db_err)?;
    Ok((StatusCode::CREATED, Json(admin_model_to_response(&m, 0))))
}

async fn update_admin_model(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path(id): Path<u64>,
    Json(input): Json<ModelInput>,
) -> Result<Json<AdminModelResponse>, Response> {
    let existing = state
        .store
        .get_model_by_id(id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "model not found"})),
            )
                .into_response()
        })?;
    let display_name = if input.display_name.is_empty() {
        existing.display_name.clone()
    } else {
        input.display_name.clone()
    };
    let mut input = input;
    input.display_name = display_name;
    let m = state.store.update_model(id, input).await.map_err(db_err)?;
    let count = state
        .store
        .list_model_links(m.id)
        .await
        .unwrap_or_default()
        .len();
    Ok(Json(admin_model_to_response(&m, count)))
}

async fn delete_admin_model(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path(id): Path<u64>,
) -> Result<Response, Response> {
    let existed = state.store.delete_model(id).await.map_err(db_err)?;
    if existed {
        Ok(StatusCode::NO_CONTENT.into_response())
    } else {
        Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "model not found"})),
        )
            .into_response())
    }
}

// ── LLMModel 的 provider 连接管理（从模型视角）──

/// 从模型视角新增 provider 连接的请求体。
///
/// 与 `/providers/{id}/models` 端点的 AddModelRequest 对称，但 model_id 由 URL 提供，
/// 故请求体不含 model_name。
#[derive(Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct AddModelProviderRequest {
    /// 提供者 row id（FK → providers）
    provider_id: u64,
    /// 提供者侧的模型 ID
    provider_model_id: String,
    /// 关联的协议 ID（FK → provider_protocols）
    protocol_id: u64,
    #[serde(default)]
    display_name: String,
    /// 提供者侧覆盖值；None = 使用模型标称值
    #[serde(default)]
    max_input_tokens: Option<i64>,
    /// 提供者侧覆盖值；None = 使用模型标称值
    #[serde(default)]
    max_output_tokens: Option<i64>,
    /// 提供者侧覆盖值；None = 使用模型标称值
    #[serde(default)]
    tool_calling: Option<bool>,
    /// 提供者侧覆盖值；None = 使用模型标称值
    #[serde(default)]
    vision: Option<bool>,
    /// 提供者侧覆盖值；None = 使用模型标称值
    #[serde(default)]
    thinking: Option<bool>,
    /// 提供者侧覆盖值；None = 使用模型标称值
    #[serde(default)]
    adaptive_thinking: Option<bool>,
    #[serde(default)]
    input_price_per_1m: Option<f64>,
    #[serde(default)]
    output_price_per_1m: Option<f64>,
    #[serde(default)]
    cache_read_price_per_1m: Option<f64>,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default = "default_priority")]
    priority: i64,
}

/// 从模型视角更新 provider 连接的请求体（不含 model_id，URL 提供）。
#[derive(Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct UpdateModelProviderRequest {
    provider_id: u64,
    provider_model_id: String,
    protocol_id: u64,
    #[serde(default)]
    display_name: String,
    /// 提供者侧覆盖值；None = 使用模型标称值（清除已有覆盖）
    max_input_tokens: Option<i64>,
    /// 提供者侧覆盖值；None = 使用模型标称值（清除已有覆盖）
    max_output_tokens: Option<i64>,
    /// 提供者侧覆盖值；None = 使用模型标称值（清除已有覆盖）
    #[serde(default)]
    tool_calling: Option<bool>,
    /// 提供者侧覆盖值；None = 使用模型标称值（清除已有覆盖）
    #[serde(default)]
    vision: Option<bool>,
    /// 提供者侧覆盖值；None = 使用模型标称值（清除已有覆盖）
    #[serde(default)]
    thinking: Option<bool>,
    /// 提供者侧覆盖值；None = 使用模型标称值（清除已有覆盖）
    #[serde(default)]
    adaptive_thinking: Option<bool>,
    #[serde(default)]
    input_price_per_1m: Option<f64>,
    #[serde(default)]
    output_price_per_1m: Option<f64>,
    #[serde(default)]
    cache_read_price_per_1m: Option<f64>,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default = "default_priority")]
    priority: i64,
}

#[derive(Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct TestModelProviderRequest {
    /// 可选自定义测试提示词。为空时使用服务端默认提示词。
    #[serde(default)]
    prompt: Option<String>,
}

#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
struct TestModelProviderResponse {
    success: bool,
    latency_ms: u64,
    model_id: u64,
    link_id: u64,
    provider_id: u64,
    provider_display_name: String,
    provider_model_id: String,
    protocol: crate::config::models::ProviderCompatibility,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

async fn list_model_providers(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path(id): Path<u64>,
) -> Result<Json<Vec<ModelLinkView>>, Response> {
    // Verify model exists
    state
        .store
        .get_model_by_id(id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "model not found"})),
            )
                .into_response()
        })?;

    let links = state.store.list_model_links(id).await.map_err(db_err)?;
    let mut views = Vec::new();
    for mp in &links {
        views.push(model_link_to_view(&state, mp).await?);
    }
    Ok(Json(views))
}

async fn add_model_provider(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path(id): Path<u64>,
    Json(req): Json<AddModelProviderRequest>,
) -> Result<(StatusCode, Json<ModelLinkView>), Response> {
    // Verify model exists
    state
        .store
        .get_model_by_id(id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "model not found"})),
            )
                .into_response()
        })?;
    // Verify provider exists
    let _provider = state
        .store
        .get_provider_by_id(req.provider_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "provider not found"})),
            )
                .into_response()
        })?;
    // Verify protocol exists and belongs to this provider
    let proto =
        crate::db::models::ProviderProtocol::get_by_id(&mut state.db.clone(), &req.protocol_id)
            .await
            .map_err(db_err)?;
    if proto.provider_id != req.provider_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "protocol does not belong to provider"})),
        )
            .into_response());
    }

    let display_name = if req.display_name.is_empty() {
        req.provider_model_id.clone()
    } else {
        req.display_name.clone()
    };

    let mp = toasty::create!(models::ModelProvider {
        model_id: id,
        provider_id: req.provider_id,
        provider_model_id: req.provider_model_id,
        protocol_id: req.protocol_id,
        display_name,
        max_input_tokens: req.max_input_tokens,
        max_output_tokens: req.max_output_tokens,
        tool_calling: req.tool_calling,
        vision: req.vision,
        thinking: req.thinking,
        adaptive_thinking: req.adaptive_thinking,
        input_price_per_1m: req.input_price_per_1m,
        output_price_per_1m: req.output_price_per_1m,
        cache_read_price_per_1m: req.cache_read_price_per_1m,
        enabled: req.enabled,
        priority: req.priority,
    })
    .exec(&mut state.db.clone())
    .await
    .map_err(|e| db_err(e.to_string()))?;

    let view = model_link_to_view(&state, &mp).await?;
    Ok((StatusCode::CREATED, Json(view)))
}

async fn update_model_provider(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path((_model_id, link_id)): Path<(u64, u64)>,
    Json(req): Json<UpdateModelProviderRequest>,
) -> Result<Json<ModelLinkView>, Response> {
    // Verify protocol exists and belongs to the request's provider
    let proto =
        crate::db::models::ProviderProtocol::get_by_id(&mut state.db.clone(), &req.protocol_id)
            .await
            .map_err(db_err)?;
    if proto.provider_id != req.provider_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "protocol does not belong to provider"})),
        )
            .into_response());
    }

    crate::db::models::ModelProvider::filter(
        crate::db::models::ModelProvider::fields().id().eq(link_id),
    )
    .update()
    .provider_id(req.provider_id)
    .provider_model_id(req.provider_model_id)
    .protocol_id(req.protocol_id)
    .display_name(req.display_name)
    .max_input_tokens(req.max_input_tokens)
    .max_output_tokens(req.max_output_tokens)
    .tool_calling(req.tool_calling)
    .vision(req.vision)
    .thinking(req.thinking)
    .adaptive_thinking(req.adaptive_thinking)
    .input_price_per_1m(req.input_price_per_1m)
    .output_price_per_1m(req.output_price_per_1m)
    .cache_read_price_per_1m(req.cache_read_price_per_1m)
    .enabled(req.enabled)
    .priority(req.priority)
    .exec(&mut state.db.clone())
    .await
    .map_err(|e| db_err(e.to_string()))?;

    let mp = crate::db::models::ModelProvider::get_by_id(&mut state.db.clone(), &link_id)
        .await
        .map_err(db_err)?;
    Ok(Json(model_link_to_view(&state, &mp).await?))
}

async fn test_model_provider_reply(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path((model_id, link_id)): Path<(u64, u64)>,
    Json(req): Json<TestModelProviderRequest>,
) -> Result<Json<TestModelProviderResponse>, Response> {
    let model = state
        .store
        .get_model_by_id(model_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "model not found"})),
            )
                .into_response()
        })?;

    let link = crate::db::models::ModelProvider::get_by_id(&mut state.db.clone(), &link_id)
        .await
        .map_err(db_err)?;
    if link.model_id != model.id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "link does not belong to model"})),
        )
            .into_response());
    }

    let provider = crate::db::models::Provider::get_by_id(&mut state.db.clone(), &link.provider_id)
        .await
        .map_err(db_err)?;
    let protocol =
        crate::db::models::ProviderProtocol::get_by_id(&mut state.db.clone(), &link.protocol_id)
            .await
            .map_err(db_err)?;

    if protocol.provider_id != provider.id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "protocol does not belong to provider"})),
        )
            .into_response());
    }
    if !provider.enabled {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "provider is disabled"})),
        )
            .into_response());
    }
    if !protocol.enabled {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "protocol is disabled"})),
        )
            .into_response());
    }
    if !link.enabled {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "model provider link is disabled"})),
        )
            .into_response());
    }

    let api_keys: &Vec<ApiKeyEntry> = &provider.api_keys;
    let key = api_keys
        .iter()
        .find(|k| !k.key.trim().is_empty())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "provider has no valid api key"})),
            )
                .into_response()
        })?;

    let compat_settings: Option<CompatibilitySettings> = protocol
        .compat_settings
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok());

    let provider_config = ProviderRuntimeConfig {
        id: provider.provider_id.clone(),
        compatibility: protocol.protocol.clone(),
        api_key: key.key.clone(),
        base_url: Some(protocol.base_url.clone()),
        compat_settings,
    };

    let prompt = req.prompt.unwrap_or_else(|| "请仅回复 pong".to_string());
    let request = ProviderChatRequest {
        model: link.provider_model_id.clone(),
        messages: vec![LanguageModelChatMessage::user(
            vec![LanguageModelInputPart::Text(LanguageModelTextPart {
                value: prompt,
            })],
            None,
        )],
        tools: None,
        tool_choice: None,
        temperature: None,
        max_tokens: None,
        top_p: None,
        stop: None,
        response_format: None,
        reasoning: None,
    };

    let started = std::time::Instant::now();

    let (provider_ref, provider_handle) = Actor::spawn(None, ProviderActor, provider_config)
        .await
        .map_err(|e| db_err(e.to_string()))?;

    let (metadata_tx, _metadata_rx) =
        tokio::sync::oneshot::channel::<crate::actors::provider::ProviderResponseMetadata>();

    let mut stream = match ractor::call_t!(
        provider_ref,
        |reply| ProviderMessage::ChatRequest(request, reply, metadata_tx),
        10_000
    ) {
        Ok(Ok(stream)) => stream,
        Ok(Err(e)) => {
            provider_ref.stop(None);
            let _ = provider_handle.await;
            let resp = TestModelProviderResponse {
                success: false,
                latency_ms: started.elapsed().as_millis() as u64,
                model_id,
                link_id,
                provider_id: provider.id,
                provider_display_name: provider.display_name.clone(),
                provider_model_id: link.provider_model_id.clone(),
                protocol: protocol.protocol.clone(),
                error: Some(e),
            };
            return Ok(Json(resp));
        }
        Err(e) => {
            provider_ref.stop(None);
            let _ = provider_handle.await;
            let resp = TestModelProviderResponse {
                success: false,
                latency_ms: started.elapsed().as_millis() as u64,
                model_id,
                link_id,
                provider_id: provider.id,
                provider_display_name: provider.display_name.clone(),
                provider_model_id: link.provider_model_id.clone(),
                protocol: protocol.protocol.clone(),
                error: Some(e.to_string()),
            };
            return Ok(Json(resp));
        }
    };

    let result = tokio::time::timeout(std::time::Duration::from_secs(10), stream.next()).await;

    provider_ref.stop(None);
    let _ = provider_handle.await;

    let (success, error) = match result {
        Ok(Some(Ok(_part))) => (true, None),
        Ok(Some(Err(e))) => (false, Some(e)),
        Ok(None) => (false, Some("upstream returned no response".to_string())),
        Err(_) => (false, Some("test timed out after 10s".to_string())),
    };

    Ok(Json(TestModelProviderResponse {
        success,
        latency_ms: started.elapsed().as_millis() as u64,
        model_id,
        link_id,
        provider_id: provider.id,
        provider_display_name: provider.display_name,
        provider_model_id: link.provider_model_id,
        protocol: protocol.protocol,
        error,
    }))
}

async fn delete_model_provider(
    State(state): State<AppState>,
    AdminAuth(_user): AdminAuth,
    Path((_model_id, link_id)): Path<(u64, u64)>,
) -> Result<Response, Response> {
    state
        .store
        .delete_provider_model(link_id)
        .await
        .map_err(db_err)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

// ── User management ──

#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
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
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid role, must be 'admin' or 'member'"})),
            )
                .into_response());
        }
    };

    state
        .store
        .update_user_role(id, role.clone())
        .await
        .map_err(db_err)?;

    // Re-fetch to return updated user
    let users = state.store.list_users().await.map_err(db_err)?;
    let user = users.into_iter().find(|u| u.id == id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "user not found"})),
        )
            .into_response()
    })?;

    Ok(Json(UserResponse::from(user)))
}
