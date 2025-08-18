use std::{env, net::SocketAddr, sync::Arc, time::Duration};
use tokio::signal;
use qdrant_client::Qdrant;

use crate::app::error::AppError;
use crate::app::result::AppResult;
use crate::app::state::AppState;
use crate::routers::api;
use crate::utils::image::ensure_dir_once;
use crate::utils::qdrant::ensure_collection;

pub async fn run() -> AppResult<()> {
    // โหลด .env ตอน dev เท่านั้น
    if cfg!(debug_assertions) {
        dotenv::dotenv()?;
    }

    // -----------------------
    // Qdrant client (จาก ENV)
    // -----------------------
    let qdrant_url = env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6334".into());
    let qdrant_api_key = env::var("QDRANT_API_KEY").ok();

    let qdrant_client = match qdrant_api_key {
        Some(k) if !k.is_empty() => {
            Qdrant::from_url(&qdrant_url)
                .api_key(k)
                .build()
                .map_err(|e| AppError::QdrantError(format!("Qdrant connection error: {e}")))?
        }
        _ => {
            Qdrant::from_url(&qdrant_url)
                .build()
                .map_err(|e| AppError::QdrantError(format!("Qdrant connection error: {e}")))?
        }
    };

    ensure_collection(&qdrant_client)
        .await
        .map_err(|e| AppError::QdrantError(format!("Failed to create collection: {e}")))?;

    // เตรียมโฟลเดอร์สำหรับเก็บรูปอัปโหลด
    ensure_dir_once("images/chat")?;

    // -----------------------
    // OpenAI config
    // -----------------------
    let openai_key = env::var("OPENAI_API_KEY")?;
    let openai_model = env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o".to_string());

    // -----------------------
    // Reused HTTP client
    // หมายเหตุ: ถ้ามีสตรีม ให้ตั้ง timeout แบบ per-request .timeout(None)
    // -----------------------
    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(60))
        .pool_max_idle_per_host(10)
        .tcp_keepalive(Some(Duration::from_secs(30)))
        .build()?; // reqwest::Error -> AppError::ReqwestError via `?`

    // -----------------------
    // Shared AppState
    // -----------------------
    let state = Arc::new(AppState {
        qdrant_client,
        http,
        openai_key,
        openai_model,
    });
  
    // -----------------------
    // Router + Server
    // -----------------------
    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());
    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .map_err(|e| AppError::BadRequest(format!("Invalid HOST/PORT: {e}")))?;

    let app = api(state);
    println!("App running on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = signal::ctrl_c().await;
            eprintln!("Shutting down…");
        })
        .await?;

    Ok(())
}
