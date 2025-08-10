use std::env;
use std::sync::Arc;

use qdrant_client::Qdrant;

use crate::app::error::AppError;
use crate::app::result::AppResult;
use crate::app::state::AppState;
use crate::routers::api;
use crate::utils::image::ensure_dir_once;
use crate::utils::qdrant::ensure_collection;

pub async fn run() -> AppResult<()> {
    if cfg!(debug_assertions) {
        dotenv::dotenv()?;
    }
    
    let qdrant_client = Qdrant::from_url("http://localhost:6334")
        .build()
        .map_err(|e| AppError::QdrantError(format!("Qdrant connection error: {}", e)))?;

    ensure_collection(&qdrant_client)
        .await
        .map_err(|e| AppError::QdrantError(format!("Failed to create collection: {}", e)))?;

    ensure_dir_once("images/chat")?;
    
    let state = Arc::new(AppState {
        qdrant_client,
    });

    let app = api(state);
    let host = env::var("HOST")?;
    let port = env::var("PORT")?;
    let addr = format!("{}:{}", host, port);

    println!("App running on: {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}