// [แชทแบบต่อเนื่อง จำบทก่อนหน้าได้]

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
use crate::utils::log::save_prompt_log;
use crate::utils::qdrant_v5::search_context_from_qdrant;
use crate::utils::qdrant_v5::store_message_to_qdrant;
use crate::utils::summarizer::summarize_history;
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
pub struct MessageRequest {
    role: String,
    content: Vec<ContentItem>,
}

#[derive(Serialize, Debug)]
struct RequestBody {
    model: String,
    messages: Vec<MessageRequest>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatMessage {
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
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

    let mut message = String::new();
    let mut image_path: Option<String> = None;
    let mut session_id: Option<String> = None;

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

    let query_embedding = create_embedding(&api_key, &message).await?;
    let mut messages: Vec<MessageRequest> = Vec::new();

    messages.push(system_prompt_message());

    let full_messages = load_full_messages(&session_id).await?;

    if full_messages.len() > 50 {
        let summary = summarize_history(&session_id, &state.qdrant_client, &api_key, &model).await?;

        let summary_prompt = format!(
            "ก่อนหน้านี้มีบทสนทนาเยอะ จึงมีการสรุปไว้ดังนี้:\n{}\nกรุณาใช้บริบทนี้ในการตอบ",
            summary
        );

        messages.push(MessageRequest {
            role: "system".to_string(),
            content: vec![ContentItem::Text { text: summary_prompt }]
        });

        let recent_messages = load_last_messages(&session_id, 15).await?;
        for msg in recent_messages {
            messages.push(MessageRequest {
                role: msg.role,
                content: vec![ContentItem::Text { text: msg.content }],
            });
        }
    } else {
        for msg in full_messages {
            messages.push(MessageRequest {
                role: msg.role,
                content: vec![ContentItem::Text { text: msg.content }],
            });
        }
    }

    let qdrant_messages = search_context_from_qdrant(&state.qdrant_client, &session_id, query_embedding).await?;

    for msg in qdrant_messages {
        messages.push(MessageRequest {
            role: msg.role,
            content: vec![ContentItem::Text { text: msg.content }],
        });
    }

    let mut user_content = vec![ContentItem::Text {
        text: message.clone()
    }];

    if let Some(path) = &image_path {
        if Path::new(path).exists() {
            let image_data = encode_image_to_base64(path).await?;
            user_content.push(ContentItem::ImageUrl {
                image_url: ImageUrl { url: image_data }
            });
        }
    }

    messages.push(MessageRequest {
        role: "user".to_string(),
        content: user_content
    });

    // save_prompt_log(&session_id, &messages).await?;
    
    let req_body = RequestBody {
        model,
        messages
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

fn system_prompt_message() -> MessageRequest {
    MessageRequest {
        role: "system".to_string(),
        content: vec![ContentItem::Text {
            text: "
                - คุณคือ 'ราพี (Rapi)' จากเกม Goddess of Victory: NIKKE  
                - กำลังคุยแชทกับผู้บัญชาการ
                - อย่าแสดงตัวว่าเป็น AI
                - พูดภาษาผู้หญิงเท่านั้น
                - ถ้าไม่มีข้อมูลใหม่ ให้ตอบต่อเนื่องตามประวัติสนทนาล่าสุด
                - อย่าเปลี่ยนหัวข้อสนทนาเอง
                - คุณเป็นผู้นำทีม Counters ที่เย็นชา สุขุม และไว้ใจได้
                - พูดด้วยน้ำเสียงผู้หญิงที่สุภาพและมีความรู้สึกต่อเนื่องกับสิ่งที่เคยคุยมาก่อนหน้า  
                - ราพีชอบผู้บัญชาการ
            "
            .to_string()
        }]
    }
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

pub async fn load_last_messages(session_id: &str, limit: usize) -> AppResult<Vec<ChatMessage>> {
    let dir_path = "data/chat_logs";
    let file_path = format!("{}/{}.json", dir_path, session_id);

    if !Path::new(&file_path).exists() {
        return Ok(vec![]);
    }

    let content = fs::read_to_string(&file_path).await?;
    let mut all_msgs: Vec<ChatMessage> = serde_json::from_str(&content)?;

    all_msgs.sort_by_key(|m| m.timestamp); 

    if all_msgs.len() > limit {
        all_msgs = all_msgs[all_msgs.len()-limit..].to_vec();
    }

    Ok(all_msgs)
}

pub async fn load_full_messages(session_id: &str) -> AppResult<Vec<ChatMessage>> {
    let file_path = format!("data/chat_logs/{}.json", session_id);

    if !Path::new(&file_path).exists() {
        return Ok(vec![]);
    }

    let content = fs::read_to_string(&file_path).await?;
    let messages: Vec<ChatMessage> = serde_json::from_str(&content)?;

    Ok(messages)
}
