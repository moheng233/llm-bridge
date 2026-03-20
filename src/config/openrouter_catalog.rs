use std::collections::HashMap;
use std::time::Duration;

use reqwest::StatusCode;
use serde::Deserialize;

use crate::config::models::ModelCatalogConfig;
use crate::types::{EndpointEditToolName, LMModelInfo};

#[derive(Debug, Clone)]
pub struct ModelCatalogSnapshot {
    models: HashMap<String, LMModelInfo>,
    pub fetched_count: usize,
    pub reported_count: Option<usize>,
}

impl ModelCatalogSnapshot {
    pub fn get(&self, canonical_model: &str) -> Option<&LMModelInfo> {
        self.models.get(canonical_model)
    }

    pub fn len(&self) -> usize {
        self.models.len()
    }
}

pub struct OpenRouterCatalogClient {
    client: reqwest::Client,
    base_url: String,
    output_modalities: String,
    check_count: bool,
}

impl OpenRouterCatalogClient {
    pub fn new(config: &ModelCatalogConfig, api_key: &str) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.request_timeout_secs))
            .user_agent(concat!(
                env!("CARGO_PKG_NAME"),
                "/",
                env!("CARGO_PKG_VERSION")
            ))
            .default_headers(build_auth_headers(api_key)?)
            .build()
            .map_err(|error| format!("failed to build openrouter catalog client: {error}"))?;

        Ok(Self {
            client,
            base_url: config.base_url.trim_end_matches('/').to_string(),
            output_modalities: config.output_modalities.clone(),
            check_count: config.count_consistency_check,
        })
    }

    pub async fn fetch_snapshot(&self) -> Result<ModelCatalogSnapshot, String> {
        let list_url = format!(
            "{}/models?output_modalities={}",
            self.base_url, self.output_modalities
        );
        let list_response = self
            .client
            .get(list_url)
            .send()
            .await
            .map_err(|error| format!("openrouter models request failed: {error}"))?;

        let list_response = ensure_success_status("models", list_response).await?;

        let payload = list_response
            .json::<OpenRouterModelsResponse>()
            .await
            .map_err(|error| format!("failed to decode openrouter models response: {error}"))?;

        let mut models = HashMap::new();
        for model in payload.data {
            let canonical_name = model
                .canonical_slug
                .as_deref()
                .unwrap_or(model.id.as_str())
                .to_string();
            models.insert(canonical_name.clone(), map_model(model, &canonical_name));
        }

        let fetched_count = models.len();
        let reported_count = if self.check_count {
            Some(self.fetch_count().await?)
        } else {
            None
        };

        Ok(ModelCatalogSnapshot {
            models,
            fetched_count,
            reported_count,
        })
    }

    async fn fetch_count(&self) -> Result<usize, String> {
        let count_url = format!(
            "{}/models/count?output_modalities={}",
            self.base_url, self.output_modalities
        );
        let response = self
            .client
            .get(count_url)
            .send()
            .await
            .map_err(|error| format!("openrouter model count request failed: {error}"))?;

        let response = ensure_success_status("models/count", response).await?;

        let payload = response
            .json::<OpenRouterModelCountResponse>()
            .await
            .map_err(|error| {
                format!("failed to decode openrouter model count response: {error}")
            })?;

        Ok(payload.data.count)
    }
}

fn build_auth_headers(api_key: &str) -> Result<reqwest::header::HeaderMap, String> {
    let mut headers = reqwest::header::HeaderMap::new();
    let token = format!("Bearer {api_key}");
    let value = reqwest::header::HeaderValue::from_str(&token)
        .map_err(|error| format!("invalid api key header value: {error}"))?;
    headers.insert(reqwest::header::AUTHORIZATION, value);
    Ok(headers)
}

async fn ensure_success_status(
    label: &str,
    response: reqwest::Response,
) -> Result<reqwest::Response, String> {
    if response.status().is_success() {
        return Ok(response);
    }

    let status: StatusCode = response.status();
    let body = response
        .text()
        .await
        .unwrap_or_else(|_| "<unavailable>".to_string());

    let error = format!(
        "openrouter {} request failed with status {}: {}",
        label,
        status.as_u16(),
        body
    );

    Err(error)
}

fn map_model(model: OpenRouterModel, canonical_name: &str) -> LMModelInfo {
    let tool_calling = model.supported_parameters.iter().any(|parameter| {
        parameter == "tools" || parameter == "tool_choice" || parameter == "function_call"
    });

    let vision = model
        .architecture
        .as_ref()
        .map(|architecture| {
            architecture
                .input_modalities
                .iter()
                .chain(architecture.output_modalities.iter())
                .any(|modality| modality == "image")
        })
        .unwrap_or(false);

    let max_input_tokens = model.context_length.unwrap_or(4096);
    let max_output_tokens = model
        .top_provider
        .as_ref()
        .and_then(|provider| provider.max_completion_tokens)
        .unwrap_or(max_input_tokens);

    LMModelInfo {
        name: canonical_name.to_string(),
        max_input_tokens,
        max_output_tokens,
        tool_calling,
        vision,
        thinking: None,
        adaptive_thinking: None,
        edit_tools: EndpointEditToolName::empty(),
    }
}

#[derive(Debug, Deserialize)]
struct OpenRouterModelsResponse {
    data: Vec<OpenRouterModel>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModel {
    id: String,
    canonical_slug: Option<String>,
    context_length: Option<u32>,
    #[serde(default)]
    supported_parameters: Vec<String>,
    top_provider: Option<OpenRouterTopProvider>,
    architecture: Option<OpenRouterArchitecture>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterTopProvider {
    max_completion_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterArchitecture {
    #[serde(default)]
    input_modalities: Vec<String>,
    #[serde(default)]
    output_modalities: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModelCountResponse {
    data: OpenRouterModelCountData,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModelCountData {
    count: usize,
}
