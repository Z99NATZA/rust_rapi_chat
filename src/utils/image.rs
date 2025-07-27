#![allow(dead_code)]

use axum_extra::extract::multipart::Field;
use infer;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use base64::engine::general_purpose;
use base64::Engine as _;
use once_cell::sync::OnceCell;
use std::fs;

use crate::app::error::AppError;
use crate::app::result::AppResult;

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
    let filename = field.file_name()
        .map(|s| s.to_owned())
        .unwrap_or_else(|| "default.jpg".to_string());

    Ok(filename)
}

pub fn ensure_chat_image_dir() -> AppResult<()> {
    static INIT_CHAT_DIR: OnceCell<()> = OnceCell::new();

    INIT_CHAT_DIR.get_or_try_init(|| {
        fs::create_dir_all("images/chat")
            .map_err(AppError::from)
    })?;

    Ok(())
}
