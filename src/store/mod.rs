//! 数据存储层（Phase 3 重写）— 基于 toasty + SQLite。
//!
//! 替代旧的 JSON 文件 + RwLock 方案。路由解析、模型查询、提供者管理
//! 全部通过 SQLite 的 `providers` + `provider_models` 两张表完成。
//!
//! models.dev 数据仅作发现用途，通过 `catalog.rs` 缓存到磁盘，
//! 运行时路由不依赖 models.dev。

pub mod catalog;
pub mod compat;
pub mod error;
pub mod router;

pub use error::StoreError;
pub use router::{AvailableModel, ResolvedProviderRoute};

use std::path::PathBuf;
use std::sync::Arc;

use crate::db;
use crate::config::models::ProviderCompatibility;
use crate::models_dev::ModelsDevRoot;

use router::KeySelector;

/// 核心数据存储 — 封装 SQLite 数据库操作 + Key 选择器。
#[derive(Clone)]
pub struct Store {
    /// toasty 数据库句柄
    db: db::Db,
    /// 数据存储目录（用于 models.dev 缓存）
    path: PathBuf,
    /// 加权轮询 Key 选择器
    key_selector: Arc<KeySelector>,
}

/// models.dev 缓存元数据。
#[derive(Debug, Clone)]
pub struct StoreMetadata {
    pub fetched_at: i64,
    pub etag: Option<String>,
}

impl Store {
    /// 创建 Store 实例（不负责数据库初始化，db 由 main.rs 传入）。
    pub fn new(db: db::Db, path: impl Into<PathBuf>) -> Self {
        Self {
            db,
            path: path.into(),
            key_selector: Arc::new(KeySelector::new()),
        }
    }

    /// 从旧的 `Store::open` 风格创建（向后兼容迁移辅助）。
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, StoreError> {
        let path = std::path::PathBuf::from(path.as_ref());
        std::fs::create_dir_all(&path)?;

        // 返回一个占位 Store，db 需要后续设置
        // 实际使用中由 main.rs 调用 Store::new(db, path)
        Err(StoreError::Io(std::io::Error::other(
            "Store::open is deprecated; use Store::new(db, path) instead",
        )))
    }

    // ── Catalog (models.dev cache, for discovery only) ──

    /// 获取 models.dev 缓存中的提供者数量（用于判断缓存是否为空）。
    pub fn catalog_provider_count(path: &std::path::Path) -> usize {
        catalog::load_catalog_cache(path)
            .map(|(data, _)| data.len())
            .unwrap_or(0)
    }

    /// 替换 models.dev 磁盘缓存（由 GatewayManagerActor 刷新时调用）。
    pub fn replace_catalog_cache(
        &self,
        data: ModelsDevRoot,
        metadata: StoreMetadata,
    ) -> Result<(), StoreError> {
        catalog::save_catalog_cache(
            &self.path,
            &crate::models_dev::CatalogCache {
                fetched_at: metadata.fetched_at,
                etag: metadata.etag.clone(),
                data,
            },
        )
    }

    /// 获取 models.dev 缓存的元数据（用于 ETag 条件请求）。
    pub fn get_catalog_metadata(&self) -> StoreMetadata {
        catalog::load_catalog_cache(&self.path)
            .map(|(_, meta)| meta)
            .unwrap_or(StoreMetadata {
                fetched_at: 0,
                etag: None,
            })
    }

    /// 从 models.dev 缓存中获取所有提供者（用于 Admin UI 发现）。
    pub fn get_catalog_providers(&self) -> ModelsDevRoot {
        catalog::load_catalog_cache(&self.path)
            .map(|(data, _)| data)
            .unwrap_or_default()
    }

    // ── Model listing ──

    /// 列出所有模型（包括未启用的，供 Admin 使用）。
    pub async fn list_all_models(&self) -> Result<Vec<AvailableModel>, String> {
        router::list_all_models(&self.db).await
    }

    /// 列出已启用提供者的可用模型（供 /v1/models 使用）。
    pub async fn list_available_models(&self) -> Result<Vec<AvailableModel>, String> {
        router::list_available_models(&self.db).await
    }

    // ── Route resolution ──

    /// 解析模型名 → 路由列表（异步查询 SQLite）。
    pub async fn resolve_model(
        &self,
        model_name: &str,
    ) -> Result<Vec<ResolvedProviderRoute>, String> {
        router::resolve_model(&self.db, &self.key_selector, model_name).await
    }

    // ── Provider management ──

    /// 列出所有提供者（返回数据库行）。
    pub async fn list_providers(&self) -> Result<Vec<crate::db::models::Provider>, String> {
        crate::db::models::Provider::all()
            .exec(&mut self.db.clone())
            .await
            .map_err(|e| e.to_string())
    }

    /// 按 provider_id 查找提供者。
    pub async fn get_provider(
        &self,
        provider_id: &str,
    ) -> Result<Option<crate::db::models::Provider>, String> {
        let results = crate::db::models::Provider::filter(
            crate::db::models::Provider::fields().provider_id().eq(provider_id),
        )
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())?;

        Ok(results.into_iter().next())
    }

    /// 创建或更新提供者。
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_provider(
        &self,
        provider_id: String,
        display_name: String,
        npm: Option<String>,
        base_url: Option<String>,
        api_keys: String,
        compat_settings: Option<String>,
        enabled: bool,
        priority: i64,
    ) -> Result<crate::db::models::Provider, String> {
        let existing = self.get_provider(&provider_id).await?;

        if let Some(provider) = existing {
            let id = provider.id;
            crate::db::models::Provider::filter(
                crate::db::models::Provider::fields().id().eq(id),
            )
            .update()
            .display_name(display_name)
            .npm(npm)
            .base_url(base_url)
            .api_keys(api_keys)
            .compat_settings(compat_settings)
            .enabled(enabled)
            .priority(priority)
            .exec(&mut self.db.clone())
            .await
            .map_err(|e| e.to_string())?;

            crate::db::models::Provider::get_by_id(&mut self.db.clone(), &id)
                .await
                .map_err(|e| e.to_string())
        } else {
            let provider = toasty::create!(db::models::Provider {
                provider_id,
                display_name,
                npm,
                base_url,
                api_keys,
                compat_settings,
                enabled,
                priority,
            })
            .exec(&mut self.db.clone())
            .await
            .map_err(|e| e.to_string())?;

            Ok(provider)
        }
    }

    /// 删除提供者（级联删除其模型）。
    pub async fn delete_provider(&self, provider_id: &str) -> Result<bool, String> {
        let provider = match self.get_provider(provider_id).await? {
            Some(p) => p,
            None => return Ok(false),
        };

        let row_id = provider.id;

        // 删除关联的模型
        let models = crate::db::models::ProviderModel::filter(
            crate::db::models::ProviderModel::fields().provider_row_id().eq(row_id),
        )
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())?;

        for model in models {
            crate::db::models::ProviderModel::filter(
                crate::db::models::ProviderModel::fields().id().eq(model.id),
            )
            .delete()
            .exec(&mut self.db.clone())
            .await
            .map_err(|e| e.to_string())?;
        }

        crate::db::models::Provider::filter(
            crate::db::models::Provider::fields().id().eq(row_id),
        )
        .delete()
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())?;

        Ok(true)
    }

    // ── Provider Model management ──

    /// 列出提供者下的所有模型。
    pub async fn list_provider_models(
        &self,
        provider_row_id: u64,
    ) -> Result<Vec<crate::db::models::ProviderModel>, String> {
        crate::db::models::ProviderModel::filter(
            crate::db::models::ProviderModel::fields()
                .provider_row_id()
                .eq(provider_row_id),
        )
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())
    }

    /// 添加模型到提供者。
    #[allow(clippy::too_many_arguments)]
    pub async fn add_provider_model(
        &self,
        provider_row_id: u64,
        model_name: String,
        provider_model_id: String,
        compatibility: ProviderCompatibility,
        display_name: String,
        description: Option<String>,
        max_input_tokens: i64,
        max_output_tokens: i64,
        tool_calling: bool,
        vision: bool,
        thinking: bool,
        adaptive_thinking: bool,
        input_price_per_1m: Option<f64>,
        output_price_per_1m: Option<f64>,
        cache_read_price_per_1m: Option<f64>,
    ) -> Result<crate::db::models::ProviderModel, String> {
        let model = toasty::create!(db::models::ProviderModel {
            provider_row_id,
            model_name,
            provider_model_id,
            compatibility,
            display_name,
            description,
            max_input_tokens,
            max_output_tokens,
            tool_calling,
            vision,
            thinking,
            adaptive_thinking,
            input_price_per_1m,
            output_price_per_1m,
            cache_read_price_per_1m,
            enabled: true,
            status: None,
        })
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())?;

        Ok(model)
    }

    /// 删除提供者模型。
    pub async fn delete_provider_model(&self, model_id: u64) -> Result<bool, String> {
        crate::db::models::ProviderModel::get_by_id(&mut self.db.clone(), &model_id)
            .await
            .map_err(|e| e.to_string())?;

        crate::db::models::ProviderModel::filter(
            crate::db::models::ProviderModel::fields().id().eq(model_id),
        )
        .delete()
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())?;

        Ok(true)
    }

    /// 更新模型启用状态。
    pub async fn set_provider_model_enabled(
        &self,
        model_id: u64,
        enabled: bool,
    ) -> Result<(), String> {
        crate::db::models::ProviderModel::filter(
            crate::db::models::ProviderModel::fields().id().eq(model_id),
        )
        .update()
        .enabled(enabled)
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())?;

        Ok(())
    }
}

/// API Key 展示（隐藏敏感信息）。
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ApiKeyDisplay {
    pub label: String,
    pub weight: u32,
    pub masked_key: String,
}

/// 隐藏 API Key 中间部分。
pub fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        return "***".to_string();
    }
    format!("{}...{}", &key[..4], &key[key.len() - 4..])
}
