//! `UpstreamState` implementation backed by `rsched_store::Store`.

use crate::SchedulerError;
use rsched_conditions::UpstreamState;
use rsched_core::RunState;
use rsched_store::Store;
use std::collections::HashMap;

/// Snapshot of job-name → most-recent run state, used for condition evaluation.
pub struct StoreUpstreamState {
    /// Map from job name to (last RunState, last exit code, is_running).
    states: HashMap<String, (RunState, Option<i32>, bool)>,
}

impl StoreUpstreamState {
    /// Build by loading the most recent run for every job in the store.
    pub async fn new(store: Store) -> Result<Self, SchedulerError> {
        let jobs = store.jobs().list().await?;
        let mut states = HashMap::new();
        for job in jobs {
            let runs = store.runs().list_for_job(job.id, 1).await?;
            if let Some(run) = runs.into_iter().next() {
                let is_running = run.state == RunState::Running || run.state == RunState::Queued;
                states.insert(job.name.clone(), (run.state, run.exit_code, is_running));
            }
        }
        Ok(Self { states })
    }
}

impl UpstreamState for StoreUpstreamState {
    fn last_run_state(&self, job_name: &str) -> Option<RunState> {
        self.states.get(job_name).map(|(s, _, _)| *s)
    }

    fn last_exit_code(&self, job_name: &str) -> Option<i32> {
        self.states.get(job_name).and_then(|(_, c, _)| *c)
    }

    fn is_running(&self, job_name: &str) -> bool {
        self.states
            .get(job_name)
            .map(|(_, _, r)| *r)
            .unwrap_or(false)
    }
}
