//! 数据存储层（Phase 3 重写 + Phase 4 重构 + 多协议架构）— 基于 toasty + SQLite。
//!
//! 路由解析、模型查询、提供者管理全部通过 SQLite 的
//! `models` + `model_providers` + `provider_protocols` + `providers` 四张表完成。

pub mod compat;
pub mod error;
pub mod router;

pub use error::StoreError;
pub use router::{AvailableModel, ModelProviderInfo, ResolvedProviderRoute};

use std::sync::Arc;

use crate::db;
use crate::config::models::ApiKeyEntry;

use router::KeySelector;

/// 核心数据存储 — 封装 SQLite 数据库操作 + Key 选择器。
#[derive(Clone)]
pub struct Store {
    /// toasty 数据库句柄
    db: db::Db,
    /// 加权轮询 Key 选择器
    key_selector: Arc<KeySelector>,
}

impl Store {
    /// 创建 Store 实例。
    pub fn new(db: db::Db) -> Self {
        Self {
            db,
            key_selector: Arc::new(KeySelector::new()),
        }
    }

    // ── Catalog (models.dev removed — providers created manually via Admin API) ──

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
        api_keys: Vec<ApiKeyEntry>,
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
            .api_keys(api_keys)
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
                api_keys,
                enabled,
                priority,
            })
            .exec(&mut self.db.clone())
            .await
            .map_err(|e| e.to_string())?;

            Ok(provider)
        }
    }

    /// 删除提供者（级联删除其 ModelProvider 和 ProviderProtocol 关联）。
    pub async fn delete_provider(&self, provider_id: &str) -> Result<bool, String> {
        let provider = match self.get_provider(provider_id).await? {
            Some(p) => p,
            None => return Ok(false),
        };

        let row_id = provider.id;

        // 删除关联的 ModelProvider
        let links = crate::db::models::ModelProvider::filter(
            crate::db::models::ModelProvider::fields().provider_id().eq(row_id),
        )
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())?;

        for link in links {
            crate::db::models::ModelProvider::filter(
                crate::db::models::ModelProvider::fields().id().eq(link.id),
            )
            .delete()
            .exec(&mut self.db.clone())
            .await
            .map_err(|e| e.to_string())?;
        }

        // 删除关联的 ProviderProtocol
        let protocols = crate::db::models::ProviderProtocol::filter(
            crate::db::models::ProviderProtocol::fields().provider_id().eq(row_id),
        )
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())?;

        for proto in protocols {
            crate::db::models::ProviderProtocol::filter(
                crate::db::models::ProviderProtocol::fields().id().eq(proto.id),
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

    /// 列出提供者下的所有 ModelProvider 关联。
    pub async fn list_provider_models(
        &self,
        provider_id: u64,
    ) -> Result<Vec<crate::db::models::ModelProvider>, String> {
        crate::db::models::ModelProvider::filter(
            crate::db::models::ModelProvider::fields()
                .provider_id()
                .eq(provider_id),
        )
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())
    }

    /// 添加模型到提供者。
    ///
    /// 如果 Model 不存在则自动创建（使用传入的能力作为标称值），
    /// 然后创建 ModelProvider 关联（定价为提供者特定）。
    #[allow(clippy::too_many_arguments)]
    pub async fn add_provider_model(
        &self,
        provider_id: u64,
        model_name: String,
        provider_model_id: String,
        protocol_id: u64,
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
    ) -> Result<crate::db::models::ModelProvider, String> {
        // 查找或创建 Model
        let model_id = self.ensure_model(
            &model_name,
            &display_name,
            description.clone(),
            max_input_tokens,
            max_output_tokens,
            tool_calling,
            vision,
            thinking,
            adaptive_thinking,
        ).await?;

        let mp = toasty::create!(db::models::ModelProvider {
            model_id,
            provider_id,
            provider_model_id,
            protocol_id,
            display_name,
            max_input_tokens: None,  // 标称值已在 Model 中
            max_output_tokens: None,
            tool_calling: None,
            vision: None,
            thinking: None,
            adaptive_thinking: None,
            input_price_per_1m,
            output_price_per_1m,
            cache_read_price_per_1m,
            enabled: true,
            priority: 100,
        })
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())?;

        Ok(mp)
    }

    /// 确保 Model 存在，不存在则创建。返回 model id。
    #[allow(clippy::too_many_arguments)]
    async fn ensure_model(
        &self,
        model_name: &str,
        display_name: &str,
        description: Option<String>,
        max_input_tokens: i64,
        max_output_tokens: i64,
        tool_calling: bool,
        vision: bool,
        thinking: bool,
        adaptive_thinking: bool,
    ) -> Result<u64, String> {
        // 先查找是否存在
        let existing = crate::db::models::LLMModel::filter(
            crate::db::models::LLMModel::fields().model_name().eq(model_name),
        )
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())?;

        if let Some(m) = existing.into_iter().next() {
            return Ok(m.id);
        }

        // 创建新 Model
        let model = toasty::create!(db::models::LLMModel {
            model_name: model_name.to_string(),
            display_name: display_name.to_string(),
            description,
            max_input_tokens,
            max_output_tokens,
            tool_calling,
            vision,
            thinking,
            adaptive_thinking,
            status: None,
        })
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())?;

        Ok(model.id)
    }

    /// 删除 ModelProvider 关联。
    pub async fn delete_provider_model(&self, model_id: u64) -> Result<bool, String> {
        crate::db::models::ModelProvider::get_by_id(&mut self.db.clone(), &model_id)
            .await
            .map_err(|e| e.to_string())?;

        crate::db::models::ModelProvider::filter(
            crate::db::models::ModelProvider::fields().id().eq(model_id),
        )
        .delete()
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())?;

        Ok(true)
    }

    /// 更新 ModelProvider 启用状态。
    pub async fn set_provider_model_enabled(
        &self,
        model_id: u64,
        enabled: bool,
    ) -> Result<(), String> {
        crate::db::models::ModelProvider::filter(
            crate::db::models::ModelProvider::fields().id().eq(model_id),
        )
        .update()
        .enabled(enabled)
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())?;

        Ok(())
    }

    // ── Provider (by row id) ──

    /// 按内部 row id 查找提供者。
    pub async fn get_provider_by_id(
        &self,
        id: u64,
    ) -> Result<Option<crate::db::models::Provider>, String> {
        crate::db::models::Provider::get_by_id(&mut self.db.clone(), &id)
            .await
            .map(Some)
            .or_else(|e| {
                if e.to_string().contains("not found") {
                    Ok(None)
                } else {
                    Err(e.to_string())
                }
            })
    }

    /// 更新提供者（按 row id）。
    #[allow(clippy::too_many_arguments)]
    pub async fn update_provider_by_id(
        &self,
        id: u64,
        display_name: String,
        api_keys: Vec<ApiKeyEntry>,
        enabled: bool,
        priority: i64,
    ) -> Result<crate::db::models::Provider, String> {
        crate::db::models::Provider::filter(
            crate::db::models::Provider::fields().id().eq(id),
        )
        .update()
        .display_name(display_name)
        .api_keys(api_keys)
        .enabled(enabled)
        .priority(priority)
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())?;

        crate::db::models::Provider::get_by_id(&mut self.db.clone(), &id)
            .await
            .map_err(|e| e.to_string())
    }

    /// 删除提供者（按 row id，级联删除其 ModelProvider 和 ProviderProtocol 关联）。
    pub async fn delete_provider_by_id(&self, id: u64) -> Result<bool, String> {
        let provider = match self.get_provider_by_id(id).await? {
            Some(p) => p,
            None => return Ok(false),
        };

        // 删除关联的 ModelProvider
        let links = crate::db::models::ModelProvider::filter(
            crate::db::models::ModelProvider::fields()
                .provider_id()
                .eq(provider.id),
        )
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())?;

        for link in links {
            crate::db::models::ModelProvider::filter(
                crate::db::models::ModelProvider::fields().id().eq(link.id),
            )
            .delete()
            .exec(&mut self.db.clone())
            .await
            .map_err(|e| e.to_string())?;
        }

        // 删除关联的 ProviderProtocol
        let protocols = crate::db::models::ProviderProtocol::filter(
            crate::db::models::ProviderProtocol::fields()
                .provider_id()
                .eq(provider.id),
        )
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())?;

        for proto in protocols {
            crate::db::models::ProviderProtocol::filter(
                crate::db::models::ProviderProtocol::fields().id().eq(proto.id),
            )
            .delete()
            .exec(&mut self.db.clone())
            .await
            .map_err(|e| e.to_string())?;
        }

        crate::db::models::Provider::filter(
            crate::db::models::Provider::fields().id().eq(provider.id),
        )
        .delete()
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())?;

        Ok(true)
    }

    // ── Provider Model full update ──

    /// 完整更新 ModelProvider 关联的所有字段。
    #[allow(clippy::too_many_arguments)]
    pub async fn update_provider_model(
        &self,
        model_id: u64,
        provider_model_id: String,
        protocol_id: u64,
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
        enabled: bool,
    ) -> Result<crate::db::models::ModelProvider, String> {
        let _ = description; // 描述属于 Model，不在此更新
        crate::db::models::ModelProvider::filter(
            crate::db::models::ModelProvider::fields().id().eq(model_id),
        )
        .update()
        .provider_model_id(provider_model_id)
        .protocol_id(protocol_id)
        .display_name(display_name)
        .max_input_tokens(Some(max_input_tokens))
        .max_output_tokens(Some(max_output_tokens))
        .tool_calling(Some(tool_calling))
        .vision(Some(vision))
        .thinking(Some(thinking))
        .adaptive_thinking(Some(adaptive_thinking))
        .input_price_per_1m(input_price_per_1m)
        .output_price_per_1m(output_price_per_1m)
        .cache_read_price_per_1m(cache_read_price_per_1m)
        .enabled(enabled)
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())?;

        crate::db::models::ModelProvider::get_by_id(&mut self.db.clone(), &model_id)
            .await
            .map_err(|e| e.to_string())
    }

    // ── User management ──

    /// 列出所有用户。
    pub async fn list_users(
        &self,
    ) -> Result<Vec<crate::db::models::User>, String> {
        crate::db::models::User::all()
            .exec(&mut self.db.clone())
            .await
            .map_err(|e| e.to_string())
    }

    /// 更新用户角色。
    pub async fn update_user_role(
        &self,
        user_id: u64,
        role: crate::db::models::UserRole,
    ) -> Result<(), String> {
        crate::db::models::User::filter(
            crate::db::models::User::fields().id().eq(user_id),
        )
        .update()
        .role(role)
        .exec(&mut self.db.clone())
        .await
        .map_err(|e| e.to_string())?;

        Ok(())
    }

    // ── Admin helpers ──
}

/// API Key 展示（隐藏敏感信息）。
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
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
