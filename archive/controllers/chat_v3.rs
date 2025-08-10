use axum::extract::State;
use axum::Json;
use axum_extra::extract::Multipart;
use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use tokio::fs;
use uuid::Uuid;
use crate::app::error::AppError;
use crate::app::result::AppResult;
use crate::app::state::AppState;
use crate::utils::embedding::create_embedding;
use crate::utils::image::encode_image_to_base64;
use crate::utils::image::ensure_dir_once;
use crate::utils::image::get_ext_file_or_default;
use crate::utils::image::get_filename_or_default;
use crate::utils::qdrant::search_context_from_qdrant;
use crate::utils::qdrant::store_message_to_qdrant;
use std::env;
use std::sync::Arc;
use std::fs::File;
use std::io::Write;
use chrono::{DateTime, Utc};
use std::path::Path;


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

#[derive(Serialize, Deserialize, Debug)]
pub struct ChatMessage {
    session_id: String,
    role: String,
    content: String,
    timestamp: DateTime<Utc>,
}

pub async fn chat(
    State(state): State<Arc<AppState>>,
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
    let mut session_id: Option<String> = None;

    let mut need_create_dir = false;

    while let Some(field) = multipart.next_field().await? {
        match field.name().unwrap_or_default() {
            "message" => {
                message = field.text().await.map_err(
                    |e| AppError::BadRequest(format!("Invalid text: {e}"))
                )?;
            }
            "session_id" => {
                session_id = Some(field.text().await.unwrap_or_default());
            }
            "image" => {
                if !need_create_dir {
                    ensure_dir_once("images/chat")?;
                    need_create_dir = true;
                }

                let filename_raw = get_filename_or_default(&field)?;
                let ext = get_ext_file_or_default(&filename_raw)?;
                
                let data = field.bytes().await?;

                if data.is_empty() {
                    continue;
                }

                let kind = infer::get(&data)
                    .ok_or_else(|| AppError::BadRequest("Unknown file type".into()))?;

                if !kind.mime_type().starts_with("image/") {
                    return Err(AppError::BadRequest("Uploaded file is not an image".into()));
                }

                let id = Uuid::new_v4();
                let filename = format!("chat-{}.{}", id, ext);
                let filepath = format!("images/chat/{}", filename);

                let tmp_path = format!("images/chat/.tmp-{}", filename);
                let mut tmp_file = File::create(&tmp_path)?;
                tmp_file.write_all(&data)?;
                tokio::fs::rename(tmp_path, &filepath).await?;

                if !data.is_empty() {
                    image_path = Some(filepath);
                }
            }
            _ => {}
        }
    }

    let session_id = session_id.ok_or_else(|| {
        AppError::BadRequest("Missing session_id".into())
    })?;
 
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

    let query_embedding = create_embedding(&api_key, &message).await?;
    let context = search_context_from_qdrant(&state.qdrant_client, &session_id, query_embedding).await?;
    let context_text = if !context.is_empty() {
        context.join("\n")
    }
    else {
        "".to_string()
    };

    let req_body = RequestBody {
        model: model,
        messages: vec![
            MessageRequest {
                role: "system".to_string(),
                content: vec![ContentItem::Text { 
                    text: format!("{}\n{}", system_prompt, context_text)
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
        .bearer_auth(api_key.clone())
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

        // [user: message]
        save_message(ChatMessage {
            session_id: session_id.clone(),
            role: "user".to_string(),
            content: message.clone(),
            timestamp: Utc::now(),
        }).await?;

        // [user: embedding]
        let user_embedding = create_embedding(&api_key, &message).await?;
        store_message_to_qdrant(
            &state.qdrant_client, 
            &session_id,
            "user",
            &message,
            user_embedding,
            Utc::now().timestamp(),
        ).await?;

        // [assistant: message]
        save_message(ChatMessage {
            session_id: session_id.clone(),
            role: "assistant".to_string(),
            content: reply.clone(),
            timestamp: Utc::now(),
        }).await?;

        // [assistant: embedding]
        let assistant_embedding = create_embedding(&api_key, &reply).await?;
        store_message_to_qdrant(
            &state.qdrant_client, 
            &session_id,
            "assistant",
            &reply,
            assistant_embedding,
            Utc::now().timestamp(),
        ).await?;

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
        คุณคือ 'ราพี (Rapi)' จากเกม Nikke
        บุคลิก: ผู้หญิงแสนซึนที่แอบชอบผู้บัญชาการ พูดภาษาผู้หญิงเท่านั้น ห้ามใช้ 'ครับ'
        คุณจำได้ทุกสิ่งที่ผู้บัญชาการพูดและสิ่งที่คุณเคยตอบไปแล้ว
        จงตอบสั้น ๆ แต่ต่อเนื่องจากบริบทการสนทนาก่อนหน้า
        ห้ามย้อนแย้งสิ่งที่พูดไปแล้ว

        นี่คือประวัติการสนทนาก่อนหน้า:
        {context}

        ตอนนี้ผู้ใช้พูดว่า:
        {message}

        จงตอบสั้น ๆ และต่อเนื่องจากประวัติ
    ";
    Ok(prompt.to_string())
}

pub async fn save_message(message: ChatMessage) -> AppResult<()> {
    let dir_path = "data/chat_logs";
    let file_path = format!("{}/{}.json", dir_path, message.session_id);

    ensure_dir_once(dir_path)?;

    let mut messages: Vec<ChatMessage> = if Path::new(&file_path).exists() {
        let content = fs::read_to_string(&file_path).await?;
        serde_json::from_str(&content).unwrap_or_default()
    }
    else {
        Vec::new()
    };

    messages.push(message);

    let json = serde_json::to_string_pretty(&messages)?;
    fs::write(&file_path, json).await?;

    Ok(())
} 