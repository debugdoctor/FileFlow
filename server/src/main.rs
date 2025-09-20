use tracing::{event, Level};

mod db;
mod router;
mod service;
mod utils;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_thread_ids(false)
        .with_target(false)
        .with_line_number(false)
        .init();
    
    event!(Level::INFO, "server started");
    
    router::start_server("0.0.0.0", "5000").await;
}
