//! Run = a single execution attempt of a job.

use crate::{AgentId, JobId, RunId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// State machine for a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    /// Created, awaiting dispatch.
    Queued,
    /// Sent to agent, running.
    Running,
    /// Exited 0.
    Success,
    /// Exited non-zero or killed.
    Failed,
    /// Manually killed.
    Killed,
    /// Calendar/dep skipped without running.
    Skipped,
    /// Agent disappeared mid-run.
    Lost,
}

impl RunState {
    /// True if this is a terminal state (won't change).
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            RunState::Success
                | RunState::Failed
                | RunState::Killed
                | RunState::Skipped
                | RunState::Lost
        )
    }
}

/// A single attempt to run a job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Run {
    /// Identifier.
    pub id: RunId,
    /// Job this is an attempt of.
    pub job_id: JobId,
    /// Agent that ran it (None until dispatched).
    pub agent_id: Option<AgentId>,
    /// Current state.
    pub state: RunState,
    /// Attempt number (1-based).
    pub attempt: u32,
    /// When it was queued.
    pub queued_at: DateTime<Utc>,
    /// When the agent started executing.
    pub started_at: Option<DateTime<Utc>>,
    /// When the run finished.
    pub finished_at: Option<DateTime<Utc>>,
    /// Exit code, if any.
    pub exit_code: Option<i32>,
    /// For dep-triggered runs, the upstream run IDs that satisfied it.
    pub parent_run_ids: Vec<RunId>,
    /// Whether log was truncated for size.
    pub log_truncated: bool,
    /// Bytes of log captured.
    pub log_bytes: u64,
    /// Peak resident set size (bytes) reported by getrusage at exit. Unix only.
    #[serde(default)]
    pub peak_rss_bytes: Option<u64>,
    /// User-mode CPU seconds at exit (getrusage). Unix only.
    #[serde(default)]
    pub cpu_user_secs: Option<f64>,
    /// Kernel-mode CPU seconds at exit (getrusage). Unix only.
    #[serde(default)]
    pub cpu_sys_secs: Option<f64>,
}

impl Run {
    /// Make a new queued run.
    pub fn new(job_id: JobId, attempt: u32) -> Self {
        Self {
            id: RunId::new(),
            job_id,
            agent_id: None,
            state: RunState::Queued,
            attempt,
            queued_at: Utc::now(),
            started_at: None,
            finished_at: None,
            exit_code: None,
            parent_run_ids: Vec::new(),
            log_truncated: false,
            log_bytes: 0,
            peak_rss_bytes: None,
            cpu_user_secs: None,
            cpu_sys_secs: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_states() {
        assert!(RunState::Success.is_terminal());
        assert!(RunState::Failed.is_terminal());
        assert!(!RunState::Queued.is_terminal());
        assert!(!RunState::Running.is_terminal());
    }

    #[test]
    fn new_run_is_queued() {
        let r = Run::new(JobId::new(), 1);
        assert_eq!(r.state, RunState::Queued);
        assert_eq!(r.attempt, 1);
        assert!(r.started_at.is_none());
    }
}
