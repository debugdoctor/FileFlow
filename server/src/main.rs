use tracing::event;
use tracing_subscriber;
use anyhow::Result;

mod service;
mod utils;
mod router;
mod db;

#[tokio::main]
async fn main() -> Result<()> {
	tracing_subscriber::fmt()
        .with_thread_ids(false)
        .with_target(false)
        .with_line_number(false)
        .init();

    event!(tracing::Level::INFO, "server started");
    router::start_server("0.0.0.0", "5000").await;
    
    Ok(())
}
