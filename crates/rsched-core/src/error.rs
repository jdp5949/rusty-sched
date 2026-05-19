//! Errors raised by core type construction / validation.

use thiserror::Error;

/// Errors for invalid domain values.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoreError {
    /// A name violated the allowed character set / length rules.
    #[error("invalid name {0:?}: {1}")]
    InvalidName(String, &'static str),

    /// A cron expression failed to parse.
    #[error("invalid cron expression {0:?}: {1}")]
    InvalidCron(String, String),

    /// A timezone string was not recognized.
    #[error("unknown timezone {0:?}")]
    UnknownTimezone(String),

    /// A retry policy had nonsensical values.
    #[error("invalid retry policy: {0}")]
    InvalidRetry(&'static str),

    /// A calendar rule was malformed.
    #[error("invalid calendar rule: {0}")]
    InvalidCalendar(&'static str),
}
