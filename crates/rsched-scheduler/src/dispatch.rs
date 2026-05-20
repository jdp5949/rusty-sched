//! Dispatch intent + sender side. Actual agent gRPC lives in `rsched-agent`.

use rsched_core::{Job, Run, RunState};
use tokio::sync::mpsc;

/// A "please run this" message produced by the tick loop and consumed by the
/// transport layer (gRPC server pushing to agents).
#[derive(Debug, Clone)]
pub struct DispatchIntent {
    /// The job spec at the moment of dispatch.
    pub job: Job,
    /// The run record (already persisted) to associate with the execution.
    pub run: Run,
}

/// Owned sender half of the dispatch queue.
#[derive(Clone)]
pub struct Dispatcher {
    tx: mpsc::Sender<DispatchIntent>,
}

impl Dispatcher {
    /// Construct a bounded queue (default 10k).
    pub fn bounded(cap: usize) -> (Self, mpsc::Receiver<DispatchIntent>) {
        let (tx, rx) = mpsc::channel(cap);
        (Self { tx }, rx)
    }

    /// Try to enqueue without blocking.
    #[allow(clippy::result_large_err)]
    pub fn try_send(&self, intent: DispatchIntent) -> Result<(), DispatchIntent> {
        self.tx.try_send(intent).map_err(|e| match e {
            mpsc::error::TrySendError::Full(i) => i,
            mpsc::error::TrySendError::Closed(i) => i,
        })
    }

    /// Async send (waits if full).
    #[allow(clippy::result_large_err)]
    pub async fn send(
        &self,
        intent: DispatchIntent,
    ) -> Result<(), mpsc::error::SendError<DispatchIntent>> {
        self.tx.send(intent).await
    }
}

/// Returns true when the run should be retried.
///
/// Rules:
/// - state must be `Failed` (not `Killed`, `Lost`, or `Success`).
/// - `run.attempt` must be less than `job.retry.max_attempts`.
pub fn should_retry(job: &Job, run: &Run) -> bool {
    run.state == RunState::Failed && run.attempt < job.retry.max_attempts
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsched_core::{JobBuilder, JobId, Trigger};

    fn make_intent() -> DispatchIntent {
        let job = JobBuilder::new(
            "j",
            "echo",
            Trigger::Cron {
                expr: "* * * * *".into(),
                timezone: None,
            },
        )
        .build()
        .unwrap();
        let run = Run::new(JobId::new(), 1);
        DispatchIntent { job, run }
    }

    use rsched_core::{BackoffKind, RetryPolicy, RunId, RunState};

    fn make_job_with_retry(max_attempts: u32) -> Job {
        JobBuilder::new(
            "j",
            "echo",
            Trigger::Cron {
                expr: "* * * * *".into(),
                timezone: None,
            },
        )
        .retry(RetryPolicy {
            max_attempts,
            backoff: BackoffKind::Fixed { delay_secs: 1 },
        })
        .build()
        .unwrap()
    }

    fn run_with_state(job: &Job, attempt: u32, state: RunState) -> Run {
        let mut r = Run::new(job.id, attempt);
        r.state = state;
        r
    }

    #[test]
    fn should_retry_on_failure_attempt_under_max() {
        let job = make_job_with_retry(3);
        let run = run_with_state(&job, 1, RunState::Failed);
        assert!(should_retry(&job, &run));
        let run2 = run_with_state(&job, 2, RunState::Failed);
        assert!(should_retry(&job, &run2));
    }

    #[test]
    fn should_not_retry_at_max_attempts() {
        let job = make_job_with_retry(3);
        let run = run_with_state(&job, 3, RunState::Failed);
        assert!(!should_retry(&job, &run));
    }

    #[test]
    fn should_not_retry_on_success() {
        let job = make_job_with_retry(3);
        let run = run_with_state(&job, 1, RunState::Success);
        assert!(!should_retry(&job, &run));
    }

    #[test]
    fn should_not_retry_on_killed() {
        let job = make_job_with_retry(3);
        let run = run_with_state(&job, 1, RunState::Killed);
        assert!(!should_retry(&job, &run));
    }

    #[test]
    fn should_not_retry_when_max_attempts_is_one() {
        let job = make_job_with_retry(1);
        let run = run_with_state(&job, 1, RunState::Failed);
        assert!(!should_retry(&job, &run));
    }

    // Suppress unused import warning — RunId used only to satisfy type in future tests
    #[allow(dead_code)]
    fn _use_run_id() -> RunId {
        RunId::new()
    }

    #[tokio::test]
    async fn send_receive() {
        let (tx, mut rx) = Dispatcher::bounded(4);
        tx.send(make_intent()).await.unwrap();
        let got = rx.recv().await.unwrap();
        assert_eq!(got.run.attempt, 1);
    }

    #[tokio::test]
    async fn bounded_full_rejects() {
        let (tx, _rx) = Dispatcher::bounded(1);
        tx.try_send(make_intent()).unwrap();
        let r = tx.try_send(make_intent());
        assert!(r.is_err());
    }
}
