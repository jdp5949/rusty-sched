//! Store errors.

use thiserror::Error;

/// Store-level error.
#[derive(Debug, Error)]
pub enum StoreError {
    /// Underlying SQL error.
    #[error("sql: {0}")]
    Sql(#[from] sqlx::Error),
    /// JSON encode/decode failure.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// Migration failure.
    #[error("migrate: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    /// Row not found by id/name.
    #[error("not found: {0}")]
    NotFound(String),
    /// Domain validation failed before write.
    #[error("validation: {0}")]
    Validation(#[from] rsched_core::CoreError),
}
