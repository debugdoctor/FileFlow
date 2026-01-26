use axum::{routing::{get, post, put}, serve, Router};
use tokio::net::{TcpListener};
use tower_http::timeout::TimeoutLayer;
use tracing::{event, instrument, Level};
use std::time::Duration;

use crate::service::handler::{download, get_assets, get_file, get_id, get_status, upload, upload_file, done, home};
use tower_http::services::ServeDir;

fn api_router() -> Router {
    Router::new()
        .route("/hello", get(|| async {
            // Changed from DEBUG to TRACE to reduce log verbosity
            event!(Level::TRACE, "Hello endpoint accessed");
            "Hi!"
        }))
        .route("/id", get(get_id))
        .route("/{id}/status", get(get_status))
        // Add timeout layer specifically for upload api
        .route("/{id}/upload", post(upload_file))
        .layer(TimeoutLayer::new(Duration::from_secs(20)))
        // Add timeout layer specifically for download api
        .route("/{id}/file", get(get_file))
        .layer(TimeoutLayer::new(Duration::from_secs(20)))
        .route("/{id}/done", put(done))
        .layer(TimeoutLayer::new(Duration::from_secs(20)))
}

fn assets_router() -> Router {
    Router::new()
        .route("/{path}", get(get_assets))
}

fn view_router() -> Router {
    Router::new()
        .route("/", get(home))
        .route("/upload", get(upload))
        .route("/download", get(download))
        .route("/{id}/file", get(download))
}

#[instrument(skip_all)]
pub async fn start_server(ip: &str, port: &str) {
    // Changed from INFO to DEBUG to reduce log verbosity
    event!(Level::INFO, "Initializing server with ip: {} and port: {}", ip, port);
    
    let app = Router::new()
        .merge(view_router())
        .nest("/api/fileflow", api_router())
        .nest("/assets", assets_router())
        // 添加静态文件服务，用于提供web/dist目录中的文件
        .fallback_service(ServeDir::new("../web/dist"));

    let addr = format!("{}:{}", ip, port);
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            event!(Level::ERROR, "Failed to bind to address {}: {}", addr, e);
            return;
        }
    };

    // Changed from INFO to DEBUG to reduce log verbosity
    event!(Level::DEBUG, "Server listening on {}", addr);
    match serve(listener, app).await {
        Ok(_) => {
            // Changed from INFO to DEBUG to reduce log verbosity
            event!(Level::DEBUG, "Server stopped");
        }
        Err(e) => {
            event!(Level::ERROR, "Server error: {}", e);
        }
    }
}
