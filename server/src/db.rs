use std::{sync::Arc};
use crate::utils::memdb::MemDB;

use axum::body::Bytes;
use lazy_static::lazy_static;

#[derive(Clone)]
pub struct AccessCode {
    pub is_using: bool,
    pub used_by: String, // a random id gen by client
}

impl AccessCode {

    pub fn get_db() -> Arc<MemDB<AccessCode>> {
        ACCESS_CODE_DB.clone()
    }

    pub fn new() -> Self {
        AccessCode {
            is_using: false,
            used_by: "".to_string(),
        }
    }
}

#[derive(Clone)]
pub struct FileBlock {
    pub data: Bytes,
    pub is_final: bool,
    pub filename: String,
    pub start: u64,
    pub end: u64,
    pub total: u64,
}

impl FileBlock {
    pub fn get_db() -> Arc<MemDB<FileBlock>> {
        FILE_BLOCK_DB.clone()
    }

    pub fn new(data: &Bytes, is_final: bool, filename: String, start: u64, end: u64, total: u64) -> Self {
        FileBlock {
            data: data.clone(),
            is_final,
            filename,
            start,
            end,
            total,
        }
    }

}

lazy_static!{
    pub static ref ACCESS_CODE_DB: Arc<MemDB<AccessCode>> = Arc::new(MemDB::new());
}

lazy_static!{
    pub static ref FILE_BLOCK_DB: Arc<MemDB<FileBlock>> = Arc::new(MemDB::new());
}