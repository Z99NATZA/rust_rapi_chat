#![allow(dead_code)]

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::Json;
use axum_extra::extract::multipart::MultipartError;
use thiserror::Error;
use serde_json::json;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Env variable error: {0}")]
    EnvVarError(#[from] std::env::VarError),

    #[error("Dotenv loading error: {0}")]
    DotenvError(#[from] dotenv::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("HTTP request error: {0}")]
    ReqwestError(#[from] reqwest::Error),

    #[error("Something went wrong: {0}")]
    InternalError(String),

    #[error("NotFound: {0}")]
    NotFound(String),

    #[error("JSON decode error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Multipart error: {0}")]
    MultipartError(#[from] MultipartError),
    
    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Qdrant connection error: {0}")]
    QdrantError(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message): (StatusCode, String) = match self {
            AppError::EnvVarError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            AppError::DotenvError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            AppError::InternalError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e),
            AppError::IoError(e) => (StatusCode::BAD_GATEWAY, e.to_string()),
            AppError::ReqwestError(e) => (StatusCode::BAD_GATEWAY, e.to_string()),
            AppError::NotFound(e) => (StatusCode::NOT_FOUND, e),
            AppError::JsonError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            AppError::MultipartError(e) => (StatusCode::BAD_REQUEST, e.to_string()),
            AppError::BadRequest(e) => (StatusCode::BAD_REQUEST, e),
            AppError::QdrantError(e) => (StatusCode::BAD_GATEWAY, e.to_string()),
        };

        let body = Json(json!({
            "error": message
        }));

        (status, body).into_response()
    }
}