use std::collections::HashMap;

use crate::{
    db::{AccessCode, FileBlock},
    service::static_files::StaticFiles,
    utils::nanoid,
};
use async_stream::stream;
use axum::{
    body::Body, extract::{Multipart, Path, Query}, http::{StatusCode}, response::{AppendHeaders, Html, IntoResponse}, Json
};
use futures::stream::Stream;
use serde::Deserialize;
use serde_json::json;
use tracing::instrument;

const RETRY_COUNT: u32 = 60;
const MAX_BLOCK_SIZE: u64 = 1024 * 1024; // 1024KB
const MAX_BLOCKS_PER_FILE: usize = 8;

// dto
#[derive(Debug, Deserialize)]
struct FileInfo {
    pub is_final: u8,
    pub filename: String,
    pub start: u64,
    pub end: u64,
    pub total: u64,
}

#[instrument]
pub async fn root() -> impl IntoResponse {
    match StaticFiles::get("index.html") {
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

pub async fn get_file(
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let receive_id = match query.get("receive_id") {
        Some(receive_id) => receive_id.to_string(),
        None => {
            return Ok(Json(json!({
                "code": 400,
                "success": false,
                "message": "Bad Request"
            }))
            .into_response());
        }
    };

    let access_code = match AccessCode::get_db().get(&id).await {
        Some(mut access_code) => {
            if access_code.value.is_using {
                return Ok(Json(json!({
                    "code": 400,
                    "success": false,
                    "message": "Bad Request"
                }))
                .into_response());
            }
            access_code.value.is_using = true;
            access_code.value.used_by = receive_id;
            access_code
        }
        None => {
            return Ok((StatusCode::NOT_FOUND, "Not Found").into_response());
        }
    };

    match AccessCode::get_db()
        .update(&id, access_code.value, access_code.exp)
        .await
    {
        Ok(_) => {}
        Err(e) => {
            tracing::error!("{}", e);
            return Ok(Json(json!({
                "code": 500,
                "success": false,
                "message": "Internal Server Error"
            }))
            .into_response());
        }
    };

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Get file size and filename
    let file_block_db = FileBlock::get_db();
    let mut total_size = 0u64;
    let mut filename = String::new();

    // Calculate file size and get filename
    let mut retry = 0;
    loop {
        match file_block_db.get(&format!("{}:{:012}", id, 0)).await {
            Some(file_block) => {
                total_size = file_block.value.total;
                filename = file_block.value.filename.clone();
                break;
            }
            None => {
                retry += 1;
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                if retry > RETRY_COUNT {
                    return Ok(Json(json!({
                        "code": 500,
                        "success": false,
                        "message": "Internal Server Error"
                    }))
                    .into_response());
                }
            }
        }
    }

    // Check if we can create a valid stream
    let stream = stream_file_data(id, 0, None).await;

    // Build response headers
    let content_disposition = format!("attachment; filename=\"{}\"", filename);
    let content_type = "application/octet-stream";
    let content_length = total_size.to_string();

    let response_headers = vec![
        ("content-type", content_type),
        ("content-disposition", content_disposition.as_str()),
        ("content-length", content_length.as_str()),
    ];

    let headers: Vec<(&str, &str)> = response_headers.iter().map(|(k, v)| (*k, *v)).collect();

    Ok((
        StatusCode::OK,
        AppendHeaders(headers),
        Body::from_stream(stream),
    )
        .into_response())
}

// Support range request
async fn stream_file_data(
    id: String,
    start_pos: u64,
    end_pos: Option<u64>,
) -> impl Stream<Item = Result<axum::body::Bytes, String>> {
    stream! {
        let file_block_db = FileBlock::get_db();
        let mut current_pos = 0u64;
        let mut is_finished = false;
        let mut retry = 0;

        while current_pos < start_pos && !is_finished {
            match file_block_db.get(&format!("{}:{:012}", id, current_pos)).await {
                Some(file_block) => {
                    is_finished = file_block.value.is_final;
                    current_pos = file_block.value.end;
                }
                None => {
                    if retry > RETRY_COUNT {
                        yield Err("Internal Server Error".to_string());
                        break;
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        }

        // Start streaming data from start position
        let mut current_pos = start_pos;
        is_finished = false;
        retry = 0;

        while !is_finished {
            match file_block_db.get(&format!("{}:{:012}", id, current_pos)).await {
                Some(file_block) => {
                    is_finished = file_block.value.is_final;
                    let data = file_block.value.data;
                    current_pos = file_block.value.end;

                    if let Some(end) = end_pos {
                        let chunk_start = file_block.value.start;
                        let chunk_end = file_block.value.end - 1;

                        // Skip if chunk is completely before request range
                        if chunk_end < start_pos {
                            continue;
                        }

                        if chunk_start > end {
                            break;
                        }

                        // Slice data if needed
                        if chunk_start < start_pos || chunk_end > end {
                            let slice_start = if chunk_start < start_pos {
                                (start_pos - chunk_start) as usize
                            } else {
                                0
                            };

                            let slice_end = if chunk_end > end {
                                (slice_start as u64 + (end - chunk_start.max(start_pos)) + 1) as usize
                            } else {
                                data.len()
                            };

                            if slice_start < data.len() && slice_end <= data.len() && slice_start < slice_end {
                                // Remove the block from database after sending it
                                let _ = file_block_db.remove(&format!("{}:{:012}", id, current_pos - (file_block.value.end - file_block.value.start))).await;
                                yield Ok(data.slice(slice_start..slice_end));
                            }
                            break;
                        } else {
                            // Remove the block from database after sending it
                            let _ = file_block_db.remove(&format!("{}:{:012}", id, current_pos - (file_block.value.end - file_block.value.start))).await;
                            yield Ok(data);
                        }
                    } else {
                        // Remove the block from database after sending it
                        let _ = file_block_db.remove(&format!("{}:{:012}", id, current_pos - (file_block.value.end - file_block.value.start))).await;
                        yield Ok(data);
                    }
                }
                None => {
                    retry += 1;
                    if retry > RETRY_COUNT {
                        yield Err("Wait for upload data timeout".to_string());
                        break;
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                }
            }
        }

        AccessCode::get_db().remove(&format!("{}", id)).await;
    }
}

pub async fn upload_file(Path(id): Path<String>, multipart: Multipart) -> impl IntoResponse {
    let mut multipart = multipart;

    let mut is_final: u8 = 0;
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
                    "message": "Bad Request: Field name is missing"
                }))
                .into_response();
            }
        };

        if name != "info" {
            tracing::error!("First part is not info");
            return Json(json!({
                "code": 400,
                "success": false,
                "message": "Bad Request: First part must be info"
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

        let info: FileInfo = match serde_json::from_slice(&data) {
            Ok(info) => info,
            Err(err) => {
                tracing::error!("Failed to parse info json: {}", err);
                return Json(json!({
                    "code": 400,
                    "success": false,
                    "message": "Bad Request: Failed to parse info json"
                }))
                .into_response();
            }
        };

        is_final = info.is_final;
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
                "message": "Bad Request: Missing file part"
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
                    "message": "Bad Request: Field name is missing"
                }))
                .into_response();
            }
        };

        if name != "file" {
            tracing::error!("Second part is not file");
            return Json(json!({
                "code": 400,
                "success": false,
                "message": "Bad Request: Second part must be file"
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
        // Check max blocks per file
        let file_block_db = FileBlock::get_db();
        let mut block_count = 0;
        let prefix = format!("{}:", id);
        let store = file_block_db.store.read().await;
        for key in store.keys() {
            if key.starts_with(&prefix) {
                block_count += 1;
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
            data.len() as u64,
            is_final > 0,
            filename,
            start,
            end,
            total,
        );
        match FileBlock::get_db()
            .insert(&format!("{}:{:012}", &id, start), file_block, 8)
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
