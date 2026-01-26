use std::{collections::HashMap, env};

use crate::{
    dao::db::{MetaInfo, FileBlock},
    service::static_files::StaticFiles,
    utils::nanoid,
};
use axum::{
    body::Body, extract::{Multipart, Path, Query}, http::{header, StatusCode}, response::{AppendHeaders, Html, IntoResponse}, Json
};
use mime_guess;
use serde::Deserialize;
use serde_json::json;
use tracing::{event, instrument, Level};
use lazy_static::lazy_static;

lazy_static! {
    static ref MAX_BLOCK_SIZE: u64 = read_env_u64("MAX_BLOCK_SIZE", 1024 * 1024);
    static ref MAX_BLOCKS_PER_FILE: usize = read_env_usize("MAX_BLOCKS_PER_FILE", 1024);
}

/// Maximum number of retry attempts for database operations
const MAX_RETRIES: u32 = 5;
/// Interval between retry attempts in milliseconds
const RETRY_INTERVAL: u64 = 250;
/// TTL for metadata entries (seconds)
const META_TTL_SECS: u64 = 60 * 60 * 24;
/// TTL for file block entries (seconds)
const BLOCK_TTL_SECS: u64 = 60;
/// Retry settings for fetching file blocks (kept below client timeout)
const BLOCK_FETCH_MAX_RETRIES: u32 = 60;
const BLOCK_FETCH_RETRY_INTERVAL: u64 = 250;

/// Aggregate file size limit derived from block constraints
fn max_total_size() -> u64 {
    max_block_size() * max_blocks_per_file() as u64
}
/// Maximum size of each file block in bytes (default 1MB, configurable via MAX_BLOCK_SIZE)
fn max_block_size() -> u64 {
    *MAX_BLOCK_SIZE
}
/// Maximum number of blocks allowed per file (default 1024, configurable via MAX_BLOCKS_PER_FILE)
fn max_blocks_per_file() -> usize {
    *MAX_BLOCKS_PER_FILE
}

fn parse_u64_param(value: Option<&String>, field: &str) -> Result<u64, (StatusCode, Json<serde_json::Value>)> {
    let raw = value.ok_or_else(|| {
        event!(Level::WARN, "Missing Parameter: {}", field);
        (StatusCode::BAD_REQUEST, Json(json!({
            "code": 400,
            "success": false,
            "message": format!("Missing Parameter: {}", field)
        })))
    })?;

    raw.parse::<u64>().map_err(|_| {
        event!(Level::WARN, "Invalid numeric Parameter: {}", field);
        (StatusCode::BAD_REQUEST, Json(json!({
            "code": 400,
            "success": false,
            "message": format!("Invalid Parameter: {}", field)
        })))
    })
}

fn read_env_u64(key: &str, default: u64) -> u64 {
    match env::var(key) {
        Ok(raw) => match raw.trim().parse::<u64>() {
            Ok(value) if value > 0 => value,
            _ => {
                event!(Level::WARN, "{} is invalid (value: '{}'), using default {}", key, raw, default);
                default
            }
        },
        Err(_) => default,
    }
}

fn read_env_usize(key: &str, default: usize) -> usize {
    match env::var(key) {
        Ok(raw) => match raw.trim().parse::<u64>() {
            Ok(value) if value > 0 => value.min(usize::MAX as u64) as usize,
            _ => {
                event!(Level::WARN, "{} is invalid (value: '{}'), using default {}", key, raw, default);
                default
            }
        },
        Err(_) => default,
    }
}

/// Data transfer object for file information
#[derive(Debug, Deserialize)]
struct FileInfo {
    pub filename: String,
    pub start: u64,
    pub end: u64,
    pub total: u64,
}

/// Handler for serving the landing page
/// Returns the index HTML page or 404 if not found
#[instrument]
pub async fn home() -> impl IntoResponse {
    match StaticFiles::get("index.html") {
        Some(content) => {
            let html = String::from_utf8(content.data.to_vec()).unwrap();
            Html(html).into_response()
        }
        None => {
            event!(Level::ERROR, "Landing page not found");
            (StatusCode::NOT_FOUND, "Page not found").into_response()
        },
    }
}

/// Handler for serving the upload page
/// Returns the upload HTML page or 404 if not found
#[instrument]
pub async fn upload() -> impl IntoResponse {
    match StaticFiles::get("upload/index.html") {
        Some(content) => {
            let html = String::from_utf8(content.data.to_vec()).unwrap();
            Html(html).into_response()
        }
        None => {
            event!(Level::ERROR, "Upload page not found");
            (StatusCode::NOT_FOUND, "Page not found").into_response()
        },
    }
}

/// Handler for serving the download page
/// Returns the download HTML page or 404 if not found
pub async fn download() -> impl IntoResponse {
    match StaticFiles::get("download/index.html") {
        Some(content) => {
            let html = String::from_utf8(content.data.to_vec()).unwrap();
            Html(html).into_response()
        }
        None => {
            event!(Level::ERROR, "Download page not found");
            (StatusCode::NOT_FOUND, "Page not found").into_response()
        },
    }
}

/// Handler for generating a unique file ID
/// Accepts file name and size as query parameters
/// Returns a unique ID that can be used for file transfer
#[instrument]
pub async fn get_id(
    Query(query): Query<HashMap<String, String>>
) -> impl IntoResponse {
    let id = nanoid::generate();

    let file_name = query.get("file_name").unwrap_or(&String::new()).to_string();
    let file_size = match parse_u64_param(query.get("file_size"), "file_size") {
        Ok(size) => size,
        Err(err) => return err.into_response(),
    };

    if file_size > max_total_size() {
        event!(Level::WARN, "File too large during ID request: {} bytes > {}", file_size, max_total_size());
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "code": 400,
                "success": false,
                "message": "File exceeds maximum allowed size"
            }))
        )
        .into_response();
    }

    // Changed from INFO to DEBUG to reduce log verbosity
    event!(Level::DEBUG, "Generating new ID for file '{}' with size {}", file_name, file_size);

    let meta_info = MetaInfo::new(file_name, file_size);

    match MetaInfo::get_db().insert(&id, meta_info, META_TTL_SECS).await {
        Ok(_) => {
            // Changed from INFO to DEBUG to reduce log verbosity
            event!(Level::DEBUG, "Successfully generated ID: {}", id);
        },
        Err(e) => {
            event!(Level::ERROR, "Failed to insert meta info into DB: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "code": 500,
                    "success": false,
                    "message": "Internal Server Error"
                }))
            ).into_response();
        }
    };

    Json(json!({
        "code": 200,
        "success": true,
        "data": {
            "id": id
        }
    }))
    .into_response()
}

/// Handler for checking the status of a file transfer
/// Returns file metadata and transfer status
#[instrument]
pub async fn get_status(Path(id): Path<String>) -> impl IntoResponse {
    // Changed from DEBUG to TRACE to reduce log verbosity
    event!(Level::TRACE, "Checking status for ID: {}", id);
    
    let meta_info = MetaInfo::get_db().get(&id).await;
    match meta_info {
        Some(meta_info) => {
            // Changed from DEBUG to TRACE to reduce log verbosity
            event!(Level::TRACE, "Status checked for ID: {}", id);
            (
                StatusCode::OK,
                Json(json!({
                    "code": 200,
                    "success": true,
                    "data": {
                        "file_name": meta_info.value.file_name,
                        "file_size": meta_info.value.file_size,
                        "is_using": meta_info.value.is_using,
                        "done": meta_info.value.done,
                    }

                }))
            )
        },
        None => {
            event!(Level::WARN, "Status check failed - ID not found: {}", id);
            (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "code": 404,
                    "success": false,
                    "message": "Not Found"
                }))
            )
        },
    }
}

/// Handler for downloading file chunks
/// Supports range requests for chunked file transfer
/// Includes retry logic and atomic operations for concurrent access
#[instrument(skip_all)]
pub async fn get_file(
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let receive_id = match query.get("rid") {
        Some(receive_id) => receive_id.to_string(),
        None => {
            event!(Level::WARN, "Missing Parameter: rid");
            return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "code": 400,
                "success": false,
                "message": "Missing Parameter: rid"
            })))
            .into_response();
        }
    };

    let start = match parse_u64_param(query.get("start"), "start") {
        Ok(s) => s,
        Err(err) => return err.into_response(),
    };

    if start == 0 {
        // Changed from INFO to DEBUG to reduce log verbosity
        event!(Level::DEBUG, "Starting file download for ID: {} by receiver: {}", id, receive_id);
        // Try to get the metadata and atomically update it in a single operation
        let mut retries = 0;
        
        loop {
            let meta_info = MetaInfo::get_db().get(&id).await;
            
            match meta_info {
                Some(mut current_meta) => {
                    if current_meta.value.is_using
                        && !current_meta.value.used_by.is_empty()
                        && current_meta.value.used_by != receive_id
                    {
                        event!(Level::WARN, "File already in use for ID: {}", id);
                        return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({
                            "code": 400,
                            "success": false,
                            "message": "Bad Request"
                        })))
                        .into_response();
                    }

                    let should_update = !current_meta.value.is_using
                        || current_meta.value.used_by.is_empty()
                        || current_meta.value.used_by != receive_id;

                    if should_update {
                        current_meta.value.is_using = true;
                        current_meta.value.used_by = receive_id.clone();

                        match MetaInfo::get_db().update(&id, current_meta.value, current_meta.exp).await {
                            Ok(_) => {
                                event!(Level::DEBUG, "Successfully updated metadata for ID: {}", id);
                                break;
                            }
                            Err(_) => {
                                retries += 1;
                                if retries >= MAX_RETRIES {
                                    event!(Level::ERROR, "Failed to update metadata after {} retries for ID: {}", MAX_RETRIES, id);
                                    return (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(json!({
                                        "code": 500,
                                        "success": false,
                                        "message": "Internal Server Error"
                                    })))
                                    .into_response();
                                }
                                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                            }
                        }
                    } else {
                        break;
                    }
                }
                None => {
                    event!(Level::WARN, "Access ID Not Found: {}", id);
                    return (StatusCode::NOT_FOUND, "Access ID Not Found").into_response();
                }
            }
        }
        
        // Small delay to ensure atomicity
        tokio::time::sleep(tokio::time::Duration::from_millis(RETRY_INTERVAL)).await;
    }

    match MetaInfo::get_db().get(&id).await {
        Some(meta_info) => {
            if meta_info.value.used_by != receive_id {
                event!(Level::WARN, "Wrong Receive ID for ID: {}", id);
                return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "code": 400,
                    "success": false,
                    "message": "Wrong Receive ID"
                }))).into_response();
            }
        },
        None => {
            event!(Level::WARN, "Access ID Not Found during verification: {}", id);
            return (StatusCode::NOT_FOUND, "Access ID Not Found").into_response();
        }
    };

    // Retry logic for getting file block with extended retries while staying under client timeout
    let mut retries = 0;
    
    let (block_name, block_data, block_start, block_end, block_total) = loop {
        match FileBlock::get_db().get(&format!("{}:{:012}", &id, start)).await {
            Some(file_block) => {
                if file_block.value.start > start {
                    event!(Level::WARN, "Wrong start position for ID: {} and start: {}", id.clone(), start);
                    return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "code": 400,
                        "success": false,
                        "message": "Wrong start position"
                    })))
                    .into_response();
                }
                // Changed from DEBUG to TRACE to reduce log verbosity
                event!(Level::TRACE, "Retrieved block for ID: {} and start: {}", id.clone(), start);
                break (file_block.value.filename, file_block.value.data, file_block.value.start, file_block.value.end, file_block.value.total);
            },
            None => {
                if retries >= BLOCK_FETCH_MAX_RETRIES {
                    event!(Level::WARN, "Block {}:{:012} not ready after {} retries", &id, start, BLOCK_FETCH_MAX_RETRIES);
                    return (
                        StatusCode::TOO_EARLY,
                        Json(json!({
                            "code": 425,
                            "success": false,
                            "message": "Block not ready, retry shortly"
                        }))
                    )
                    .into_response();
                }
                retries += 1;
                tokio::time::sleep(tokio::time::Duration::from_millis(BLOCK_FETCH_RETRY_INTERVAL)).await;
            }
        }
    };

    // Delete the block in a separate async task to prevent blocking the response
    // This ensures that even if the deletion fails, the response is still sent
    let delete_task = async |id, start|{
        match FileBlock::get_db().remove(&format!("{}:{:012}", &id, start)).await {
            Some(_) => {
                // Changed from DEBUG to TRACE to reduce log verbosity
                event!(Level::TRACE, "Successfully removed block {}:{:012}", &id, start);
            }
            None => {
                // Changed from WARN to TRACE to reduce log verbosity
                event!(Level::TRACE, "Attempted to remove non-existent block {}:{:012}", &id, start);
            }
        }
    };
    
    // Spawn the deletion task but don't wait for it to complete
    tokio::spawn(delete_task(id.clone(), start));

    let headers: [(&str, &str); 3] = [
        ("Content-Name", &block_name),
        ("Content-Type", "application/octet-stream"),
        ("Content-Range", &format!("bytes {}-{}/{}", block_start, block_end, block_total)),
    ];
    
    // Changed from DEBUG to TRACE to reduce log verbosity
    event!(Level::TRACE, "Sending file block for ID: {} range: {}-{}", id, block_start, block_end);
    
    (
        StatusCode::PARTIAL_CONTENT,
        AppendHeaders(headers),
        Body::from(block_data)
    ).into_response()
}


/// Handler for uploading file chunks
/// Processes multipart form data with file info and chunk data
/// Includes validation for block size and file limits
#[instrument]
pub async fn upload_file(Path(id): Path<String>, multipart: Multipart) -> impl IntoResponse {
    // Changed from INFO to DEBUG to reduce log verbosity for large files
    event!(Level::DEBUG, "Starting file upload for ID: {}", id);
    
    // Allow upload even if receiver hasn't connected yet; only require a valid ID.
    match MetaInfo::get_db().get(&id).await {
        Some(meta_info) => {
            if !meta_info.value.is_using {
                event!(Level::DEBUG, "Receiver not connected yet for ID: {}", id);
            }
        },
        None => {
            event!(Level::WARN, "Missing Access ID: {}", id);
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "code": 404,
                    "success": false,
                    "message": "Missing Access ID"
                }))
            )
            .into_response();
        }
    };


    let mut multipart = multipart;

    let mut filename: String = String::new();
    let mut start: u64 = 0;
    let mut end: u64 = 0;
    let mut total: u64 = 0;

    // Process info part
    if let Some(field) = match multipart.next_field().await {
        Ok(Some(field)) => Some(field),
        Ok(None) => {
            event!(Level::ERROR, "Missing info part");
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "code": 400,
                    "success": false,
                    "message": "Bad Request: Missing info part"
                }))
            )
            .into_response();
        }
        Err(e) => {
            event!(Level::ERROR, "Failed to process multipart: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "code": 500,
                    "success": false,
                    "message": "Internal Server Error"
                }))
            )
            .into_response();
        }
    } {
        let name = match field.name() {
            Some(name) => name.to_string(),
            None => {
                event!(Level::WARN, "Field name is missing");
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "code": 400,
                        "success": false,
                        "message": "Field name is missing"
                    }))
                )
                .into_response();
            }
        };

        if name != "info" {
            event!(Level::WARN, "First part must be info");
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "code": 400,
                    "success": false,
                    "message": "First part must be info"
                }))
            )
            .into_response();
        }

        let data = match field.bytes().await {
            Ok(data) => data,
            Err(err) => {
                event!(Level::ERROR, "Failed to read field bytes: {}", err);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "code": 500,
                        "success": false,
                        "message": "Failed to read file data"
                    }))
                )
                .into_response();
            }
        };

        let info: FileInfo = match serde_json::from_slice(&data) {
            Ok(info) => info,
            Err(err) => {
                event!(Level::ERROR, "Failed to parse info json: {}", err);
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "code": 400,
                        "success": false,
                        "message": "Failed to parse info json"
                    }))
                )
                .into_response();
            }
        };

        filename = info.filename;
        start = info.start;
        end = info.end;
        total = info.total;

        if end < start || total == 0 || start >= total {
            event!(Level::WARN, "Invalid range in info part: start={}, end={}, total={}", start, end, total);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "code": 400,
                    "success": false,
                    "message": "Invalid file range"
                }))
            )
            .into_response();
        }

        let max_total = max_total_size();
        if total > max_total {
            event!(Level::WARN, "File too large: {} > {}", total, max_total);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "code": 400,
                    "success": false,
                    "message": "File exceeds maximum allowed size"
                }))
            )
            .into_response();
        }
        
        // Changed from DEBUG to TRACE to reduce log verbosity for large files
        event!(Level::TRACE, "Processed info part for file '{}' with range {}-{} of total {}", filename, start, end, total);
    }

    // Process file part
    if let Some(field) = match multipart.next_field().await {
        Ok(Some(field)) => Some(field),
        Ok(None) => {
            event!(Level::ERROR, "Missing file part");
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "code": 400,
                    "success": false,
                    "message": "Missing file part"
                }))
            )
            .into_response();
        }
        Err(err) => {
            event!(Level::ERROR, "Failed to process multipart: {}", err);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "code": 500,
                    "success": false,
                    "message": "Internal Server Error"
                }))
            )
            .into_response();
        }
    } {
        let name = match field.name() {
            Some(name) => name.to_string(),
            None => {
                event!(Level::WARN, "Field name is missing");
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "code": 400,
                        "success": false,
                        "message": "Field name is missing"
                    }))
                )
                .into_response();
            }
        };

        if name != "file" {
            event!(Level::WARN, "Second part must be file");
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "code": 400,
                    "success": false,
                    "message": "Second part must be file"
                }))
            )
            .into_response();
        }

        let data = match field.bytes().await {
            Ok(data) => data,
            Err(err) => {
                event!(Level::ERROR, "Failed to read field bytes: {}", err);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "code": 500,
                        "success": false,
                        "message": "Internal Server Error: Failed to read file data"
                    }))
                )
                .into_response();
            }
        };

        // Check block size limit
        if data.len() as u64 > max_block_size() {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "code": 400,
                    "success": false,
                    "message": "Block size exceeds maximum limitation"
                }))
            )
            .into_response();
        }

        let expected_len = end.saturating_sub(start).saturating_add(1);
        if data.len() as u64 != expected_len {
            event!(Level::WARN, "Mismatched block length for ID {}: expected {}, got {}", id, expected_len, data.len());
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "code": 400,
                    "success": false,
                    "message": "Block size mismatch"
                }))
            )
            .into_response();
        }
        // Check if meet the max blocks per file in cache
        let file_block_db = FileBlock::get_db();
        let mut block_count = 0;
        let prefix = format!("{}:", id);
        let store = file_block_db.store.read().await;
        
        for key in store.keys() {
            if key.starts_with(&prefix) {
                block_count += 1;
            }

            if block_count >= max_blocks_per_file() {
                break;
            }
        }
        drop(store);

        if block_count >= max_blocks_per_file() {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                      "code": 400,
                      "success": false,
                      "message": format!("Maximum number of blocks per file reached ({})", max_blocks_per_file())
                }))
            )
            .into_response();
        }

        let file_block = FileBlock::new(
            &data,
            filename.clone(),
            start,
            end,
            total,
        );

        match FileBlock::get_db()
            .insert(&format!("{}:{:012}", &id, start), file_block, BLOCK_TTL_SECS)
            .await
        {
            Ok(_) => {
                // Changed from INFO to DEBUG to reduce log verbosity for large files
                event!(Level::DEBUG, "Successfully uploaded block for '{}' range {}-{} of total {} for ID: {}", filename, start, end, total, id);
            },
            Err(e) => {
                event!(Level::ERROR, "Failed to insert file block into DB: {} for ID: {}", e, id);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "code": 500,
                        "success": false,
                        "message": "Internal Server Error"
                    }))
                )
                .into_response();
            }
        }
    }

    Json(json!({
        "code": 200,
        "success": true,
        "message": "Upload Success"
    }))
    .into_response()
}

/// Handler for marking file download as complete
/// Updates the metadata to indicate successful download
#[instrument]
pub async fn done(Path(id): Path<String>, Json(_payload): Json<serde_json::Value>) -> impl IntoResponse {
    // Mark download as complete for the given ID
    match MetaInfo::get_db().get(&id).await {
        Some(mut meta_info) => {
            meta_info.value.done = true;
            match MetaInfo::get_db().update(&id, meta_info.value, meta_info.exp).await {
                Ok(_) => {
                    event!(Level::DEBUG, "Download marked as complete for ID: {}", id);
                    (
                        StatusCode::OK,
                        Json(json!({
                            "code": 200,
                            "success": true,
                            "message": "Download completion marked successfully"
                        }))
                    )
                },
                Err(e) => {
                    event!(Level::ERROR, "Failed to update download completion status: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({
                            "code": 500,
                            "success": false,
                            "message": "Internal Server Error"
                        }))
                    )
                }
            }
        },
        None => {
            event!(Level::WARN, "ID not found for download completion: {}", id);
            (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "code": 404,
                    "success": false,
                    "message": "Not Found"
                }))
            )
        }
    }
}

/// Handler for serving static assets
/// Returns CSS, JS, and other static files with appropriate MIME types
#[instrument(skip_all)]
pub async fn get_assets(Path(file): Path<String>) -> impl IntoResponse {
    match StaticFiles::get(format!("assets/{}", file).as_str()) {
        Some(f) => {
            let mime = mime_guess::from_path(&file).first_or_octet_stream();
            let headers = AppendHeaders([(
                header::CONTENT_TYPE,
                mime.as_ref().to_string().parse::<axum::http::HeaderValue>().unwrap(),
            )]);
            (headers, Body::from(f.data)).into_response()
        },
        None => {
            event!(Level::WARN, "Asset file not found: {}", file);
            (StatusCode::NOT_FOUND, "File not found").into_response()
        },
    }
}
