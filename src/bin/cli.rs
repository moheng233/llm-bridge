use toasty_cli::{Config, ToastyCli};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load()?;

    let db = toasty::Db::builder()
        .models(toasty::models!(crate::*))
        .connect("sqlite:./my_app.db")
        .await?;

    let cli = ToastyCli::with_config(db, config);
    cli.parse_and_run().await?;

    Ok(())
}
