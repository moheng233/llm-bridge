//! llm-bridge 数据库管理 CLI（基于 toasty-cli）。
//!
//! 与 `src/main.rs` 保持同步：模型集合用 [`db::all_models`]，
//! 连接 URL 用 [`RuntimeSettings`] 的 `store_path`（env: `LLM_BRIDGE_STORE_PATH`）。

use llm_bridge::config::models::RuntimeSettings;
use llm_bridge::db;
use toasty_cli::{Config, ToastyCli};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load()?;

    let settings = RuntimeSettings::from_env().map_err(anyhow::Error::msg)?;

    let db = toasty::Db::builder()
        .models(db::all_models())
        .connect(&format!("sqlite:{}/sqlite.db", settings.store_path))
        .await?;

    let cli = ToastyCli::with_config(db, config);
    cli.parse_and_run().await?;

    Ok(())
}
