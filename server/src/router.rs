use axum::{routing::{get, post}, serve, Router};
use tokio::net::{TcpListener};
use tracing::{event, instrument};

use crate::service::handler::{download, get_file, get_id, get_status, upload, upload_file};

fn api_router() -> Router {
    Router::new()
        .route("/hello", get(|| async { "Hi!" }))
        .route("/get_id", get(get_id))
        .route("/{id}/status", get(get_status))
        .route("/{id}/upload", post(upload_file))
        .route("/{id}/file", get(get_file))
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
        .nest_service("/assets", axum::routing::get_service(
            tower_http::services::ServeDir::new("../web/dist/assets")
        ));

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
        Err(e) => {
            event!(tracing::Level::ERROR, "Server error: {}", e);
        }
    };
}
