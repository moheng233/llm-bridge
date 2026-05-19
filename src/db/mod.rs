//! 数据库模块 — toasty 连接管理与 schema 初始化。
//!
//! 支持 SQLite（默认）和 PostgreSQL（可选 feature `postgresql`）。
//! 在 Phase 1 早期开发阶段使用 `push_schema` 自动建表，
//! 后续将迁移到正式的 migration 系统。

pub mod models;

use std::path::Path;
use tracing::info;

/// 通过连接 URL 初始化数据库。
///
/// 接受 toasty 支持的任何连接 URL 格式：
/// - SQLite: `sqlite:/path/to/db.db` 或 `sqlite::memory:`
/// - PostgreSQL（需启用 `postgresql` feature）: `postgresql://user:pass@host/db`
///
/// 连接后自动执行 `push_schema` 创建/更新表结构。
///
/// # 示例
///
/// ```ignore
/// use llm_bridge::db;
///
/// // SQLite 文件数据库
/// let db = db::init(models, "sqlite:./data/app.db").await?;
///
/// // 启用 postgresql feature 后：
/// let db = db::init(models, "postgresql://user:pass@localhost/llm_bridge").await?;
/// ```
pub async fn init(models: toasty::ModelSet, url: &str) -> toasty::Result<toasty::Db> {
    info!(%url, "connecting to database");

    let db = toasty::Db::builder()
        .models(models)
        .connect(url)
        .await?;

    info!("applying schema...");
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
) -> toasty::Result<toasty::Db> {
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
