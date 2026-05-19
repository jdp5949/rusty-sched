//! Agent errors.

use thiserror::Error;

/// Errors from job execution.
#[derive(Debug, Error)]
pub enum AgentError {
    /// IO failure spawning the process.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// Run was killed (timeout or external).
    #[error("killed")]
    Killed,
    /// Run dispatched twice with same id.
    #[error("duplicate run id")]
    DuplicateRun,
}
