#![allow(unused)]

use crate::app::result::AppResult;
use crate::utils::embedding::create_embedding;
use crate::utils::qdrant_v5::store_message_to_qdrant;
use crate::controllers::chat_v5::ChatMessage;

use std::env;
use chrono::Utc;
use qdrant_client::Qdrant;
use reqwest::Client;
use serde_json::json;
use tokio::fs;

pub async fn summarize_history(
    session_id: &str, 
    qdrant: &Qdrant,
    openai_key: &str,
    model: &str
) -> AppResult<String> {
    let file_path = format!("data/chat_logs/{}.json", session_id);

    let content = fs::read_to_string(&file_path).await?;
    let messages: Vec<ChatMessage> = serde_json::from_str(&content)?;

    let mut history_text = String::new();

    for msg in messages.iter() {
        history_text.push_str(&format!("[{}]: {}\n", msg.role, msg.content));
    }

    let system_prompt = "สรุปบทสนทนานี้ให้เป็นย่อหน้าเดียวแบบกระชับ โดยบอกบริบทหลักที่คุยกัน เช่น 'ผู้บัญชาการชวนราพีไปเที่ยวทะเล และกำลังเลือกชุด'";

    let payload = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": history_text }
        ]
    });

    let client = Client::new();
    let res = client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(&openai_key)
        .json(&payload)
        .send()
        .await?;

    let body = res.text().await?;

    #[derive(serde::Deserialize)]
    struct ChoiceMsg {
        choices: Vec<Choice>,
    }

    #[derive(serde::Deserialize)]
    struct Choice {
        message: Message,
    }

    #[derive(serde::Deserialize)]
    struct Message {
        content: String,
    }

    let parsed: ChoiceMsg = serde_json::from_str(&body)?;
    let summary = parsed.choices.first().map(|c: &Choice| c.message.content.clone())
        .unwrap_or("ไม่สามารถสรุปเนื้อหาได้".to_string());

    let embedding = create_embedding(&openai_key, &summary).await?;

    store_message_to_qdrant(
        &qdrant,
        session_id,
        "summary",
        &summary,
        embedding,
        Utc::now().timestamp()
    ).await?;

    Ok(summary)
}
