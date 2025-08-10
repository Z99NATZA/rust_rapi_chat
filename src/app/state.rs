use qdrant_client::Qdrant;

#[derive(Clone)]
pub struct AppState {
    pub qdrant_client: Qdrant,
    pub http: reqwest::Client,
    pub openai_key: String,
    pub openai_model: String,
}