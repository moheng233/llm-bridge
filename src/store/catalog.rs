use std::path::Path;

use crate::models_dev::{CatalogCache, ModelsDevRoot};

use super::error::StoreError;
use super::StoreMetadata;

const CATALOG_CACHE_FILE: &str = "catalog_cache.json";

pub(super) fn load_catalog_cache(
    dir: &Path,
) -> Result<(ModelsDevRoot, StoreMetadata), StoreError> {
    let path = dir.join(CATALOG_CACHE_FILE);
    if !path.exists() {
        return Ok((ModelsDevRoot::new(), StoreMetadata {
            fetched_at: 0,
            etag: None,
        }));
    }

    let content = std::fs::read_to_string(&path)?;
    if content.trim().is_empty() {
        return Ok((ModelsDevRoot::new(), StoreMetadata {
            fetched_at: 0,
            etag: None,
        }));
    }

    let cache: CatalogCache = serde_json::from_str(&content)?;
    Ok((
        cache.data,
        StoreMetadata {
            fetched_at: cache.fetched_at,
            etag: cache.etag,
        },
    ))
}

pub(super) fn save_catalog_cache(
    dir: &Path,
    cache: &CatalogCache,
) -> Result<(), StoreError> {
    let path = dir.join(CATALOG_CACHE_FILE);
    let content = serde_json::to_string(cache)?;
    std::fs::write(&path, content)?;
    Ok(())
}
