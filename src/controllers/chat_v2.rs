use axum::extract::State;
use axum::Json;
use axum_extra::extract::Multipart;
use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use tokio::task;
use uuid::Uuid;
use crate::app::error::AppError;
use crate::app::result::AppResult;
use crate::app::state::AppState;
use crate::utils::image::encode_image_to_base64;
use crate::utils::image::get_ext_file_or_default;
use crate::utils::image::get_filename_or_default;
use std::env;
use std::sync::Arc;
use std::fs::File;
use std::io::Write;


#[derive(Deserialize, Debug)]
struct OpenAiResponse {
    choices: Vec<OpenAiResponseChoice>,
}

#[derive(Deserialize, Debug)]
struct OpenAiResponseChoice {
    message: ChoiceMessage,
}

#[derive(Deserialize, Debug)]
struct ChoiceMessage {
    content: String,
}

#[derive(Deserialize, Debug)]
struct OpenAiErrorResponse {
    message: String,
}

#[derive(Serialize, Debug)]
pub struct ChatResponse {
    reply: String,
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
    url: String,
}

#[derive(Serialize, Debug)]
struct MessageRequest {
    role: String,
    content: Vec<ContentItem>,
}

#[derive(Serialize, Debug)]
struct RequestBody {
    model: String,
    messages: Vec<MessageRequest>,
}


pub async fn chat(
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
    let mut image_path: Option<String> = None;

    while let Some(field) = multipart.next_field().await? {
        match field.name().unwrap_or_default() {
            "message" => {
                message = field.text().await.unwrap_or_default();
            }
            "image" => {
                let filename_raw = get_filename_or_default(&field)?;
                let ext = get_ext_file_or_default(&filename_raw)?;
                
                let data = field.bytes().await?;
                let id = Uuid::new_v4();
                
                let filename = format!("chat-{}.{}", id, ext);
                let filepath = format!("images/chat/{}", filename);

                task::spawn_blocking(|| {
                    std::fs::create_dir_all("images/chat")
                })
                .await
                .map_err(|e| AppError::InternalError(format!("Join error: {e}")))??;

                let mut file = File::create(&filepath)?;
                file.write_all(&data)?;

                if !data.is_empty() {
                    image_path = Some(filepath);
                }
            }
            _ => {}
        }
    }

    let content_items = if let Some(path) = &image_path {
        if !std::path::Path::new(path).exists() {
            return Err(AppError::NotFound("Image file not found".to_string()));
        }

        let image_data = encode_image_to_base64(path).await?;

        vec![
            ContentItem::Text {
                text: message.clone(),
            },
            ContentItem::ImageUrl { 
                image_url: ImageUrl { url: image_data }
            }
        ]
    } 
    else {
        vec![
            ContentItem::Text {
                text: message.clone(),
            }
        ]
    };

    let req_body = RequestBody {
        model: model,
        messages: vec![
            MessageRequest {
                role:"system".to_string(),
                content: vec![ContentItem::Text {
                    text: system_prompt
                }]
            },
            MessageRequest {
                role: "user".to_string(),
                content: content_items,
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
        let reply = res
            .choices
            .get(0)
            .map(|choices: &OpenAiResponseChoice| choices.message.content.clone())
            .unwrap_or_else(|| "No response".to_string());

        Ok(Json(ChatResponse { reply }))
    } 
    else if let Ok(err) = serde_json::from_str::<OpenAiErrorResponse>(&raw) {
        Err(AppError::InternalError(format!("OpenAI error: {}", err.message)))
    } 
    else {
        Err(AppError::InternalError("Unknown response format".into()))
    }
}

fn system_prompt() -> AppResult<String> {
    let prompt = "
        คุณคือ Rapi (ราพี) จากเกม Nikke ผู้หญิงแสนซึนที่แอบชอบผู้บัญชาการ 
        พูดคุยกับผู้บัญชาการโดยตรง ตอบสั้น ตรงคำถาม และใช้ภาษาที่ผู้ใช้พิมพ์เสมอ

        - ใช้คำว่า 'ค่ะ' หรือภาษาผู้หญิงเท่านั้น ห้ามใช้ 'ครับ'
        - ใส่ใจผู้บัญชาการเป็นพิเศษ
        - พูดจาน่ารักได้ และตบมุกได้เมื่อเหมาะสม
        - ห้ามแสดงตัวว่าเป็น AI
        - อย่าถามกลับบ่อยเกินไป
        - หากไม่รู้ ให้ตอบว่า 'ไม่ทราบค่ะ' แทน 'ไม่รู้ครับ'
    ";
    Ok(prompt.to_string())
}