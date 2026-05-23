//! `UpstreamState` implementation backed by `rsched_store::Store`.

use crate::SchedulerError;
use chrono::{DateTime, Utc};
use rsched_conditions::UpstreamState;
use rsched_core::RunState;
use rsched_store::Store;
use std::collections::HashMap;
use std::time::Duration;

/// How many recent runs per job we cache for look-back evaluation.
/// Cronicle observes ~1-5 runs per look-back window in practice; 200 is a
/// generous ceiling that keeps the snapshot cheap (1 query per job per tick).
const PER_JOB_HISTORY: i64 = 200;

/// One cached run snapshot used for windowed evaluation.
#[derive(Debug, Clone)]
struct RunSnap {
    state: RunState,
    /// `started_at` if present, otherwise the run's `queued_at` (always set).
    when: DateTime<Utc>,
}

/// Snapshot of job-name → recent run history, used for condition evaluation
/// (including Autosys look-back windows) plus a snapshot of global variables.
pub struct StoreUpstreamState {
    /// Latest (state, exit code, is_running) per job, for fast non-windowed checks.
    latest: HashMap<String, (RunState, Option<i32>, bool)>,
    /// Recent runs (most-recent first) per job, used for windowed counts/checks.
    history: HashMap<String, Vec<RunSnap>>,
    /// Snapshot of all global variables (name → raw value).
    globals: HashMap<String, String>,
    /// Snapshot "now" — windowed checks compare `RunSnap::when >= now - window`.
    now: DateTime<Utc>,
}

impl StoreUpstreamState {
    /// Build by loading recent runs for every job + every global from the store.
    pub async fn new(store: Store) -> Result<Self, SchedulerError> {
        let now = Utc::now();
        let jobs = store.jobs().list().await?;
        let mut latest = HashMap::new();
        let mut history = HashMap::new();
        for job in jobs {
            let runs = store.runs().list_for_job(job.id, PER_JOB_HISTORY).await?;
            if let Some(run) = runs.first() {
                let is_running = run.state == RunState::Running || run.state == RunState::Queued;
                latest.insert(job.name.clone(), (run.state, run.exit_code, is_running));
            }
            let snaps: Vec<RunSnap> = runs
                .iter()
                .map(|r| RunSnap {
                    state: r.state,
                    when: r.started_at.unwrap_or(r.queued_at),
                })
                .collect();
            history.insert(job.name.clone(), snaps);
        }
        let globals = store
            .globals()
            .list()
            .await?
            .into_iter()
            .map(|(n, v, _)| (n, v))
            .collect();
        Ok(Self {
            latest,
            history,
            globals,
            now,
        })
    }

    fn within<F>(&self, job_name: &str, window: Duration, predicate: F) -> Option<u32>
    where
        F: Fn(&RunSnap) -> bool,
    {
        let hist = self.history.get(job_name)?;
        // Convert std::time::Duration to chrono::Duration safely.
        let secs = i64::try_from(window.as_secs()).unwrap_or(i64::MAX);
        let cutoff = self.now - chrono::Duration::seconds(secs);
        let n = hist
            .iter()
            .filter(|r| r.when >= cutoff && predicate(r))
            .count();
        Some(n as u32)
    }
}

impl UpstreamState for StoreUpstreamState {
    fn last_run_state(&self, job_name: &str) -> Option<RunState> {
        self.latest.get(job_name).map(|(s, _, _)| *s)
    }

    fn last_exit_code(&self, job_name: &str) -> Option<i32> {
        self.latest.get(job_name).and_then(|(_, c, _)| *c)
    }

    fn is_running(&self, job_name: &str) -> bool {
        self.latest
            .get(job_name)
            .map(|(_, _, r)| *r)
            .unwrap_or(false)
    }

    fn success_within(&self, job_name: &str, within: Duration) -> Option<bool> {
        let n = self.within(job_name, within, |r| r.state == RunState::Success)?;
        Some(n > 0)
    }

    fn failure_within(&self, job_name: &str, within: Duration) -> Option<bool> {
        let n = self.within(job_name, within, |r| r.state == RunState::Failed)?;
        Some(n > 0)
    }

    fn done_within(&self, job_name: &str, within: Duration) -> Option<bool> {
        let n = self.within(job_name, within, |r| r.state.is_terminal())?;
        Some(n > 0)
    }

    fn count_runs_within(&self, job_name: &str, within: Duration) -> Option<u32> {
        self.within(job_name, within, |_| true)
    }

    fn count_successes_within(&self, job_name: &str, within: Duration) -> Option<u32> {
        self.within(job_name, within, |r| r.state == RunState::Success)
    }

    fn count_failures_within(&self, job_name: &str, within: Duration) -> Option<u32> {
        self.within(job_name, within, |r| r.state == RunState::Failed)
    }

    fn global_value(&self, name: &str) -> Option<bool> {
        let v = self.globals.get(name)?;
        Some(matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "y" | "yes" | "true" | "1"
        ))
    }
}
