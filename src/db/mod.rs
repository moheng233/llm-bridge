//! 数据库模块 — toasty 连接管理与 schema 初始化。
//!
//! 支持 SQLite（默认）和 PostgreSQL（可选 feature `postgresql`）。
//! 在 Phase 1 早期开发阶段使用 `push_schema` 自动建表，
//! 后续将迁移到正式的 migration 系统。

pub mod models;

use std::path::Path;
use tracing::{info, warn};

/// toasty 数据库句柄类型别名。
pub type Db = toasty::Db;

/// 返回包含所有 9 张核心表的 [`toasty::ModelSet`]。
///
/// 注册：`User`, `Token`, `UsageRecord`, `LLMModel`, `Provider`, `ModelProvider`,
/// `ProviderProtocol`, `LlmRequestTrace`, `UsageDaily`。
/// 配合 [`init`] / [`init_sqlite`] 使用：
///
/// ```ignore
/// let db = db::init(db::all_models(), "sqlite::memory:").await?;
/// ```
pub fn all_models() -> toasty::ModelSet {
    toasty::models!(
        models::User,
        models::Token,
        models::UsageRecord,
        models::LLMModel,
        models::Provider,
        models::ModelProvider,
        models::ProviderProtocol,
        models::LlmRequestTrace,
        models::UsageDaily
    )
}

/// 通过连接 URL 初始化数据库并自动建表。
///
/// 接受 toasty 支持的任何连接 URL 格式：
/// - SQLite: `sqlite:/path/to/db.db` 或 `sqlite::memory:`
/// - PostgreSQL（需启用 `postgresql` feature）: `postgresql://user:pass@host/db`
///
/// 连接后调用 [`toasty::Db::push_schema`] 创建表结构。
/// 如果表已存在则跳过（`push_schema` 对已存在的表仅输出警告，不致命）。
///
/// # 示例
///
/// ```ignore
/// use llm_bridge::db;
///
/// // 内存数据库（测试用）
/// let db = db::init(db::all_models(), "sqlite::memory:").await?;
///
/// // SQLite 文件数据库
/// let db = db::init(db::all_models(), "sqlite:./data/app.db").await?;
///
/// // PostgreSQL（需 --features postgresql）
/// let db = db::init(db::all_models(), "postgresql://user:pass@localhost/llm_bridge").await?;
/// ```
pub async fn init(models: toasty::ModelSet, url: &str) -> toasty::Result<Db> {
    info!(%url, "connecting to database");

    let db = toasty::Db::builder().models(models).connect(url).await?;

    info!("applying schema...");
    // toasty 的 push_schema 使用 CREATE TABLE（非 IF NOT EXISTS），
    // 在已存在的数据库上重复调用会报错。
    // 此处捕获 "already exists" 错误，作为非致命警告处理。
    if let Err(e) = db.push_schema().await {
        let err_msg = e.to_string();
        if err_msg.contains("already exists") {
            warn!("schema push skipped — tables already exist ({err_msg})");
        } else {
            return Err(e);
        }
    }
    info!("database initialized successfully");

    Ok(db)
}

/// 初始化文件型 SQLite 数据库（便捷函数）。
///
/// 在给定目录下自动创建 `llm-bridge.db`，
/// 父目录不存在时会递归创建。
///
/// 等价于 `init(models, "sqlite:<store_path>/llm-bridge.db")`，
/// 额外处理了目录创建。
pub async fn init_sqlite(models: toasty::ModelSet, store_path: &Path) -> toasty::Result<Db> {
    tokio::fs::create_dir_all(store_path).await.map_err(|e| {
        toasty::Error::from_args(format_args!(
            "failed to create store directory '{}': {e}",
            store_path.display()
        ))
    })?;

    let db_path = store_path.join("llm-bridge.db");
    let url = format!("sqlite:{}", db_path.display());

    init(models, &url).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn push_schema_creates_tables() {
        let mut db = init(all_models(), "sqlite::memory:")
            .await
            .expect("database initialization should succeed");

        // 验证 db 句柄可用（push_schema 已在 init 中调用）
        // 尝试插入数据来间接验证表存在
        let _user = toasty::create!(models::User {
            oidc_sub: "sub_test",
            name: "Test",
            role: models::UserRole::Member,
            active: true,
        })
        .exec(&mut db)
        .await
        .expect("insert after push_schema should succeed");
    }

    #[tokio::test]
    async fn insert_and_query_user() {
        let mut db = init(all_models(), "sqlite::memory:")
            .await
            .expect("database initialization should succeed");

        // 插入一个用户
        let user = toasty::create!(models::User {
            oidc_sub: "sub_123",
            name: "Alice",
            email: "alice@example.com",
            role: models::UserRole::Admin,
            active: true,
        })
        .exec(&mut db)
        .await
        .expect("insert should succeed");

        assert_eq!(user.oidc_sub, "sub_123");
        assert_eq!(user.name, "Alice");

        // 按 id 查询
        let fetched = models::User::get_by_id(&mut db, &user.id)
            .await
            .expect("get_by_id should succeed");

        assert_eq!(fetched.email.as_deref(), Some("alice@example.com"));
    }

    #[tokio::test]
    async fn insert_and_query_llm_request_trace() {
        use crate::db::models::{LlmRequestTrace, TraceInterface, TraceStatus};
        use crate::types::{LanguageModelChatMessage, LanguageModelInputPart, LanguageModelTextPart};

        let mut db = init(all_models(), "sqlite::memory:")
            .await
            .expect("database initialization should succeed");

        // 插入一条 pending 状态的 trace（带内容快照）
        let messages = vec![LanguageModelChatMessage::user(
            vec![LanguageModelInputPart::Text(LanguageModelTextPart {
                value: "hello".to_string(),
            })],
            None,
        )];

        let trace = toasty::create!(LlmRequestTrace {
            request_id: "req-001".to_string(),
            interface: TraceInterface::OpenAiHttp,
            token_id: 1,
            user_id: 1,
            token_prefix: "lb_ab3x".to_string(),
            model: "openai/gpt-4o".to_string(),
            provider_id: "openai".to_string(),
            provider_model_id: "gpt-4o".to_string(),
            protocol: "openai".to_string(),
            status: TraceStatus::Pending,
            estimated_tokens: 100,
            request_messages: Some(toasty::Json(messages)),
        })
        .exec(&mut db)
        .await
        .expect("insert trace should succeed");

        assert_eq!(trace.status, TraceStatus::Pending);
        assert!(!trace.status.is_final());
        assert!(trace.request_messages.is_some());

        // 按 request_id 唯一索引查询
        let fetched = LlmRequestTrace::get_by_request_id(&mut db, &"req-001".to_string())
            .await
            .expect("get_by_request_id should succeed");

        assert_eq!(fetched.interface, TraceInterface::OpenAiHttp);
        assert_eq!(fetched.token_prefix, "lb_ab3x");
        let msgs = fetched.request_messages.as_ref().expect("messages should exist");
        assert_eq!(msgs.len(), 1);
    }

    #[tokio::test]
    async fn insert_and_query_usage_daily() {
        use crate::db::models::UsageDaily;

        let mut db = init(all_models(), "sqlite::memory:")
            .await
            .expect("database initialization should succeed");

        let daily = toasty::create!(UsageDaily {
            day: "2026-07-22".to_string(),
            token_id: 1,
            model: "openai/gpt-4o".to_string(),
            request_count: 5,
            input_tokens: 1200,
            output_tokens: 3400,
            reasoning_tokens: 0,
            cached_tokens: 100,
            total_tokens: 4600,
            cost_usd: 0.0123,
        })
        .exec(&mut db)
        .await
        .expect("insert usage_daily should succeed");

        assert_eq!(daily.request_count, 5);
        assert_eq!(daily.total_tokens, 4600);

        // 按 day 索引查询（filter_by_<field>(value) 返回 Query builder，再 .exec(db)）
        let rows: Vec<_> = UsageDaily::filter_by_day("2026-07-22")
            .exec(&mut db)
            .await
            .expect("filter_by_day should succeed");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].input_tokens, 1200);
    }
}
