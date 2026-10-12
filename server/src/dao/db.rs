use std::{sync::Arc};
use super::memdb::MemDB;
use super::block_cache::BlockCache;

use axum::body::Bytes;
use lazy_static::lazy_static;
use serde::Serialize;
use serde_json::Value;

#[derive(Clone)]
pub struct MetaInfo {
    pub is_using: bool,
    pub used_by: String, // a random id gen by client
    pub block_size: u32,
    pub file_name: String,
    pub file_size: u64,
    pub done: bool,
    pub cancelled_by: Option<String>,
    pub sender_token: String,
}

impl MetaInfo {

    pub async fn claim(id: &str, receiver: &str) -> Result<(), &'static str> {
        if receiver.is_empty() {
            return Err("Missing receiver ID");
        }
        let db = Self::get_db();
        let mut store = db.store.write().await;
        let entry = store.get_mut(id).filter(|entry| entry.exp > std::time::Instant::now())
            .ok_or("Access ID Not Found")?;
        if entry.value.cancelled_by.is_some() { return Err("Transfer cancelled"); }
        if entry.value.is_using && entry.value.used_by != receiver {
            return Err("File already in use");
        }
        entry.value.is_using = true;
        entry.value.used_by = receiver.to_owned();
        Ok(())
    }

    pub fn get_db() -> Arc<MemDB<MetaInfo>> {
        META_INFO_DB.clone()
    }

    pub fn new(
        file_name: String,
        file_size: u64,
    ) -> Self {
        MetaInfo {
            is_using: false,
            used_by: "".to_string(),
            block_size: 1024 * 1024,
            file_name: file_name,
            file_size: file_size,
            done: false,
            cancelled_by: None,
            sender_token: String::new(),
        }
    }
}

#[derive(Clone)]
pub struct FileBlock {
    pub data: Bytes,
    pub filename: String,
    pub start: u64,
    pub end: u64,
    pub total: u64,
}

impl FileBlock {
    pub fn get_db() -> Arc<BlockCache> {
        FILE_BLOCK_DB.clone()
    }

    pub fn new(data: &Bytes, filename: String, start: u64, end: u64, total: u64) -> Self {
        FileBlock {
            data: data.clone(),
            filename,
            start,
            end,
            total,
        }
    }

}

#[derive(Clone, Serialize)]
pub struct SignalMessage {
    pub seq: u64,
    pub from: String,
    pub msg_type: String,
    pub data: Value,
    pub rid: Option<String>,
}

#[derive(Clone)]
pub struct SignalState {
    pub seq: u64,
    pub messages: Vec<SignalMessage>,
}

impl SignalState {
    pub fn get_db() -> Arc<MemDB<SignalState>> {
        SIGNAL_DB.clone()
    }

    pub fn new() -> Self {
        SignalState {
            seq: 0,
            messages: Vec::new(),
        }
    }
}

lazy_static!{
    pub static ref META_INFO_DB: Arc<MemDB<MetaInfo>> = Arc::new(MemDB::new());
}

lazy_static!{
    pub static ref FILE_BLOCK_DB: Arc<BlockCache> = Arc::new(BlockCache::from_env());
}

lazy_static!{
    pub static ref SIGNAL_DB: Arc<MemDB<SignalState>> = Arc::new(MemDB::new());
}
