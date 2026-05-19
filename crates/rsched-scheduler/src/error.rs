//! Scheduler errors.

use thiserror::Error;

/// Scheduler-level error.
#[derive(Debug, Error)]
#[allow(clippy::result_large_err)]
pub enum SchedulerError {
    /// Storage failure.
    #[error("store: {0}")]
    Store(#[from] rsched_store::StoreError),
    /// Core validation.
    #[error("core: {0}")]
    Core(#[from] rsched_core::CoreError),
    /// Bad cron expression.
    #[error("cron: {0}")]
    Cron(String),
    /// DAG cycle detected.
    #[error("dependency cycle through job {0}")]
    Cycle(String),
}
