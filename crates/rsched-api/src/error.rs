//! API error type with axum IntoResponse impl.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

/// Error returned from API handlers.
#[derive(Debug, Error)]
pub enum ApiError {
    /// Validation failure.
    #[error("validation: {0}")]
    Validation(String),
    /// Resource not found.
    #[error("not found: {0}")]
    NotFound(String),
    /// Unauthorized (no credentials or invalid).
    #[error("unauthorized")]
    Unauthorized,
    /// Authenticated but lacks required role.
    #[error("forbidden")]
    Forbidden,
    /// Backend storage failure.
    #[error("store: {0}")]
    Store(#[from] rsched_store::StoreError),
    /// Domain validation.
    #[error("core: {0}")]
    Core(#[from] rsched_core::CoreError),
    /// Scheduler failure.
    #[error("scheduler: {0}")]
    Scheduler(#[from] rsched_scheduler::SchedulerError),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            ApiError::Validation(m) => (StatusCode::BAD_REQUEST, m.clone()),
            ApiError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".into()),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "forbidden".into()),
            ApiError::Store(e) => match e {
                rsched_store::StoreError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            },
            ApiError::Core(e) => (StatusCode::BAD_REQUEST, e.to_string()),
            ApiError::Scheduler(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
        (status, Json(json!({"error": msg}))).into_response()
    }
}
