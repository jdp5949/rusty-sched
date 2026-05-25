//! rsched-agent — job execution surface.
//!
//! Phase 1 (this crate): `Executor` trait + `LocalExecutor` that runs jobs
//! via `tokio::process` on the same host as the server, streaming stdout +
//! stderr back as chunks. Cross-platform: picks `cmd /C` on Windows,
//! `sh -c` on unix when shell is `Auto`. Enforces hard timeout.
//!
//! Phase 2 (future, M4-gRPC): remote agents over mTLS bidi gRPC will
//! implement the same `Executor` trait.

#![warn(missing_docs)]

mod error;
mod exec;
mod grpc;
mod local;

pub use error::AgentError;
pub use exec::{Executor, LogChunk, LogStream, RunHandle, RunOutcome, Stream};
pub use grpc::GrpcExecutor;
pub use local::LocalExecutor;
