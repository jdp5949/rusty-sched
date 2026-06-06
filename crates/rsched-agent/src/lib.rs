//! rsched-agent — job execution surface.
//!
//! Phase 1 (this crate): `Executor` trait + `LocalExecutor` that runs jobs
//! via `tokio::process` on the same host as the server, streaming stdout +
//! stderr back as chunks. Cross-platform: picks `cmd /C` on Windows,
//! `sh -c` on unix when shell is `Auto`. Enforces hard timeout.
//!
//! Remote agents (mTLS gRPC) were removed in v2 (Cronicle-model
//! simplification). Execution is local; the `Executor` trait remains the
//! seam if a simpler HTTP satellite executor is added later.

#![warn(missing_docs)]

mod error;
mod exec;
mod local;

pub use error::AgentError;
pub use exec::{Executor, LogChunk, LogStream, RunHandle, RunOutcome, Stream};
pub use local::LocalExecutor;
