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
    /// Calendar to *exclude* (job blocked when this calendar allows).
    pub exclude_calendar: Option<String>,
    /// "HH:MM,HH:MM,..." times the job must start by.
    pub must_start_times: Option<String>,
    /// "HH:MM,HH:MM,..." times the job must complete by.
    pub must_complete_times: Option<String>,
    /// "100,101,..." exit codes that are always Failure.
    pub fail_codes: Option<String>,
    /// Maximum exit code considered Success (default 0).
    pub max_exit_success: Option<i32>,
    /// Exit code mapped to Conditional outcome.
    pub condition_code: Option<i32>,
    /// (Box only) condition expression for box success.
    pub box_success: Option<String>,
    /// (Box only) condition expression for box failure.
    pub box_failure: Option<String>,
    /// (Box only) terminate children on box failure.
    pub box_terminator: Option<bool>,
    /// (Box only) propagate kill to children when box fails.
    pub job_terminator: Option<bool>,
    /// (Box only) auto-hold children when box transitions to Running.
    pub auto_hold: Option<bool>,
    /// Resource claims in raw Autosys form: `"resA(3),resB(1)"` or `"resA"` (=1 unit).
    pub resources: Option<String>,
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
    /// Exclude-calendar.
    pub exclude_calendar: Option<String>,
    /// Must-start times.
    pub must_start_times: Option<String>,
    /// Must-complete times.
    pub must_complete_times: Option<String>,
    /// Fail codes CSV.
    pub fail_codes: Option<String>,
    /// Max exit success.
    pub max_exit_success: Option<i32>,
    /// Condition code.
    pub condition_code: Option<i32>,
    /// Box success expr.
    pub box_success: Option<String>,
    /// Box failure expr.
    pub box_failure: Option<String>,
    /// Box terminator.
    pub box_terminator: Option<bool>,
    /// Job terminator.
    pub job_terminator: Option<bool>,
    /// Auto hold.
    pub auto_hold: Option<bool>,
    /// Resource claims.
    pub resources: Option<String>,
    /// Warnings.
    pub warnings: Vec<String>,
}

impl JobSpec {
    /// Build a new spec for a command job with all new fields defaulted to None.
    /// Convenience used in tests.
    pub fn empty(name: impl Into<String>, job_type: JilJobType) -> Self {
        Self {
            name: name.into(),
            job_type,
            command: None,
            machine: None,
            owner: None,
            days_of_week: None,
            start_times: None,
            condition: None,
            alarm_if_fail: false,
            n_retrys: 0,
            term_run_time: None,
            description: None,
            std_out_file: None,
            std_err_file: None,
            box_name: None,
            exclude_calendar: None,
            must_start_times: None,
            must_complete_times: None,
            fail_codes: None,
            max_exit_success: None,
            condition_code: None,
            box_success: None,
            box_failure: None,
            box_terminator: None,
            job_terminator: None,
            auto_hold: None,
            resources: None,
            warnings: Vec::new(),
        }
    }
}
