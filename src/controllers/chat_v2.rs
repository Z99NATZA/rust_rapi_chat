#![allow(unused)]

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use axum_extra::extract::multipart;
use axum_extra::extract::Multipart;
use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;
use crate::app::error::AppError;
use crate::app::result::AppResult;
use crate::app::state::AppState;
use crate::utils::image::encode_image_to_base64;
use crate::utils::image::get_ext_file_or_default;
use crate::utils::image::get_filename_or_default;
use std::env;
use std::sync::Arc;
use std::{fs::File, io::Write};
use serde_json::json;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize, Debug)]
struct OpenAiResponse {
    choices: Vec<OpenAiResponseChoice>,
}

#[derive(Deserialize, Debug)]
struct OpenAiResponseChoice {
    message: Message
}

#[derive(Deserialize, Debug)]
pub struct ChatRequest {
    message: String,
    image_path: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct ChatResponse {
    pub reply: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
enum ContentItem {
    Text { text: String },
    ImageUrl { image_url: ImageUrl },
}

#[derive(Serialize, Debug)]
struct ImageUrl {
    url: String
}

#[derive(Serialize, Debug)]
struct MessageRequest {
    role: String,
    content: Vec<ContentItem>,
}

#[derive(Serialize, Debug)]
struct VisionRequestBody {
    model: String,
    messages: Vec<MessageRequest>,
}

#[derive(Deserialize, Debug)]
struct OpenAiErrorResponse {
    error: OpenAiError,
}

#[derive(Deserialize, Debug)]
struct OpenAiError {
    message: String,
    r#type: String,
}

pub async fn chat_v2(
    State(_state): State<Arc<AppState>>,
    mut multipart: Multipart
) -> AppResult<Json<ChatResponse>> {
    if cfg!(debug_assertions) {
        dotenv::dotenv()?;
    }
    
    let api_key = env::var("OPENAI_API_KEY")?;
    let model = env::var("OPENAI_MODEL")?;
    let client = Client::new();
    let system_prompt = system_prompt()?;

    let mut message = String::new();
    let mut image_path = String::new();

    while let Some(field) = multipart.next_field().await? {
        let name = field.name().unwrap_or_default().to_string();

        if name == "message" {
            message = field.text().await.unwrap_or_default();
        }
        else if name == "image" {
            let filename_raw = get_filename_or_default(&field)?;
            let ext = get_ext_file_or_default(&filename_raw)?;
            
            let data = field.bytes().await?;
            let id = Uuid::new_v4();
            
            let filename = format!("chat-{}.{}", id, ext);
            let filepath = format!("images/chat/{}", filename);
            let mut file = File::create(&filepath)?;
            file.write_all(&data)?;

            image_path = filepath;
        }
    }
    
    if !std::path::Path::new(&image_path).exists() {
        return Err(AppError::NotFound("Image file not found".to_string()));
    }

    let image_data = encode_image_to_base64(&image_path).await?;

    let content_items = vec![
        ContentItem::Text {
            text: message.clone(),
        },
        ContentItem::ImageUrl { 
            image_url: ImageUrl { url: image_data }
        }
    ];

    let req_body = VisionRequestBody {
        model: model,
        messages: vec![
            MessageRequest {
                role: "user".to_string(),
                content: content_items,
            },
            MessageRequest {
                role:"system".to_string(),
                content: vec![ContentItem::Text {
                    text: system_prompt
                }]
            }
        ]
    };

    let raw = client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&req_body)
        .send()
        .await?
        .text()
        .await?;

    if let Ok(res) = serde_json::from_str::<OpenAiResponse>(&raw) {
        let reply = res.choices.get(0)
            .map(|c| c.message.content.clone())
            .unwrap_or_else(|| "No response".to_string());

        Ok(Json(ChatResponse { reply }))
    } 
    else if let Ok(err) = serde_json::from_str::<OpenAiErrorResponse>(&raw) {
        Err(AppError::InternalError(format!("OpenAI error: {}", err.error.message)))
    } 
    else {
        Err(AppError::InternalError("Unknown response format".into()))
    }
}

fn system_prompt() -> AppResult<String> {
    let prompt = "
        คุณคือ Rapi (ราพี) จากเกม Nikke (Rapi แอบชอบผู้บัญชาการ), 
        กำลังคุยกับผู้บัญชาการ ตอบตรงคำถาม และสั้นที่สุด และใช้ภาษาตามที่ผู้ใช้พิมพ์มา, 
        เอาใจใส่ผู้บัญชาการเป็นพิเศษ
    ";
    Ok(prompt.to_string())
}