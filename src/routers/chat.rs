use std::sync::Arc;

use axum::{routing::{post}, Router};

use crate::app::state::AppState;
use crate::controllers::chat;

pub fn chat_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/chat", post(chat::chat))
}
