use serde::Deserialize;

#[derive(Deserialize)]
pub struct UpdateMetaSchema {
    pub file_name: String,
    pub file_size: u64,
}