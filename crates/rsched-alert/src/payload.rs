//! Outgoing alert payload.

use chrono::{DateTime, Utc};
use rsched_core::{AlertEvent, JobId, RunId, RunState};
use serde::{Deserialize, Serialize};

/// Payload sent on every alert. Stable JSON shape consumed by webhook
/// recipients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertPayload {
    /// Event that triggered the alert.
    pub event: AlertEvent,
    /// Job id (ULID).
    pub job_id: JobId,
    /// Job name (human-friendly).
    pub job_name: String,
    /// Run id (ULID).
    pub run_id: RunId,
    /// Final or current run state.
    pub state: RunState,
    /// Exit code, if any.
    pub exit_code: Option<i32>,
    /// Attempt number.
    pub attempt: u32,
    /// When the run started.
    pub started_at: Option<DateTime<Utc>>,
    /// When the run finished (None if still running).
    pub finished_at: Option<DateTime<Utc>>,
    /// Human-readable host (server hostname or "scheduler").
    pub host: String,
    /// Optional free-form message (e.g. "SLA exceeded 30m").
    pub message: Option<String>,
}
