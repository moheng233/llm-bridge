//! 数据库模块 — toasty 连接管理与 schema 初始化。
//!
//! 支持 SQLite（默认）和 PostgreSQL（可选 feature `postgresql`）。
//! 新表、索引及周期记录去重在同一事务内应用；不兼容旧字段明确失败，绝不删库。

pub mod models;

use std::path::Path;
use tracing::{info, warn};

/// toasty 数据库句柄类型别名。
pub type Db = toasty::Db;

/// 返回包含所有 10 张核心表的 [`toasty::ModelSet`]。
///
/// 注册：`User`, `Token`, `UsageRecord`, `LLMModel`, `Provider`, `ModelProvider`,
/// `ProviderProtocol`, `LlmRequestTrace`, `UsageDaily`, `CliSession`。
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
        models::UsageDaily,
        models::CliSession
    )
}

/// 通过连接 URL 初始化数据库并应用 schema。
///
/// 接受 toasty 支持的任何连接 URL 格式：
/// - SQLite: `sqlite:/path/to/db.db` 或 `sqlite::memory:`
/// - PostgreSQL（需启用 `postgresql` feature）: `postgresql://user:pass@host/db`
///
/// Schema 策略（BUG-015，保留数据的升级路径）：
///
/// 新库及字段兼容的旧库：事务内创建缺失表、检查必需列、合并重复周期记录，
/// 然后建立缺失索引。未知旧字段结构需先备份并显式迁移；启动失败不改动原数据。
///
/// # 示例
///
/// ```ignore
/// use llm_bridge::db;
///
/// // 内存数据库（测试用）
/// let db = db::init(db::all_models(), "sqlite::memory:").await?;
///
/// // PostgreSQL（需 --features postgresql）
/// let db = db::init(db::all_models(), "postgresql://user:pass@localhost/llm_bridge").await?;
/// ```
pub async fn init(models: toasty::ModelSet, url: &str) -> toasty::Result<Db> {
    info!("connecting to database");

    let mut builder = toasty::Db::builder();
    builder.models(models);
    if url.starts_with("sqlite:") {
        // rusqlite 执行同步 SQL；连接池排队避免同池多个写者阻塞运行时。
        builder.max_pool_size(1);
    }
    let db = builder.connect(url).await?;

    info!("applying schema...");
    apply_schema(&db).await?;
    info!("database initialized successfully");

    Ok(db)
}

/// 原子应用可加性 schema；不将某张表已存在误当作整个升级完成。
async fn apply_schema(db: &Db) -> toasty::Result<()> {
    use toasty::{
        schema::{db::Schema, diff},
        sql,
    };
    let empty = Schema::default();
    let hints = diff::RenameHints::new();
    let migration =
        db.driver()
            .generate_migration(&diff::Schema::from(&empty, &db.schema().db, &hints));
    let mut connection = db.clone();
    let mut tx = connection.transaction().await?;
    // 写入首条语句取得 SQLite 写锁；PostgreSQL 的固定行锁也串行化重复启动。
    sql::statement("CREATE TABLE IF NOT EXISTS llm_bridge_schema_lock (id INTEGER PRIMARY KEY)")
        .exec(&mut tx)
        .await?;
    sql::statement(
        "INSERT INTO llm_bridge_schema_lock (id) VALUES (1) ON CONFLICT (id) DO UPDATE SET id = 1",
    )
    .exec(&mut tx)
    .await?;
    let mut indexes = Vec::new();
    for statement in migration
        .statements()
        .iter()
        .flat_map(|script| generated_statements(script))
    {
        let statement = statement.trim();
        if let Some(rest) = statement.strip_prefix("CREATE TABLE ") {
            sql::statement(format!("CREATE TABLE IF NOT EXISTS {rest}"))
                .exec(&mut tx)
                .await?;
        } else if let Some(rest) = statement.strip_prefix("CREATE UNIQUE INDEX ") {
            indexes.push(format!("CREATE UNIQUE INDEX IF NOT EXISTS {rest}"));
        } else if let Some(rest) = statement.strip_prefix("CREATE INDEX ") {
            indexes.push(format!("CREATE INDEX IF NOT EXISTS {rest}"));
        } else if let Some(rest) = statement.strip_prefix("CREATE TYPE ") {
            let (name, values) = rest.split_once(" AS ENUM ").ok_or_else(|| {
                toasty::Error::from_args(format_args!("unsupported generated type: {statement}"))
            })?;
            let values = values
                .strip_prefix('(')
                .and_then(|value| value.strip_suffix(')'))
                .ok_or_else(|| {
                    toasty::Error::from_args(format_args!("invalid generated enum: {statement}"))
                })?;
            let name_literal = name.replace('\'', "''");
            // 固定 schema 锁串行化启动；已有枚举必须完全匹配，不能静默忽略新增/缺失值。
            sql::statement(format!(
                "DO $llm_bridge$ BEGIN IF to_regtype('{name_literal}') IS NULL THEN {statement}; \
                 ELSIF (SELECT array_agg(enumlabel::text ORDER BY enumsortorder) FROM pg_enum WHERE enumtypid = to_regtype('{name_literal}')) \
                 IS DISTINCT FROM ARRAY[{values}]::text[] THEN RAISE EXCEPTION 'incompatible enum type: %', '{name_literal}'; END IF; END $llm_bridge$"
            )).exec(&mut tx).await?;
        } else if !statement.is_empty() {
            return Err(toasty::Error::from_args(format_args!(
                "unsupported schema operation; back up the database and apply an explicit migration: {statement}"
            )));
        }
    }
    for table in &db.schema().db.tables {
        // 以限定列读取验证字段存在，SQLite 双引号字符串兼容不会掩盖缺列。
        let columns = table
            .columns
            .iter()
            .map(|column| format!("t.\"{}\"", column.name.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(", ");
        sql::query(format!("SELECT {columns} FROM \"{}\" AS t LIMIT 0", table.name.replace('"', "\"\"")))
            .exec(&mut tx).await.map_err(|error| toasty::Error::from_args(format_args!(
                "incompatible schema for table '{}'; back up the database and migrate its columns explicitly; no data was changed: {error}", table.name
            )))?;
    }
    if db
        .schema()
        .db
        .tables
        .iter()
        .any(|table| table.name == "usage_records")
    {
        merge_duplicate_usage_records(&mut tx).await?;
    }
    for index in indexes {
        sql::statement(index).exec(&mut tx).await?;
    }
    tx.commit().await
}

/// 驱动可能返回一整段 CREATE 脚本；仅按引号外分号切分，保留标识符/枚举值中的分号。
/// 输入是驱动生成的 DDL，不是任意用户 SQL（后续仍只接受明确支持的 CREATE 操作）。
fn generated_statements(script: &str) -> Vec<&str> {
    let mut statements = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut chars = script.char_indices().peekable();
    while let Some((offset, character)) = chars.next() {
        if let Some(delimiter) = quote {
            if character == delimiter {
                if chars.peek().is_some_and(|(_, next)| *next == delimiter) {
                    chars.next();
                } else {
                    quote = None;
                }
            }
        } else if matches!(character, '\'' | '"') {
            quote = Some(character);
        } else if character == ';' {
            statements.push(&script[start..offset]);
            start = offset + 1;
        }
    }
    if start < script.len() {
        statements.push(&script[start..]);
    }
    statements
}

/// 每组保留最小 ID 并求和；与删除重复行、建立唯一索引共享一个事务。
async fn merge_duplicate_usage_records(tx: &mut toasty::Transaction<'_>) -> toasty::Result<()> {
    use toasty::sql;
    let merged = sql::statement(
        "UPDATE usage_records AS u
         SET request_count = (SELECT SUM(d.request_count) FROM usage_records d WHERE d.token_id = u.token_id AND d.period_key = u.period_key),
             token_count = (SELECT SUM(d.token_count) FROM usage_records d WHERE d.token_id = u.token_id AND d.period_key = u.period_key)
         WHERE u.id IN (SELECT MIN(id) FROM usage_records GROUP BY token_id, period_key HAVING COUNT(*) > 1)"
    ).exec(tx).await?;
    let deleted = sql::statement(
        "DELETE FROM usage_records WHERE id NOT IN (SELECT MIN(id) FROM usage_records GROUP BY token_id, period_key)"
    ).exec(tx).await?;
    if deleted > 0 {
        warn!(
            merged,
            deleted, "merged duplicate usage periods without discarding counters"
        );
    }
    Ok(())
}

/// 初始化文件型 SQLite 数据库（便捷函数）。
///
/// 在给定目录下自动创建 `sqlite.db`（保留主程序历史文件名），
/// 父目录不存在时会递归创建。
///
/// 等价于 `init(models, "sqlite:<store_path>/sqlite.db")`，
/// 额外处理了目录创建。
pub async fn init_sqlite(models: toasty::ModelSet, store_path: &Path) -> toasty::Result<Db> {
    tokio::fs::create_dir_all(store_path).await.map_err(|e| {
        toasty::Error::from_args(format_args!(
            "failed to create store directory '{}': {e}",
            store_path.display()
        ))
    })?;

    let db_path = store_path.join("sqlite.db");
    let url = format!("sqlite:{}", db_path.display());

    init(models, &url).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn upgrade_adds_session_table_and_merges_usage_without_loss() {
        let directory = tempfile::tempdir().unwrap();
        let url = format!("sqlite:{}", directory.path().join("legacy.db").display());
        let mut db = init(all_models(), &url).await.unwrap();
        toasty::sql::statement("DROP TABLE cli_sessions")
            .exec(&mut db)
            .await
            .unwrap();
        let indexes = db
            .schema()
            .db
            .tables
            .iter()
            .find(|table| table.name == "usage_records")
            .unwrap()
            .indices
            .iter()
            .filter(|index| index.unique && !index.primary_key)
            .map(|index| index.name.clone())
            .collect::<Vec<_>>();
        for name in indexes {
            toasty::sql::statement(format!("DROP INDEX \"{name}\""))
                .exec(&mut db)
                .await
                .unwrap();
        }
        for (requests, tokens) in [(2, 40), (3, 60)] {
            toasty::create!(models::UsageRecord {
                token_id: 1,
                period_key: "2026-09",
                request_count: requests,
                token_count: tokens
            })
            .exec(&mut db)
            .await
            .unwrap();
        }
        let mut upgraded = init(all_models(), &url).await.unwrap();
        let rows = models::UsageRecord::all()
            .exec(&mut upgraded)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].request_count, rows[0].token_count), (5, 100));
        models::CliSession::all().exec(&mut upgraded).await.unwrap();
        let duplicate = toasty::create!(models::UsageRecord {
            token_id: 1,
            period_key: "2026-09",
            request_count: 1,
            token_count: 1
        })
        .exec(&mut upgraded)
        .await;
        assert!(
            duplicate.is_err(),
            "upgraded database must enforce period uniqueness"
        );
        apply_schema(&upgraded).await.unwrap();
        let rows = models::UsageRecord::all()
            .exec(&mut upgraded)
            .await
            .unwrap();
        assert_eq!((rows[0].request_count, rows[0].token_count), (5, 100));
    }

    #[tokio::test]
    async fn incompatible_columns_roll_back_upgrade_and_preserve_existing_rows() {
        let directory = tempfile::tempdir().unwrap();
        let url = format!("sqlite:{}", directory.path().join("legacy.db").display());
        let mut db = init(all_models(), &url).await.unwrap();
        toasty::create!(models::User {
            oidc_sub: "preserved",
            name: "Preserved",
            role: models::UserRole::Member,
            active: true
        })
        .exec(&mut db)
        .await
        .unwrap();
        toasty::sql::statement("DROP TABLE cli_sessions")
            .exec(&mut db)
            .await
            .unwrap();
        toasty::sql::statement("ALTER TABLE models RENAME COLUMN display_name TO legacy_name")
            .exec(&mut db)
            .await
            .unwrap();
        let error = init(all_models(), &url).await.unwrap_err().to_string();
        assert!(
            error.contains("incompatible schema for table 'models'"),
            "{error}"
        );
        assert_eq!(
            models::User::all().exec(&mut db).await.unwrap()[0].oidc_sub,
            "preserved"
        );
        assert!(
            models::CliSession::all().exec(&mut db).await.is_err(),
            "new tables must roll back with the failed upgrade"
        );
    }

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
        use crate::types::{
            LanguageModelChatMessage, LanguageModelInputPart, LanguageModelTextPart,
        };

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
        let msgs = fetched
            .request_messages
            .as_ref()
            .expect("messages should exist");
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
