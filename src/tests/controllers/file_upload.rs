use std::{fs::File, io::Write};

use axum::{http::StatusCode, response::IntoResponse};
use axum_extra::extract::Multipart;
use uuid::Uuid;
use serde_json::json;
use axum::Json;

use crate::app::result::AppResult;

pub async fn file_upload(mut multipart: Multipart) -> AppResult<impl IntoResponse> {
    let mut message = String::new();
    let mut image: String = String::new();

    while let Some(field) = multipart.next_field().await.unwrap() {
        let name = field.name().unwrap_or_default().to_string();

        if name == "message" {
            message = field.text().await.unwrap_or_default();
        }
        else if name == "image" {
            let data = field.bytes().await.unwrap();
            let id = Uuid::new_v4();
            let filename = format!("chat-{}.jpg", id);
            let filepath = format!("images/chat/{}", filename);

            let mut file = File::create(&filepath)?;
            file.write_all(&data)?;

            image = filename;
        }
    }

    Ok((
        StatusCode::OK,
        Json(json!({
            "message": message,
            "image": image
        }))
    ))
}