use std::collections::HashMap;

use crate::{
    db::{AccessCode, FileBlock},
    service::static_files::StaticFiles,
    utils::nanoid,
};
use axum::{
    body::Body, extract::{Multipart, Path, Query}, http::{header, StatusCode}, response::{AppendHeaders, Html, IntoResponse}, Json
};
use mime_guess;
use serde::Deserialize;
use serde_json::json;
use tracing::{instrument};

const MAX_BLOCK_SIZE: u64 = 1024 * 1024; // 1MB
const MAX_BLOCKS_PER_FILE: usize = 4;
const MAX_RETRIES: u32 = 5;
const RETRY_INTERVAL: u64 = 250; // milliseconds

// dto
#[derive(Debug, Deserialize)]
struct FileInfo {
    pub filename: String,
    pub start: u64,
    pub end: u64,
    pub total: u64,
}

#[instrument]
pub async fn upload() -> impl IntoResponse {
    match StaticFiles::get("upload/index.html") {
        Some(content) => {
            let html = String::from_utf8(content.data.to_vec()).unwrap();
            Html(html).into_response()
        }
        None => (StatusCode::NOT_FOUND, "Page not found").into_response(),
    }
}

pub async fn download() -> impl IntoResponse {
    match StaticFiles::get("download/index.html") {
        Some(content) => {
            let html = String::from_utf8(content.data.to_vec()).unwrap();
            Html(html).into_response()
        }
        None => (StatusCode::NOT_FOUND, "Page not found").into_response(),
    }
}

pub async fn get_id() -> impl IntoResponse {
    let id = nanoid::generate();

    let access_code = AccessCode::new();

    match AccessCode::get_db()
        .insert(&id, access_code, 60 * 60 * 24)
        .await
    {
        Ok(_) => {}
        Err(e) => {
            tracing::error!("{}", e);
            return Json(json!({
                "code": 500,
                "success": false,
                "message": "Internal Server Error"
            }))
            .into_response();
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

pub async fn get_status(Path(id): Path<String>) -> impl IntoResponse {
    let access_code = AccessCode::get_db().get(&id).await;
    match access_code {
        Some(access_code) => Json(json!({
            "code": 200,
            "success": true,
            "data": {
                "is_using": access_code.value.is_using,
            }

        })),
        None => Json(json!({
            "code": 404,
            "success": false,
            "message": "Not Found"
        })),
    }
}

#[instrument(skip_all)]
pub async fn get_file(
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let receive_id = match query.get("rid") {
        Some(receive_id) => receive_id.to_string(),
        None => {
            return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "code": 400,
                "success": false,
                "message": "Missing Parameter: rid"
            })))
            .into_response());
        }
    };

    let start = match query.get("start") {
        Some(start) => start.parse::<u64>().unwrap(),
        None => {
            return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "code": 400,
                "success": false,
                "message": "Missing Parameter: start"
            })))
            .into_response());
        }
    };

    if start == 0 {
        let access_code = match AccessCode::get_db().get(&id).await {
            Some(mut access_code) => {
                if access_code.value.is_using {
                    return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "code": 400,
                        "success": false,
                        "message": "Bad Request"
                    })))
                    .into_response());
                }
                access_code.value.is_using = true;
                access_code.value.used_by = receive_id.clone();
                access_code
            }
            None => {
                return Ok((StatusCode::NOT_FOUND, "Access ID Not Found").into_response());
            }
        };

        match AccessCode::get_db()
            .update(&id, access_code.value, access_code.exp)
            .await
        {
            Ok(_) => {}
            Err(_) => {
                return Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "code": 500,
                    "success": false,
                    "message": "Internal Server Error"
                })))
                .into_response());
            }
        };

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    match AccessCode::get_db().get(&id).await {
        Some(access_code) => {
            if access_code.value.used_by != receive_id {
                return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "code": 400,
                    "success": false,
                    "message": "Wrong Receive ID"
                }))).into_response());
            }
        },
        None => {
            return Ok((StatusCode::NOT_FOUND, "Access ID Not Found").into_response());
        }
    };

    // Retry logic for getting file block with 5 retries and 250ms intervals
    let mut retries = 0;
    
    let (block_name, block_data, block_start, block_end, block_total) = loop {
        match FileBlock::get_db().get(&format!("{}:{:012}", &id, start)).await {
            Some(file_block) => {
                if file_block.value.start > start {
                    return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "code": 400,
                        "success": false,
                        "message": "Wrong start position"
                    })))
                    .into_response());
                }
                break (file_block.value.filename, file_block.value.data, file_block.value.start, file_block.value.end, file_block.value.total);
            },
            None => {
                if retries >= MAX_RETRIES {
                    return Ok((StatusCode::NOT_FOUND, format!("Block {}:{:012} Not Found", &id, start)).into_response());
                }
                retries += 1;
                tokio::time::sleep(tokio::time::Duration::from_millis(RETRY_INTERVAL)).await;
            }
        }
    };

    match FileBlock::get_db().remove(&format!("{}:{:012}", &id, start)).await{
        Some(_) => {},
        None => {
            return Ok((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "code": 500,
                "success": false,
                "message": "Missing Block"
            })))
            .into_response());
        }
    }

    let headers: [(&str, &str); 3] = [
        ("Content-Name", &block_name),
        ("Content-Type", "application/octet-stream"),
        ("Content-Range", &format!("bytes {}-{}/{}", block_start, block_end, block_total)),
    ];
    
    Ok((
        StatusCode::PARTIAL_CONTENT,
        AppendHeaders(headers),
        Body::from(block_data)
    ).into_response())
}


pub async fn upload_file(Path(id): Path<String>, multipart: Multipart) -> impl IntoResponse {
    // Check if receiver had visited this id
    match AccessCode::get_db().get(&id).await {
        Some(access_code) => {
            if !access_code.value.is_using {
                return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "code": 400,
                    "success": false,
                    "message": "Receiver had not visited this id"
                })))
                .into_response();
            }
        },
        None => {
            return Json(json!({
                "code": 404,
                "success": false,
                "message": "Missing Access ID"
            }))
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
            tracing::error!("Missing info part");
            return Json(json!({
                "code": 400,
                "success": false,
                "message": "Bad Request: Missing info part"
            }))
            .into_response();
        }
        Err(_) => {
            return Json(json!({
                "code": 500,
                "success": false,
                "message": "Internal Server Error"
            }))
            .into_response();
        }
    } {
        let name = match field.name() {
            Some(name) => name.to_string(),
            None => {
                return Json(json!({
                    "code": 400,
                    "success": false,
                    "message": "Field name is missing"
                }))
                .into_response();
            }
        };

        if name != "info" {
            return Json(json!({
                "code": 400,
                "success": false,
                "message": "First part must be info"
            }))
            .into_response();
        }

        let data = match field.bytes().await {
            Ok(data) => data,
            Err(err) => {
                tracing::error!("Failed to read field bytes: {}", err);
                return Json(json!({
                    "code": 500,
                    "success": false,
                    "message": "Failed to read file data"
                }))
                .into_response();
            }
        };

        let info: FileInfo = match serde_json::from_slice(&data) {
            Ok(info) => info,
            Err(err) => {
                tracing::error!("Failed to parse info json: {}", err);
                return Json(json!({
                    "code": 400,
                    "success": false,
                    "message": "Failed to parse info json"
                }))
                .into_response();
            }
        };

        filename = info.filename;
        start = info.start;
        end = info.end;
        total = info.total;
    }

    // Process file part
    if let Some(field) = match multipart.next_field().await {
        Ok(Some(field)) => Some(field),
        Ok(None) => {
            tracing::error!("Missing file part");
            return Json(json!({
                "code": 400,
                "success": false,
                "message": "Missing file part"
            }))
            .into_response();
        }
        Err(err) => {
            tracing::error!("{}", err);
            return Json(json!({
                "code": 500,
                "success": false,
                "message": "Internal Server Error"
            }))
            .into_response();
        }
    } {
        let name = match field.name() {
            Some(name) => name.to_string(),
            None => {
                tracing::error!("Field name is missing");
                return Json(json!({
                    "code": 400,
                    "success": false,
                    "message": "Field name is missing"
                }))
                .into_response();
            }
        };

        if name != "file" {
            return Json(json!({
                "code": 400,
                "success": false,
                "message": "Second part must be file"
            }))
            .into_response();
        }

        let data = match field.bytes().await {
            Ok(data) => data,
            Err(err) => {
                tracing::error!("Failed to read field bytes: {}", err);
                return Json(json!({
                    "code": 500,
                    "success": false,
                    "message": "Internal Server Error: Failed to read file data"
                }))
                .into_response();
            }
        };

        // Check block size limit
        if data.len() as u64 > MAX_BLOCK_SIZE {
            return Json(json!({
                "code": 400,
                "success": false,
                "message": "Block size exceeds maximum limit of 1024KB"
            }))
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

            if block_count >= MAX_BLOCKS_PER_FILE {
                break;
            }
        }
        drop(store);

        if block_count >= MAX_BLOCKS_PER_FILE {
            return Json(json!({
                  "code": 400,
                  "success": false,
                  "message": "Maximum number of blocks per file reached (8)"
            }))
            .into_response();
        }

        let file_block = FileBlock::new(
            &data,
            filename,
            start,
            end,
            total,
        );

        match FileBlock::get_db()
            .insert(&format!("{}:{:012}", &id, start), file_block, 60)
            .await
        {
            Ok(_) => {}
            Err(e) => {
                tracing::error!("{}", e);
                return Json(json!({
                    "code": 500,
                    "success": false,
                    "message": "Internal Server Error"
                }))
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
        None => (StatusCode::NOT_FOUND, "File not found").into_response(),
    }
}
