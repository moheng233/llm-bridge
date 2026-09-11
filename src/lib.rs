#[cfg(not(any(feature = "dev-ui", feature = "embed-frontend")))]
compile_error!("enable dev-ui for development or embed-frontend for a standalone server");

pub mod actors;
pub mod auth;
pub mod config;
pub mod db;
#[cfg(feature = "embed-frontend")]
pub mod embed;
pub mod http;
pub mod middleware;
pub mod observability;
pub mod quota;
pub mod server;
pub mod store;
pub mod types;
