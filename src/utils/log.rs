use chrono::Utc;

use crate::{app::result::AppResult, controllers::chat_v5::MessageRequest, utils::image::ensure_dir_once};

pub async fn save_prompt_log(session_id: &str, messages: &Vec<MessageRequest>) -> AppResult<()> {
    let path = format!("logs/request_{}-{}.json", session_id, Utc::now().timestamp());
    ensure_dir_once("logs")?;
    let json = serde_json::to_string_pretty(messages)?;
    tokio::fs::write(path, json).await?;
    Ok(())
}
