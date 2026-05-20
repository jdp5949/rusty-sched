//! `JobSpec` and `PartialJobSpec` — attribute bags from a JIL block.

use serde::{Deserialize, Serialize};

/// Job type as expressed in JIL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JilJobType {
    /// Command job (`c`).
    Command,
    /// Box job (`box`).
    Box,
    /// File-watcher job (`fw`).
    FileWatcher,
}

/// Full set of attributes from an `insert_job` block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobSpec {
    /// Job name.
    pub name: String,
    /// Job type.
    pub job_type: JilJobType,
    /// Shell command to run (command jobs).
    pub command: Option<String>,
    /// Target machine name.
    pub machine: Option<String>,
    /// Job owner (email or user).
    pub owner: Option<String>,
    /// Day-of-week constraint ("mo,tu,we,th,fr" etc.).
    pub days_of_week: Option<String>,
    /// Start times ("02:00,04:00" etc.).
    pub start_times: Option<String>,
    /// Raw Autosys condition expression — resolved by M19.
    pub condition: Option<String>,
    /// Alert on failure (`y`/`n`).
    pub alarm_if_fail: bool,
    /// Number of *retries* (total attempts = n + 1).
    pub n_retrys: u32,
    /// Hard timeout in minutes.
    pub term_run_time: Option<u64>,
    /// Human description.
    pub description: Option<String>,
    /// Stdout file path.
    pub std_out_file: Option<String>,
    /// Stderr file path.
    pub std_err_file: Option<String>,
    /// Box that contains this job.
    pub box_name: Option<String>,
    /// Warnings collected during parse (unknown attributes etc.).
    pub warnings: Vec<String>,
}

/// Partial attribute set from an `update_job` block.
/// All fields are `Option`; only the ones present in the JIL are `Some`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PartialJobSpec {
    /// Shell command.
    pub command: Option<String>,
    /// Target machine.
    pub machine: Option<String>,
    /// Owner.
    pub owner: Option<String>,
    /// Days of week.
    pub days_of_week: Option<String>,
    /// Start times.
    pub start_times: Option<String>,
    /// Raw condition.
    pub condition: Option<String>,
    /// Alarm on failure.
    pub alarm_if_fail: Option<bool>,
    /// Retry count.
    pub n_retrys: Option<u32>,
    /// Hard timeout in minutes.
    pub term_run_time: Option<u64>,
    /// Description.
    pub description: Option<String>,
    /// Stdout path.
    pub std_out_file: Option<String>,
    /// Stderr path.
    pub std_err_file: Option<String>,
    /// Box membership.
    pub box_name: Option<String>,
    /// Warnings.
    pub warnings: Vec<String>,
}
