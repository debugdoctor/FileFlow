use std::env;
use tracing::{event, Level};
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};
use dotenvy::dotenv;

mod dao;
mod router;
mod service;
mod utils;

const DEFAULT_HOST: &str = "0.0.0.0";
const DEFAULT_PORT: &str = "5000";

#[tokio::main]
async fn main() {
    // Load environment variables from .env if present
    dotenv().ok();

    // 设置日志级别，默认为INFO，可通过环境变量RUST_LOG调整
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_thread_ids(false)
        .with_target(false)
        .with_line_number(false)
        .with_span_events(FmtSpan::NONE) // 减少span事件的日志输出
        .init();

    let host = env::var("FILEFLOW_HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string());
    let port = env::var("FILEFLOW_PORT").unwrap_or_else(|_| DEFAULT_PORT.to_string());

    event!(Level::INFO, "FileFlow server started");
    
    router::start_server(&host, &port).await;
}
