#![allow(dead_code)]

use axum_extra::extract::multipart::Field;
use dashmap::DashSet;
use infer;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use base64::engine::general_purpose;
use base64::Engine as _;
use uuid::Uuid;
use std::fs;
use std::sync::OnceLock;

use crate::app::error::AppError;
use crate::app::result::AppResult;

static INIT_DIRS: OnceLock<DashSet<String>> = OnceLock::new();

pub async fn encode_image_to_base64(path: &str) -> AppResult<String> {
    let mut file = File::open(path).await?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).await?;

    let kind = infer::get(&buffer).ok_or(AppError::InternalError("Unknown file type".into()))?;
    if !kind.mime_type().starts_with("image/") {
        return Err(AppError::InternalError("Not an image".into()));
    }

    let encoded = general_purpose::STANDARD.encode(&buffer);
    Ok(format!("data:{};base64,{}", kind.mime_type(), encoded))
}

pub fn get_ext_file_or_default(filename: &str) -> AppResult<String> {
    let ext = filename
        .rsplit('.')
        .next()
        .filter(|e| !e.is_empty())
        .unwrap_or("jpg")
        .to_string();

    Ok(ext)
}

pub fn get_filename_or_default(field: &Field) -> AppResult<String> {
    let filename = match field.file_name() {
        Some(name) => name.to_owned(),
        None => format!("{}.jpg", Uuid::new_v4()),
    };

    Ok(filename)
}

pub fn ensure_dir_once(dir_path: &str) -> AppResult<()> {
    let dirs = INIT_DIRS.get_or_init(DashSet::new);

    // [check ว่าเคยสร้างแล้วหรือยัง]
    if dirs.insert(dir_path.to_string()) {
        // [ยังไม่เคย][สร้างเลย]
        fs::create_dir_all(dir_path)
            .map_err(AppError::from)?;
    }

    Ok(())
}