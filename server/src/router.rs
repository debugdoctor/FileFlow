use axum::{routing::{get, post}, serve, Router};
use tokio::net::{TcpListener};
use tower_http::timeout::TimeoutLayer;
use tracing::{event, instrument};
use std::time::Duration;

use crate::service::handler::{download, get_assets, get_file, get_id, get_status, upload, upload_file};
use tower_http::services::ServeDir;

fn api_router() -> Router {
    Router::new()
        .route("/hello", get(|| async { "Hi!" }))
        .route("/get_id", get(get_id))
        .route("/{id}/status", get(get_status))
        // Add timeout layer specifically for upload api
        .route("/{id}/upload", post(upload_file))
        .layer(TimeoutLayer::new(Duration::from_secs(20))) 
        // Add timeout layer specifically for download api
        .route("/{id}/file", get(get_file))
        .layer(TimeoutLayer::new(Duration::from_secs(20))) 
}

fn assets_router() -> Router {
    Router::new()
        .route("/{path}", get(get_assets))
}

fn view_router() -> Router {
    Router::new()
        .route("/", get(upload))
        .route("/{id}/file", get(download))
}

#[instrument(skip_all)]
pub async fn start_server(ip: &str, port: &str) {
    let app = Router::new()
        .merge(view_router())
        .nest("/api", api_router())
        .nest("/assets", assets_router())
        // 添加静态文件服务，用于提供web/dist目录中的文件
        .fallback_service(ServeDir::new("../web/dist"));

    let addr = format!("{}:{}", ip, port);
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            event!(tracing::Level::ERROR, "Failed to bind to address: {}", e);
            return;
        }
    };

    event!(tracing::Level::INFO, "Server listening on {}", addr);
    match serve(listener, app).await {
        Ok(_) => {}
        Err(e) => event!(tracing::Level::ERROR, "Server error: {}", e),
    }
}