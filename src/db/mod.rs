//! 数据库模块 — toasty 连接管理与 schema 初始化。
//!
//! 支持 SQLite（默认）和 PostgreSQL（可选 feature `postgresql`）。
//! 在 Phase 1 早期开发阶段使用 `push_schema` 自动建表，
//! 后续将迁移到正式的 migration 系统。

pub mod models;

use std::path::Path;
use tracing::info;

/// toasty 数据库句柄类型别名。
pub type Db = toasty::Db;

/// 返回包含所有 5 张核心表的 [`toasty::ModelSet`]。
///
/// 注册：`User`, `Token`, `UsageRecord`, `Provider`, `ProviderModel`。
/// 配合 [`init`] / [`init_sqlite`] 使用：
///
/// ```ignore
/// let db = db::init(db::all_models(), "sqlite::memory:").await?;
/// ```
pub fn all_models() -> toasty::ModelSet {
    toasty::models!(models::User, models::Token, models::UsageRecord, models::Provider, models::ProviderModel)
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

    let db = toasty::Db::builder()
        .models(models)
        .connect(url)
        .await?;

    info!("applying schema...");
    // push_schema 对已存在的表只会打印警告，不会返回错误。
    // 若确实发生致命错误（如类型不兼容），会返回 Err。
    db.push_schema().await?;
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
pub async fn init_sqlite(
    models: toasty::ModelSet,
    store_path: &Path,
) -> toasty::Result<Db> {
    tokio::fs::create_dir_all(store_path)
        .await
        .map_err(|e| {
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
        let db = init(all_models(), "sqlite::memory:")
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
        .exec(&db)
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
}
