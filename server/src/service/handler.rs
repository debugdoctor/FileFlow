use std::{collections::HashMap, env, time::Instant};

use crate::{
    dao::{db::{FileBlock, MetaInfo, SignalMessage, SignalState}, memdb::CacheEntry},
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
    static ref UPLOAD_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::new(4);
}

/// TTL for metadata entries (seconds)
const META_TTL_SECS: u64 = 60 * 60 * 24;
/// TTL for signaling data (seconds)
const SIGNAL_TTL_SECS: u64 = 60 * 60;
/// Retry settings for fetching file blocks (kept below client timeout)
const BLOCK_FETCH_MAX_RETRIES: u32 = 60;
const BLOCK_FETCH_RETRY_INTERVAL: u64 = 250;

/// Aggregate file size limit derived from block constraints
fn max_total_size() -> u64 {
    chunk_size().saturating_mul(max_blocks_per_file() as u64)
}
fn chunk_size() -> u64 {
    max_block_size().min(1024 * 1024)
}
/// Maximum size of each file block in bytes (default 1MB, configurable via MAX_BLOCK_SIZE)
fn max_block_size() -> u64 {
    *MAX_BLOCK_SIZE
}
/// Maximum number of blocks allowed per file (default 1024, configurable via MAX_BLOCKS_PER_FILE)
fn max_blocks_per_file() -> usize {
    *MAX_BLOCKS_PER_FILE
}

fn encode_header_filename(filename: &str) -> String {
    filename.as_bytes().iter().map(|byte| {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(byte) {
            (*byte as char).to_string()
        } else {
            format!("%{:02X}", byte)
        }
    }).collect()
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

#[derive(Debug, Deserialize)]
pub struct SignalPostPayload {
    pub role: String,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub data: serde_json::Value,
    pub rid: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SignalQuery {
    pub role: String,
    pub since: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct BlockAck {
    pub rid: String,
    pub start: u64,
}

pub async fn get_transfer_config() -> impl IntoResponse {
    let cache = FileBlock::get_db();
    Json(json!({
        "success": true,
        "data": {
            "chunk_size": chunk_size(),
            "max_file_size": max_total_size(),
            "cache_bytes": cache.max_bytes,
            "window_bytes": cache.window_bytes,
        }
    }))
}

pub async fn acknowledge_block(Path(id): Path<String>, Json(ack): Json<BlockAck>) -> impl IntoResponse {
    let Some(meta) = MetaInfo::get_db().get(&id).await else {
        return (StatusCode::NOT_FOUND, Json(json!({"success": false, "message": "Access ID Not Found"}))).into_response();
    };
    if ack.rid.is_empty() || meta.value.used_by != ack.rid {
        return (StatusCode::BAD_REQUEST, Json(json!({"success": false, "message": "Wrong Receive ID"}))).into_response();
    }
    match FileBlock::get_db().acknowledge(&id, ack.start).await {
        Ok(()) => Json(json!({"success": true})).into_response(),
        Err(message) => (StatusCode::NOT_FOUND, Json(json!({"success": false, "message": message}))).into_response(),
    }
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

/// Handler for returning P2P configuration (STUN/TURN)
pub async fn get_p2p_config() -> impl IntoResponse {
    let stun_server = env::var("STUN_SERVER").ok().and_then(|raw| {
        let trimmed = raw.trim().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    });
    let turn_server = env::var("TURN_SERVER").ok().and_then(|raw| {
        let trimmed = raw.trim().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    });
    let turn_username = env::var("TURN_USERNAME").ok().and_then(|raw| {
        let trimmed = raw.trim().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    });
    let turn_credential = env::var("TURN_CREDENTIAL").ok().and_then(|raw| {
        let trimmed = raw.trim().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    });

    Json(json!({
        "code": 200,
        "success": true,
        "data": {
            "stun": stun_server,
            "turn": turn_server,
            "turn_username": turn_username,
            "turn_credential": turn_credential,
        }
    }))
    .into_response()
}

/// Handler for posting WebRTC signaling messages
pub async fn post_signal(
    Path(id): Path<String>,
    Json(payload): Json<SignalPostPayload>,
) -> impl IntoResponse {
    let role = payload.role.trim();
    if role != "sender" && role != "receiver" {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "code": 400,
                "success": false,
                "message": "Invalid role"
            })),
        )
            .into_response();
    }

    let msg_type = payload.msg_type.trim();
    if msg_type.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "code": 400,
                "success": false,
                "message": "Invalid message type"
            })),
        )
            .into_response();
    }

    if msg_type == "ready" && role == "receiver" {
        if let Err(message) = MetaInfo::claim(&id, payload.rid.as_deref().unwrap_or("")).await {
            return (StatusCode::BAD_REQUEST, Json(json!({
                "code": 400, "success": false, "message": message
            }))).into_response();
        }
    }

    // Append under one lock so simultaneous ICE candidates cannot overwrite each other.
    let db = SignalState::get_db();
    let mut store = db.store.write().await;
    let now = Instant::now();
    let entry = store.entry(id).or_insert_with(|| CacheEntry {
        value: SignalState::new(),
        exp: now + std::time::Duration::from_secs(SIGNAL_TTL_SECS),
    });
    if entry.exp <= now {
        entry.value = SignalState::new();
    }
    entry.exp = now + std::time::Duration::from_secs(SIGNAL_TTL_SECS);
    let state = &mut entry.value;

    state.seq = state.seq.saturating_add(1);
    let seq = state.seq;
    state.messages.push(SignalMessage {
        seq,
        from: role.to_string(),
        msg_type: msg_type.to_string(),
        data: payload.data,
        rid: payload.rid,
    });

    if state.messages.len() > 200 {
        let drain_len = state.messages.len() - 200;
        state.messages.drain(0..drain_len);
    }

    Json(json!({
            "code": 200,
            "success": true,
            "data": {
                "seq": seq
            }
        }))
        .into_response()
}

/// Handler for fetching WebRTC signaling messages
pub async fn get_signal(
    Path(id): Path<String>,
    Query(query): Query<SignalQuery>,
) -> impl IntoResponse {
    let role = query.role.trim();
    if role != "sender" && role != "receiver" {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "code": 400,
                "success": false,
                "message": "Invalid role"
            })),
        )
            .into_response();
    }

    let since = query.since.unwrap_or(0);
    let state = SignalState::get_db().get(&id).await;

    let (messages, latest) = match state {
        Some(entry) => {
            let latest = entry.value.seq;
            let messages = entry
                .value
                .messages
                .iter()
                .filter(|msg| msg.seq > since && msg.from != role)
                .cloned()
                .collect::<Vec<_>>();
            (messages, latest)
        }
        None => (Vec::new(), since),
    };

    Json(json!({
        "code": 200,
        "success": true,
        "data": {
            "latest": latest,
            "messages": messages
        }
    }))
    .into_response()
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

    let mut meta_info = MetaInfo::new(file_name, file_size);
    meta_info.block_size = chunk_size() as u32;

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
                        "chunk_size": meta_info.value.block_size,
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
        if let Err(message) = MetaInfo::claim(&id, &receive_id).await {
            return (StatusCode::BAD_REQUEST, Json(json!({
                "code": 400, "success": false, "message": message
            }))).into_response();
        }
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
        match FileBlock::get_db().get(&id, start).await {
            Some(file_block) => {
                if file_block.start > start {
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
                break (file_block.filename, file_block.data, file_block.start, file_block.end, file_block.total);
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

    // Keep blocks available for retries until the receiver acknowledges completion.

    let headers: [(&str, &str); 3] = [
        ("Content-Name", &encode_header_filename(&block_name)),
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
    // Bound multipart buffers as well as cached payloads.
    let _slot = UPLOAD_SLOTS.acquire().await.unwrap();
    // Changed from INFO to DEBUG to reduce log verbosity for large files
    event!(Level::DEBUG, "Starting file upload for ID: {}", id);
    
    // Allow upload even if receiver hasn't connected yet; only require a valid ID.
    let metadata = match MetaInfo::get_db().get(&id).await {
        Some(meta_info) => {
            if !meta_info.value.is_using {
                event!(Level::DEBUG, "Receiver not connected yet for ID: {}", id);
            }
            meta_info.value
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

        if total != metadata.file_size || start % metadata.block_size as u64 != 0
            || end != start.saturating_add(metadata.block_size as u64).min(total).saturating_sub(1) {
            return (StatusCode::BAD_REQUEST, Json(json!({
                "code": 400, "success": false, "message": "Block range does not match transfer metadata"
            }))).into_response();
        }

        if end < start || total == 0 || start >= total || end >= total {
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

        let file_block = FileBlock::new(
            &data,
            filename.clone(),
            start,
            end,
            total,
        );

        if !metadata.done && !FileBlock::get_db().insert(&id, file_block).await {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                AppendHeaders([("Retry-After", "1")]),
                Json(json!({"code": 429, "success": false, "message": "Waiting for receiver or available RAM"})),
            ).into_response();
        }
        event!(Level::DEBUG, "Successfully uploaded block for '{}' range {}-{} of total {} for ID: {}", filename, start, end, total, id);
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
                    FileBlock::get_db().finish(&id).await;
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Bytes};

    #[tokio::test]
    async fn downloaded_blocks_remain_retryable_until_done() {
        let id = "test-retry-block";
        MetaInfo::get_db().insert(id, MetaInfo::new("中文 & test.txt".into(), 4), 60).await.unwrap();
        assert!(FileBlock::get_db().insert(id,
            FileBlock::new(&Bytes::from_static(b"test"), "中文 & test.txt".into(), 0, 3, 4)).await);
        let query = HashMap::from([("rid".into(), "receiver".into()), ("start".into(), "0".into())]);
        for _ in 0..2 {
            let response = get_file(Path(id.into()), Query(query.clone())).await.into_response();
            assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
            assert_eq!(response.headers()["content-name"], "%E4%B8%AD%E6%96%87%20%26%20test.txt");
            assert_eq!(to_bytes(response.into_body(), 100).await.unwrap(), "test");
        }
        let response = done(Path(id.into()), Json(json!({}))).await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(FileBlock::get_db().get(id, 0).await.is_none());
    }

    #[tokio::test]
    async fn concurrent_signals_preserve_every_message() {
        let id = "test-signal-concurrency";
        let mut tasks = Vec::new();
        for index in 0..50 {
            tasks.push(tokio::spawn(async move {
                let response = post_signal(Path(id.into()), Json(SignalPostPayload {
                    role: "sender".into(), msg_type: "candidate".into(),
                    data: json!({"index": index}), rid: None,
                })).await.into_response();
                assert_eq!(response.status(), StatusCode::OK);
            }));
        }
        for task in tasks { task.await.unwrap(); }
        let entry = SignalState::get_db().get(id).await.unwrap();
        assert_eq!(entry.value.seq, 50);
        assert_eq!(entry.value.messages.len(), 50);
        for (index, message) in entry.value.messages.iter().enumerate() {
            assert_eq!(message.seq, index as u64 + 1);
        }
    }

    #[tokio::test]
    async fn only_one_receiver_can_claim_a_transfer() {
        let id = "test-receiver-claim";
        MetaInfo::get_db().insert(id, MetaInfo::new("test".into(), 4), 60).await.unwrap();
        let (first, second) = tokio::join!(MetaInfo::claim(id, "first"), MetaInfo::claim(id, "second"));
        assert_ne!(first.is_ok(), second.is_ok());
        let winner = if first.is_ok() { "first" } else { "second" };
        assert!(MetaInfo::claim(id, winner).await.is_ok());
    }

    #[tokio::test]
    async fn acknowledgements_are_receiver_bound_and_idempotent() {
        let id = "test-block-ack";
        MetaInfo::get_db().insert(id, MetaInfo::new("test".into(), 4), 60).await.unwrap();
        MetaInfo::claim(id, "receiver").await.unwrap();
        assert!(FileBlock::get_db().insert(id,
            FileBlock::new(&Bytes::from_static(b"test"), "test".into(), 0, 3, 4)).await);
        let wrong = acknowledge_block(Path(id.into()), Json(BlockAck { rid: "wrong".into(), start: 0 })).await.into_response();
        assert_eq!(wrong.status(), StatusCode::BAD_REQUEST);
        assert!(FileBlock::get_db().get(id, 0).await.is_some());
        for _ in 0..2 {
            let response = acknowledge_block(Path(id.into()), Json(BlockAck { rid: "receiver".into(), start: 0 })).await.into_response();
            assert_eq!(response.status(), StatusCode::OK);
        }
        assert!(FileBlock::get_db().get(id, 0).await.is_none());
    }
}
