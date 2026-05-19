//! Executor trait + supporting types.

use crate::AgentError;
use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use rsched_core::{Job, RunId};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

/// Which stream a log chunk came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
}

/// One chunk of streamed log output.
#[derive(Debug, Clone)]
pub struct LogChunk {
    /// Source stream.
    pub stream: Stream,
    /// Wall-clock timestamp.
    pub ts: DateTime<Utc>,
    /// Raw bytes (may include partial UTF-8).
    pub bytes: Bytes,
}

/// Async stream of log chunks.
pub type LogStream = ReceiverStream<LogChunk>;

/// Final outcome of a single run.
#[derive(Debug, Clone)]
pub struct RunOutcome {
    /// Exit code (None if killed by signal / terminated).
    pub exit_code: Option<i32>,
    /// True if killed by timeout enforcement.
    pub timed_out: bool,
    /// Total bytes captured across stdout + stderr.
    pub log_bytes: u64,
    /// When the process exited (UTC).
    pub finished_at: DateTime<Utc>,
}

/// Handle returned by [`Executor::dispatch`]. Drop to detach; explicit
/// `kill` to stop the run early.
pub struct RunHandle {
    /// Run id (echoed for tracing).
    pub run_id: RunId,
    /// Receiver side for log chunks.
    pub logs: LogStream,
    /// Future that resolves when the run completes.
    pub outcome: tokio::task::JoinHandle<Result<RunOutcome, AgentError>>,
    /// Send `()` here to request graceful termination.
    pub kill_tx: mpsc::Sender<()>,
}

/// Abstraction over "where jobs run."
///
/// Implemented by [`crate::LocalExecutor`] (in-process) and, in the future,
/// by a gRPC remote-agent client.
#[async_trait]
pub trait Executor: Send + Sync {
    /// Start `job` as a new run; returns a handle the caller can use to
    /// stream logs and await the outcome.
    async fn dispatch(&self, run_id: RunId, job: Job) -> Result<RunHandle, AgentError>;
}
