/// Models.dev API client — downloads https://models.dev/api.json.
use std::path::Path;
use std::time::Duration;

use reqwest::StatusCode;
use tracing::{debug, info, instrument};

use crate::config::models::ModelCatalogConfig;
use crate::models_dev::{CatalogCache, ModelsDevRoot};
use crate::store::StoreMetadata;

pub struct ModelsDevCatalogClient {
    client: reqwest::Client,
    base_url: String,
    timeout: Duration,
}

impl ModelsDevCatalogClient {
    pub fn new(config: &ModelCatalogConfig) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.request_timeout_secs))
            .user_agent(concat!(
                env!("CARGO_PKG_NAME"),
                "/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .map_err(|error| format!("failed to build models.dev catalog client: {error}"))?;

        Ok(Self {
            client,
            base_url: config.base_url.trim_end_matches('/').to_string(),
            timeout: Duration::from_secs(config.request_timeout_secs),
        })
    }

    /// Fetch the catalog from models.dev, optionally using a conditional request (ETag).
    #[instrument(level = "info", skip(self), fields(base_url = %self.base_url))]
    pub async fn fetch(&self, etag: Option<&str>) -> Result<(ModelsDevRoot, StoreMetadata), String> {
        let url = format!("{}/api.json", self.base_url);
        debug!("requesting models.dev catalog");

        let mut req = self.client.get(&url);
        if let Some(etag) = etag {
            req = req.header("If-None-Match", etag);
        }

        let response = req
            .send()
            .await
            .map_err(|error| format!("models.dev request failed: {error}"))?;

        // If 304 Not Modified, the catalog hasn't changed.
        if response.status() == StatusCode::NOT_MODIFIED {
            info!("models.dev catalog unchanged (304)");
            return Err("unchanged".to_string());
        }

        let status = response.status();
        let response_etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unavailable>".to_string());
            return Err(format!(
                "models.dev request failed (status {}): {}",
                status.as_u16(),
                body
            ));
        }

        let data: ModelsDevRoot = response
            .json()
            .await
            .map_err(|error| format!("failed to parse models.dev response: {error}"))?;

        let fetched_at = time::OffsetDateTime::now_utc().unix_timestamp();

        info!(
            provider_count = data.len(),
            "models.dev catalog fetched successfully"
        );

        Ok((
            data,
            StoreMetadata {
                fetched_at,
                etag: response_etag,
            },
        ))
    }

    /// Try loading catalog from the local cache file.
    #[instrument(level = "debug")]
    pub fn load_cache(path: &Path) -> Result<Option<(ModelsDevRoot, StoreMetadata)>, String> {
        let cache_path = path.join("catalog_cache.json");
        if !cache_path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&cache_path)
            .map_err(|e| format!("failed to read cache: {e}"))?;
        if content.trim().is_empty() {
            return Ok(None);
        }

        let cache: CatalogCache = serde_json::from_str(&content)
            .map_err(|e| format!("failed to parse cache: {e}"))?;

        Ok(Some((
            cache.data,
            StoreMetadata {
                fetched_at: cache.fetched_at,
                etag: cache.etag,
            },
        )))
    }
}
