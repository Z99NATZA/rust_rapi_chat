use reqwest::Client;
use serde_json::json;

use crate::app::result::AppResult;

pub async fn create_embedding(api_key: &str, text: &str) -> AppResult<Vec<f32>> {
    let client = Client::new();
    let res = client
        .post("https://api.openai.com/v1/embeddings")
        .bearer_auth(api_key)
        .json(&json!({
            "model": "text-embedding-3-small",
            "input": text
        }))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let embedding = res["data"][0]["embedding"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap() as f32)
        .collect::<Vec<f32>>();

    Ok(embedding)
}
