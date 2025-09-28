use tracing::{event, Level};
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};

mod dao;
mod router;
mod service;
mod utils;

#[tokio::main]
async fn main() {
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

    event!(Level::INFO, "FileFlow server started");
    
    router::start_server("0.0.0.0", "5000").await;
}