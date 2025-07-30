use qdrant_client::Qdrant;

#[derive(Clone)]
pub struct AppState {
    pub qdrant_client: Qdrant,
}