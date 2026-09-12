//! 管理员只读目录预览；仅供首次预填，网络数据不会参与运行时路由。
use super::AppState;
use crate::{
    config::models::ProviderCompatibility,
    db::models::{LLMModel, ModelProvider, Provider, ProviderProtocol},
    middleware::session_auth::AdminAuth,
    store::ModelInput,
};
use axfetchum::ApiRouter;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CatalogProvider {
    pub provider_id: String,
    pub display_name: String,
    pub base_url: String,
    pub compat: ProviderCompatibility,
}
impl CatalogProvider {
    pub fn protocol_key(&self) -> String {
        let compat = match self.compat {
            ProviderCompatibility::OpenAiChatCompletions => "openAiChatCompletions",
            ProviderCompatibility::OpenAiResponses => "openAiResponses",
            ProviderCompatibility::AnthropicMessages => "anthropicMessages",
        };
        format!("{compat}|{}", self.base_url)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CatalogLink {
    pub provider_id: String,
    pub protocol_key: String,
    pub model_name: String,
    pub provider_model_id: String,
    pub input_price_per_1m: Option<f64>,
    pub output_price_per_1m: Option<f64>,
    pub cache_read_price_per_1m: Option<f64>,
    pub enabled: bool,
    pub max_input_tokens: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub tool_calling: Option<bool>,
    pub vision: Option<bool>,
    pub thinking: Option<bool>,
    pub adaptive_thinking: Option<bool>,
}
impl CatalogLink {
    pub fn key(&self) -> String {
        serde_json::to_string(&(
            &self.provider_id,
            &self.protocol_key,
            &self.model_name,
            &self.provider_model_id,
        ))
        .expect("string tuple is serializable")
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub schema_version: u32,
    pub source_rev: String,
    pub generated_at: String,
    pub models: Vec<ModelInput>,
    pub providers: Vec<CatalogProvider>,
    pub links: Vec<CatalogLink>,
}
impl Catalog {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err(format!(
                "unsupported catalog schemaVersion {}",
                self.schema_version
            ));
        }
        self.generated_at
            .parse::<jiff::Timestamp>()
            .map_err(|_| "invalid catalog generatedAt")?;
        if self.source_rev.is_empty() {
            return Err("catalog sourceRev is empty".into());
        }
        let mut models = HashSet::new();
        for model in &self.models {
            if model.model_name.is_empty() || !models.insert(model.model_name.as_str()) {
                return Err("empty or duplicate modelName".into());
            }
            for (field, count) in [
                ("maxInputTokens", model.max_input_tokens),
                ("maxOutputTokens", model.max_output_tokens),
            ] {
                if !(1..=u32::MAX as i64).contains(&count) {
                    return Err(format!(
                        "{}.{field}: expected integer 1..4294967295, got {count}",
                        model.model_name
                    ));
                }
            }
        }
        let mut providers = HashMap::new();
        for provider in &self.providers {
            if provider.provider_id.is_empty()
                || providers
                    .insert(provider.provider_id.as_str(), provider.protocol_key())
                    .is_some()
            {
                return Err("empty or duplicate providerId".into());
            }
            let url =
                reqwest::Url::parse(&provider.base_url).map_err(|_| "invalid provider baseUrl")?;
            if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
                return Err("provider baseUrl must use HTTP(S)".into());
            }
        }
        let mut links = HashSet::new();
        for link in &self.links {
            if !models.contains(link.model_name.as_str())
                || providers.get(link.provider_id.as_str()) != Some(&link.protocol_key)
            {
                return Err(format!(
                    "dangling model/provider/protocol reference in {}",
                    link.key()
                ));
            }
            if link.provider_model_id.is_empty() || !links.insert(link.key()) {
                return Err("empty or duplicate provider model identity".into());
            }
            for price in [
                link.input_price_per_1m,
                link.output_price_per_1m,
                link.cache_read_price_per_1m,
            ]
            .into_iter()
            .flatten()
            {
                if !price.is_finite() || price < 0.0 {
                    return Err("catalog prices must be finite and nonnegative".into());
                }
            }
            for (field, count) in [
                ("maxInputTokens", link.max_input_tokens),
                ("maxOutputTokens", link.max_output_tokens),
            ] {
                if let Some(count) = count
                    && !(1..=u32::MAX as i64).contains(&count)
                {
                    return Err(format!(
                        "{} / {} ({}).{field}: expected integer 1..4294967295, got {count}",
                        link.provider_id, link.provider_model_id, link.model_name
                    ));
                }
            }
        }
        Ok(())
    }
}
struct CachedCatalog {
    catalog: Arc<Catalog>,
    etag: Option<String>,
    modified: Option<String>,
}
pub struct CatalogService {
    source_url: String,
    cache: Mutex<Option<CachedCatalog>>,
}
impl CatalogService {
    pub fn new(source_url: String) -> Self {
        Self {
            source_url,
            cache: Mutex::new(None),
        }
    }
    pub async fn load(&self) -> Result<Arc<Catalog>, String> {
        let mut cache = self.cache.lock().await;
        let source = reqwest::Url::parse(&self.source_url).map_err(|error| error.to_string())?;
        if !matches!(source.scheme(), "http" | "https") {
            return Err("catalog source must use HTTP(S)".into());
        }
        let client = crate::http::client_builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| error.to_string())?;
        let manifest = client
            .get(
                source
                    .join("contract.json")
                    .map_err(|error| error.to_string())?,
            )
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;
        let manifest: serde_json::Value =
            serde_json::from_slice(&bounded_body(manifest, 64 * 1024).await?)
                .map_err(|error| error.to_string())?;
        if manifest
            .get("schemaVersion")
            .and_then(serde_json::Value::as_u64)
            != Some(1)
        {
            return Err("unsupported contract schemaVersion".into());
        }
        let mut request = client.get(source);
        if let Some(cached) = cache.as_ref() {
            if let Some(etag) = &cached.etag {
                request = request.header(reqwest::header::IF_NONE_MATCH, etag);
            }
            if let Some(modified) = &cached.modified {
                request = request.header(reqwest::header::IF_MODIFIED_SINCE, modified);
            }
        }
        let response = request.send().await.map_err(|error| error.to_string())?;
        if response.status() == StatusCode::NOT_MODIFIED {
            return cache
                .as_ref()
                .map(|cached| cached.catalog.clone())
                .ok_or_else(|| "catalog returned 304 without a cached body".into());
        }
        let response = response
            .error_for_status()
            .map_err(|error| error.to_string())?;
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let modified = response
            .headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let catalog: Catalog =
            serde_json::from_slice(&bounded_body(response, 16 * 1024 * 1024).await?)
                .map_err(|error| error.to_string())?;
        catalog
            .validate()
            .map_err(|error| format!("目录数据校验失败：{error}"))?;
        let catalog = Arc::new(catalog);
        *cache = Some(CachedCatalog {
            catalog: catalog.clone(),
            etag,
            modified,
        });
        Ok(catalog)
    }
}
async fn bounded_body(response: reqwest::Response, limit: usize) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|size| size > limit as u64)
    {
        return Err("catalog response exceeds size limit".into());
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        if chunk.len() > limit - body.len() {
            return Err("catalog response exceeds size limit".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}
#[derive(Serialize, TS)]
#[ts(export)]
pub struct CatalogModelPreview {
    pub model: ModelInput,
    pub exists: bool,
}
#[derive(Serialize, TS)]
#[ts(export)]
pub struct CatalogProviderPreview {
    pub provider: CatalogProvider,
    pub exists: bool,
}
#[derive(Serialize, TS)]
#[ts(export)]
pub struct CatalogLinkPreview {
    pub key: String,
    pub link: CatalogLink,
    pub exists: bool,
}
#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CatalogPreview {
    pub source_rev: String,
    pub generated_at: String,
    pub models: Vec<CatalogModelPreview>,
    pub providers: Vec<CatalogProviderPreview>,
    pub links: Vec<CatalogLinkPreview>,
}

pub fn routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .group("modelsImport")
        .get("/api/v1/admin/models-import/preview", preview)
        .response::<CatalogPreview>()
        .auth()
        .done()
}
fn error(status: StatusCode, message: String) -> Response {
    (status, Json(serde_json::json!({"error":message}))).into_response()
}
async fn preview(
    State(state): State<AppState>,
    AdminAuth(_): AdminAuth,
) -> Result<Json<CatalogPreview>, Response> {
    let catalog = state
        .catalog
        .load()
        .await
        .map_err(|message| error(StatusCode::BAD_GATEWAY, message))?;
    preview_catalog(&state.db, &catalog)
        .await
        .map(Json)
        .map_err(|message| error(StatusCode::INTERNAL_SERVER_ERROR, message))
}

pub async fn preview_catalog(
    db: &crate::db::Db,
    catalog: &Catalog,
) -> Result<CatalogPreview, String> {
    let mut db = db.clone();
    let models = LLMModel::all()
        .exec(&mut db)
        .await
        .map_err(|error| error.to_string())?;
    let providers = Provider::all()
        .exec(&mut db)
        .await
        .map_err(|error| error.to_string())?;
    let protocols = ProviderProtocol::all()
        .exec(&mut db)
        .await
        .map_err(|error| error.to_string())?;
    let links = ModelProvider::all()
        .exec(&mut db)
        .await
        .map_err(|error| error.to_string())?;
    let model_ids: HashMap<_, _> = models
        .iter()
        .map(|model| (model.model_name.as_str(), model.id))
        .collect();
    let provider_ids: HashMap<_, _> = providers
        .iter()
        .map(|provider| (provider.provider_id.as_str(), provider.id))
        .collect();
    let mut protocol_ids = HashMap::new();
    for provider in &catalog.providers {
        if let Some(id) = provider_ids.get(provider.provider_id.as_str())
            && let Some(protocol) = protocols
                .iter()
                .filter(|protocol| {
                    protocol.provider_id == *id
                        && protocol.protocol == provider.compat
                        && protocol.base_url == provider.base_url
                })
                .min_by_key(|protocol| protocol.id)
        {
            protocol_ids.insert(provider.provider_id.as_str(), protocol.id);
        }
    }
    let links = catalog
        .links
        .iter()
        .map(|link| {
            let exists = model_ids
                .get(link.model_name.as_str())
                .zip(protocol_ids.get(link.provider_id.as_str()))
                .is_some_and(|(model, protocol)| {
                    links.iter().any(|row| {
                        row.model_id == *model
                            && row.protocol_id == *protocol
                            && row.provider_model_id == link.provider_model_id
                    })
                });
            CatalogLinkPreview {
                key: link.key(),
                link: link.clone(),
                exists,
            }
        })
        .collect();
    Ok(CatalogPreview {
        source_rev: catalog.source_rev.clone(),
        generated_at: catalog.generated_at.clone(),
        models: catalog
            .models
            .iter()
            .map(|model| CatalogModelPreview {
                exists: model_ids.contains_key(model.model_name.as_str()),
                model: model.clone(),
            })
            .collect(),
        providers: catalog
            .providers
            .iter()
            .map(|provider| CatalogProviderPreview {
                exists: provider_ids.contains_key(provider.provider_id.as_str()),
                provider: provider.clone(),
            })
            .collect(),
        links,
    })
}
