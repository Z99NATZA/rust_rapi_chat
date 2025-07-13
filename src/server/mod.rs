use std::env;
use std::sync::Arc;

use crate::app::result::AppResult;
use crate::app::state::AppState;
use crate::routers::api;

pub async fn run() -> AppResult<()> {
    if cfg!(debug_assertions) {
        dotenv::dotenv()?;
    }
    
    let state = Arc::new(AppState);
    let app = api(state);
    let host = env::var("HOST")?;
    let port = env::var("PORT")?;
    let addr = format!("{}:{}", host, port);

    println!("App running on: {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}