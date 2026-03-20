pub mod actors;
pub mod config;
pub mod observability;
pub mod protocol;
pub mod routing;
pub mod server;
pub mod types;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    tracing::info!("Starting llm-bridge gateway");
}
