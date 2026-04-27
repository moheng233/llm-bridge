use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::models::ProviderConfig;

use super::error::StoreError;

const PROVIDERS_FILE: &str = "providers.json";

#[derive(Debug, Serialize, Deserialize)]
struct ProvidersFile {
    providers: HashMap<String, ProviderConfig>,
}

pub(super) fn load_providers(
    dir: &Path,
) -> Result<HashMap<String, ProviderConfig>, StoreError> {
    let path = dir.join(PROVIDERS_FILE);
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let content = std::fs::read_to_string(&path)?;
    if content.trim().is_empty() {
        return Ok(HashMap::new());
    }

    let file: ProvidersFile = serde_json::from_str(&content)?;
    Ok(file.providers)
}

pub(super) fn save_providers(
    dir: &Path,
    providers: &HashMap<String, ProviderConfig>,
) -> Result<(), StoreError> {
    let path = dir.join(PROVIDERS_FILE);
    let file = ProvidersFile {
        providers: providers.clone(),
    };
    let content = serde_json::to_string_pretty(&file)?;
    std::fs::write(&path, content)?;
    Ok(())
}
