use axum::{Router, http::{Method, header, HeaderValue}};
use tower_http::cors::CorsLayer;
use std::sync::Arc;
use crate::app::state::AppState;
use axum::routing::{post};
use crate::controllers::chat_v5;

pub fn api(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(HeaderValue::from_static("http://localhost:3000"))
        // .allow_origin(HeaderValue::from_static("https://z99natza.netlify.app"))
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
        .allow_credentials(true);

    Router::<Arc<AppState>>::new()
        .route("/api/chat", post(chat_v5::chat))
        .layer(cors)
        .with_state(state)
} 