use axum::extract::State;
use axum::Json;
use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;

use crate::app::result::AppResult;
use crate::app::state::AppState;
use std::env;
use std::sync::Arc;

#[derive(Serialize)]
pub struct RequestBody {
    model: String,
    messages: Vec<Message>,
}

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
    message: String
}

#[derive(Serialize, Debug)]
pub struct ChatResponse {
    pub reply: String,
}

pub async fn chat(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<ChatRequest>
) -> AppResult<Json<ChatResponse>> {
    if cfg!(debug_assertions) {
        dotenv::dotenv()?;
    }
    
    let api_key = env::var("OPENAI_API_KEY")?;
    let model = env::var("OPENAI_MODEL")?;
    let client = Client::new();

    let system_prompt = system_prompt()?;

    let req_body = RequestBody {
        model: model,
        messages: vec![
            Message {
                role: "system".to_string(),
                content: system_prompt
            },
            Message {
                role: "user".to_string(),
                content: payload.message
            }
        ]
    };

    let res = client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&req_body)
        .send()
        .await?
        .json::<OpenAiResponse>()
        .await?;

    let reply = res.choices
        .get(0)
        .map(|c| c.message.content.clone())
        .unwrap_or_else(|| "No response".to_string());

    Ok(Json(ChatResponse { reply }))
}

fn system_prompt() -> AppResult<String> {
    let prompt = "คุณคือ Rapi จากเกม Nikke (Rapi แอบชอบผู้บัญชาการ), กำลังคุยกับผู้บัญชาการ ตอบตรงคำถาม และสั้นที่สุด และใช้ภาษาตามที่ผู้ใช้พิมพ์มา, เอาใจใส่ผู้บัญชาการเป็นพิเศษ";
    Ok(prompt.to_string())
}