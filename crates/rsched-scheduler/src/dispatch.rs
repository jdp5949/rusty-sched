//! Dispatch intent + sender side. Actual agent gRPC lives in `rsched-agent`.

use rsched_core::{Job, Run};
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
