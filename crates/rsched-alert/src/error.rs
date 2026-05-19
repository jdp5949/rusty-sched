//! Alert errors.

use thiserror::Error;

/// Errors from alert delivery.
#[derive(Debug, Error)]
pub enum AlertError {
    /// HTTP failure.
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    /// JSON serialization failure.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// Channel kind not yet supported.
    #[error("unsupported channel: {0}")]
    Unsupported(&'static str),
    /// SMTP send / build failure.
    #[error("smtp: {0}")]
    Smtp(String),
    /// SMTP not configured in env.
    #[error("smtp not configured: set RSCHED_SMTP_HOST/USER/PASS/FROM env vars")]
    SmtpNotConfigured,
}
