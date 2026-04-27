use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::Path;

use bincode_next::{Decode, Encode, config::{self, Configuration}};
use fjall::{Database, Keyspace, KeyspaceCreateOptions, PersistMode};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::models::{ProviderCompatibility, ProviderCompatConfig, CompatibilitySettings, ProviderType};
use crate::config::openrouter_catalog::ModelCatalogSnapshot;
use crate::types::LMModelInfo;

const SCHEMA_VERSION: u32 = 2;

pub struct DatabaseRepo {
    db: Database,
    catalog_models: Keyspace,
    pub providers: Keyspace,
    pub provider_models: Keyspace,
    pub provider_secrets: Keyspace,
    metadata: Keyspace,
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Storage(#[from] fjall::Error),
    #[error("encode error: {0}")]
    Encode(#[from] bincode_next::error::EncodeError),
    #[error("decode error: {0}")]
    Decode(#[from] bincode_next::error::DecodeError),
    #[error("provider not found: {0}")]
    ProviderNotFound(String),
    #[error("catalog model not found: {0}")]
    CatalogModelNotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct CatalogModelRecord {
    pub model_name: String,
    pub capabilities: LMModelInfo,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct ProviderRecord {
    pub provider_name: String,
    /// Map of enabled compatibilities with their per-compatibility settings.
    pub compatibilities: HashMap<ProviderCompatibility, ProviderCompatConfig>,
    pub base_url: Option<String>,
}

impl ProviderRecord {
    /// Returns the list of enabled compatibilities for this provider.
    pub fn enabled_compatibilities(&self) -> Vec<&ProviderCompatibility> {
        self.compatibilities
            .iter()
            .filter(|(_, config)| config.enabled)
            .map(|(compat, _)| compat)
            .collect()
    }

    /// Get settings for a specific compatibility if enabled.
    pub fn get_compat_settings(&self, compat: &ProviderCompatibility) -> Option<&CompatibilitySettings> {
        self.compatibilities.get(compat).and_then(|c| {
            if c.enabled {
                c.settings.as_ref()
            } else {
                None
            }
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct ProviderModelRecord {
    pub model_name: String,
    pub provider_name: String,
    pub provider_model_name: String,
    pub priority: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
struct SchemaVersionRecord {
    version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
struct CatalogRefreshRecord {
    fetched_at: i64,
    fetched_count: usize,
    reported_count: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct AvailableModel {
    pub model_name: String,
    pub capabilities: LMModelInfo,
}

#[derive(Debug, Clone)]
pub struct ResolvedProviderRoute {
    pub model_name: String,
    pub capabilities: LMModelInfo,
    pub provider_name: String,
    pub provider_model_name: String,
    pub priority: u32,
    pub compatibility: ProviderCompatibility,
    pub compat_settings: Option<CompatibilitySettings>,
    pub base_url: Option<String>,
    pub api_key: String,
}

impl DatabaseRepo {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let db = Database::builder(path).open()?;
        let catalog_models = db.keyspace("catalog_models", KeyspaceCreateOptions::default)?;
        let providers = db.keyspace("providers", KeyspaceCreateOptions::default)?;
        let provider_models = db.keyspace("provider_models", KeyspaceCreateOptions::default)?;        let provider_secrets = db.keyspace("provider_secrets", KeyspaceCreateOptions::default)?;        let metadata = db.keyspace("metadata", KeyspaceCreateOptions::default)?;

        let repo = Self {
            db,
            catalog_models,
            providers,
            provider_models,
            provider_secrets,
            metadata,
        };

        repo.ensure_schema()?;
        Ok(repo)
    }

    pub fn catalog_model_count(&self) -> Result<usize, DbError> {
        Ok(self.catalog_models.iter().count())
    }

    pub fn replace_catalog(&self, snapshot: ModelCatalogSnapshot) -> Result<(), DbError> {
        clear_keyspace(&self.catalog_models)?;

        let fetched_at = time::OffsetDateTime::now_utc().unix_timestamp();
        let fetched_count = snapshot.fetched_count;
        let reported_count = snapshot.reported_count;

        for (model_name, capabilities) in snapshot.into_models() {
            let record = CatalogModelRecord {
                model_name: model_name.clone(),
                capabilities,
                fetched_at,
            };
            self.catalog_models
                .insert(model_name.as_bytes(), encode(&record)?)?;
        }

        let refresh_record = CatalogRefreshRecord {
            fetched_at,
            fetched_count,
            reported_count,
        };
        self.metadata
            .insert(b"catalog_refresh", encode(&refresh_record)?)?;
        self.purge_invalid_provider_models()?;
        self.db.persist(PersistMode::SyncAll)?;
        Ok(())
    }

    pub fn put_provider(&self, record: &ProviderRecord) -> Result<(), DbError> {
        self.providers
            .insert(record.provider_name.as_bytes(), encode(record)?)?;
        self.db.persist(PersistMode::SyncAll)?;
        Ok(())
    }

    pub fn get_provider(&self, provider_name: &str) -> Result<Option<ProviderRecord>, DbError> {
        let value = self.providers.get(provider_name.as_bytes())?;
        Ok(value.map(|bytes| decode(bytes.as_ref())).transpose()?)
    }

    pub fn list_providers(&self) -> Result<Vec<ProviderRecord>, DbError> {
        let mut providers = Vec::new();
        for guard in self.providers.iter() {
            let (_, value) = guard.into_inner()?;
            providers.push(decode(value.as_ref())?);
        }
        Ok(providers)
    }

    pub fn delete_provider(&self, provider_name: &str) -> Result<bool, DbError> {
        if self.providers.get(provider_name.as_bytes())?.is_none() {
            return Ok(false);
        }

        let mut binding_keys = Vec::new();
        for guard in self.provider_models.iter() {
            let (key, value) = guard.into_inner()?;
            let record: ProviderModelRecord = decode(value.as_ref())?;
            if record.provider_name == provider_name {
                binding_keys.push(key);
            }
        }
        for key in binding_keys {
            self.provider_models.remove(key)?;
        }

        self.provider_secrets.remove(provider_name.as_bytes())?;
        self.providers.remove(provider_name.as_bytes())?;
        self.db.persist(PersistMode::SyncAll)?;
        Ok(true)
    }

    pub fn put_provider_secret(&self, provider_name: &str, api_key: &str) -> Result<(), DbError> {
        self.provider_secrets
            .insert(provider_name.as_bytes(), api_key.as_bytes().to_vec())?;
        self.db.persist(PersistMode::SyncAll)?;
        Ok(())
    }

    pub fn get_provider_secret(&self, provider_name: &str) -> Result<Option<String>, DbError> {
        let value = self.provider_secrets.get(provider_name.as_bytes())?;
        match value {
            Some(bytes) => {
                let secret = String::from_utf8(bytes.to_vec())
                    .map_err(|e| DbError::Decode(bincode_next::error::DecodeError::Other(Box::leak(e.to_string().into_boxed_str()))))?;
                Ok(Some(secret))
            }
            None => Ok(None),
        }
    }

    pub fn delete_provider_secret(&self, provider_name: &str) -> Result<(), DbError> {
        self.provider_secrets.remove(provider_name.as_bytes())?;
        self.db.persist(PersistMode::SyncAll)?;
        Ok(())
    }

    pub fn list_catalog_models(&self) -> Result<Vec<CatalogModelRecord>, DbError> {
        let mut models = Vec::new();
        for guard in self.catalog_models.iter() {
            let (_, value) = guard.into_inner()?;
            models.push(decode(value.as_ref())?);
        }
        Ok(models)
    }

    pub fn list_provider_models_by_provider(
        &self,
        provider_name: &str,
    ) -> Result<Vec<ProviderModelRecord>, DbError> {
        let mut records = Vec::new();
        for guard in self.provider_models.iter() {
            let (_, value) = guard.into_inner()?;
            let record: ProviderModelRecord = decode(value.as_ref())?;
            if record.provider_name == provider_name {
                records.push(record);
            }
        }
        Ok(records)
    }

    pub fn put_provider_model(&self, record: &ProviderModelRecord) -> Result<(), DbError> {
        if self.get_provider(&record.provider_name)?.is_none() {
            return Err(DbError::ProviderNotFound(record.provider_name.clone()));
        }

        if self.get_catalog_model(&record.model_name)?.is_none() {
            return Err(DbError::CatalogModelNotFound(record.model_name.clone()));
        }

        self.provider_models
            .insert(provider_model_key(record).as_bytes(), encode(record)?)?;
        self.db.persist(PersistMode::SyncAll)?;
        Ok(())
    }

    pub fn delete_provider_model(
        &self,
        model_name: &str,
        provider_name: &str,
    ) -> Result<(), DbError> {
        self.provider_models
            .remove(provider_model_key_parts(model_name, provider_name).as_bytes())?;
        self.db.persist(PersistMode::SyncAll)?;
        Ok(())
    }

    pub fn get_catalog_model(
        &self,
        model_name: &str,
    ) -> Result<Option<CatalogModelRecord>, DbError> {
        let value = self.catalog_models.get(model_name.as_bytes())?;
        Ok(value.map(|bytes| decode(bytes.as_ref())).transpose()?)
    }

    pub fn put_catalog_model(&self, record: &CatalogModelRecord) -> Result<(), DbError> {
        self.catalog_models
            .insert(record.model_name.as_bytes(), encode(record)?)?;
        self.db.persist(PersistMode::SyncAll)?;
        Ok(())
    }

    pub fn list_available_models(&self) -> Result<Vec<AvailableModel>, DbError> {
        let mut models = BTreeMap::new();

        for guard in self.provider_models.iter() {
            let (_, value) = guard.into_inner()?;
            let record: ProviderModelRecord = decode(value.as_ref())?;
            if models.contains_key(&record.model_name) {
                continue;
            }

            if let Some(catalog_record) = self.get_catalog_model(&record.model_name)? {
                models.insert(
                    record.model_name.clone(),
                    AvailableModel {
                        model_name: record.model_name,
                        capabilities: catalog_record.capabilities,
                    },
                );
            }
        }

        Ok(models.into_values().collect())
    }

    pub fn resolve_model(&self, model_name: &str) -> Result<Vec<ResolvedProviderRoute>, DbError> {
        let Some(catalog_record) = self.get_catalog_model(model_name)? else {
            return Ok(Vec::new());
        };

        let prefix = format!("{model_name}_");
        let mut routes = Vec::new();

        for guard in self.provider_models.prefix(prefix.as_bytes()) {
            let (_, value) = guard.into_inner()?;
            let model_record: ProviderModelRecord = decode(value.as_ref())?;

            let Some(provider_value) = self.providers.get(model_record.provider_name.as_bytes())?
            else {
                continue;
            };

            let provider_record: ProviderRecord = decode(provider_value.as_ref())?;
            let api_key = match self.get_provider_secret(&provider_record.provider_name) {
                Ok(Some(key)) => key,
                _ => continue,
            };

            // Create one route per enabled compatibility
            for compat in provider_record.enabled_compatibilities() {
                let compat_settings = provider_record.get_compat_settings(compat).cloned();
                routes.push(ResolvedProviderRoute {
                    model_name: model_record.model_name.clone(),
                    capabilities: catalog_record.capabilities.clone(),
                    provider_name: provider_record.provider_name.clone(),
                    provider_model_name: model_record.provider_model_name.clone(),
                    priority: model_record.priority,
                    compatibility: (*compat).clone(),
                    compat_settings,
                    base_url: provider_record.base_url.clone(),
                    api_key: api_key.clone(),
                });
            }
        }

        routes.sort_by_key(|route| route.priority);
        Ok(routes)
    }

    fn purge_invalid_provider_models(&self) -> Result<(), DbError> {
        let mut invalid_keys = Vec::new();

        for guard in self.provider_models.iter() {
            let (key, value) = guard.into_inner()?;
            let record: ProviderModelRecord = decode(value.as_ref())?;
            let catalog_exists = self.get_catalog_model(&record.model_name)?.is_some();
            let provider_exists = self.get_provider(&record.provider_name)?.is_some();

            if !catalog_exists || !provider_exists {
                invalid_keys.push(key);
            }
        }

        for key in invalid_keys {
            self.provider_models.remove(key)?;
        }

        Ok(())
    }

    fn ensure_schema(&self) -> Result<(), DbError> {
        let stored = self.metadata.get(b"schema_version")?;

        match stored {
            Some(bytes) => {
                let schema: SchemaVersionRecord = decode(bytes.as_ref())?;
                if schema.version != SCHEMA_VERSION {
                    // Run migrations based on current version
                    if schema.version == 1 {
                        self.migrate_v1_to_v2()?;
                    }
                    self.metadata.insert(
                        b"schema_version",
                        encode(&SchemaVersionRecord {
                            version: SCHEMA_VERSION,
                        })?,
                    )?;
                    self.db.persist(PersistMode::SyncAll)?;
                }
            }
            None => {
                self.metadata.insert(
                    b"schema_version",
                    encode(&SchemaVersionRecord {
                        version: SCHEMA_VERSION,
                    })?,
                )?;
                self.db.persist(PersistMode::SyncAll)?;
            }
        }

        Ok(())
    }

    /// Migrate from schema v1 (single provider_type) to v2 (multiple compatibilities).
    fn migrate_v1_to_v2(&self) -> Result<(), DbError> {
        let mut providers_to_update = Vec::new();

        for guard in self.providers.iter() {
            let (key, value) = guard.into_inner()?;
            // Try to decode as v1 format first
            let v1_record: V1ProviderRecord = match decode(value.as_ref()) {
                Ok(r) => r,
                Err(_) => continue, // Already migrated or corrupted
            };

            // Map old ProviderType to new ProviderCompatibility
            let compatibilities = match v1_record.provider_type {
                ProviderType::OpenAI => {
                    let mut map = HashMap::new();
                    map.insert(ProviderCompatibility::OpenAiResponses, ProviderCompatConfig {
                        enabled: true,
                        settings: None,
                    });
                    map
                }
                ProviderType::Anthropic => {
                    let mut map = HashMap::new();
                    map.insert(ProviderCompatibility::AnthropicMessages, ProviderCompatConfig {
                        enabled: true,
                        settings: None,
                    });
                    map
                }
                ProviderType::Gemini => {
                    // Gemini not yet implemented, skip or mark as disabled
                    HashMap::new()
                }
            };

            let new_record = ProviderRecord {
                provider_name: v1_record.provider_name,
                compatibilities,
                base_url: v1_record.base_url,
            };

            providers_to_update.push((key, new_record));
        }

        // Write all migrated providers
        for (key, record) in providers_to_update {
            self.providers.insert(key, encode(&record)?)?;
        }

        self.db.persist(PersistMode::SyncAll)?;
        Ok(())
    }
}

/// Legacy v1 provider record for migration purposes only.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
struct V1ProviderRecord {
    provider_name: String,
    provider_type: ProviderType,
    base_url: Option<String>,
}

fn encode<T: Encode>(value: &T) -> Result<Vec<u8>, bincode_next::error::EncodeError> {
    bincode_next::encode_to_vec(value, bincode_config())
}

fn decode<T: Decode<()>>(bytes: &[u8]) -> Result<T, bincode_next::error::DecodeError> {
    let (value, _) = bincode_next::decode_from_slice(bytes, bincode_config())?;
    Ok(value)
}

fn bincode_config() -> Configuration {
    config::standard()
}

fn clear_keyspace(keyspace: &Keyspace) -> Result<(), DbError> {
    let keys = keyspace
        .iter()
        .map(|guard| guard.key())
        .collect::<Result<Vec<_>, _>>()?;

    for key in keys {
        keyspace.remove(key)?;
    }

    Ok(())
}

fn provider_model_key(record: &ProviderModelRecord) -> String {
    provider_model_key_parts(&record.model_name, &record.provider_name)
}

fn provider_model_key_parts(model_name: &str, provider_name: &str) -> String {
    format!("{model_name}_{provider_name}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_db() -> (DatabaseRepo, TempDir) {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let db = DatabaseRepo::open(temp_dir.path()).expect("failed to open test database");
        (db, temp_dir)
    }

    #[test]
    fn test_put_and_get_provider() {
        let (db, _temp) = setup_test_db();

        let mut compatibilities = HashMap::new();
        compatibilities.insert(ProviderCompatibility::OpenAiResponses, ProviderCompatConfig {
            enabled: true,
            settings: None,
        });

        let provider = ProviderRecord {
            provider_name: "test-provider".to_string(),
            compatibilities,
            base_url: Some("https://api.example.com".to_string()),
        };

        db.put_provider(&provider).expect("failed to put provider");

        let retrieved = db
            .get_provider("test-provider")
            .expect("failed to get provider");
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.provider_name, "test-provider");
    }

    #[test]
    fn test_provider_not_found() {
        let (db, _temp) = setup_test_db();
        let result = db.get_provider("nonexistent").expect("should not error");
        assert!(result.is_none());
    }

    #[test]
    fn test_provider_model_referential_integrity() {
        let (db, _temp) = setup_test_db();

        let model_record = ProviderModelRecord {
            model_name: "gpt-4".to_string(),
            provider_name: "nonexistent".to_string(),
            provider_model_name: "gpt-4".to_string(),
            priority: 1,
        };

        let result = db.put_provider_model(&model_record);
        assert!(result.is_err());
    }

    #[test]
    fn test_provider_model_key_format() {
        let key = provider_model_key_parts("gpt-4", "openai");
        assert_eq!(key, "gpt-4_openai");
    }
}
