//! Raft state-machine skeleton.
//!
//! In the full multi-node design, every committed [`RsRaftReq`] entry is
//! `apply()`-ed in log order against a [`rsched_store::Store`]. This file
//! lays out the surface; the openraft `RaftStateMachine` trait impl is
//! deferred to a later milestone.
//!
//! The single-node default path does NOT route through this state machine —
//! today, repos are called directly from the API / scheduler.

use std::sync::Arc;

use rsched_store::Store;
use tracing::warn;

use crate::types::{RsRaftReq, RsRaftResp};

/// A thin wrapper around a [`rsched_store::Store`] that knows how to apply
/// a committed [`RsRaftReq`] log entry against the underlying SQL repos.
///
/// Not yet wired to openraft — this is the skeleton that v0.8 will build on.
#[derive(Clone)]
pub struct RaftStateMachine {
    store: Arc<Store>,
}

impl RaftStateMachine {
    /// Construct a state machine over the given shared store.
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    /// Borrow the underlying store (handy for tests + future snapshot code).
    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    /// Apply a committed log entry against the underlying store.
    ///
    /// Today each arm logs and returns Ok — wiring through to the repo layer
    /// lands incrementally with the rest of the Raft pipeline. The shape of
    /// the dispatch is deliberately exhaustive so adding a new [`RsRaftReq`]
    /// variant forces a compile error here.
    pub async fn apply(&self, req: RsRaftReq) -> RsRaftResp {
        match req {
            RsRaftReq::InsertJob(job) => {
                warn!(job = %job.name, "raft apply: InsertJob (stub)");
                Ok(())
            }
            RsRaftReq::UpdateJob(job) => {
                warn!(job = %job.name, "raft apply: UpdateJob (stub)");
                Ok(())
            }
            RsRaftReq::DeleteJob(id) => {
                warn!(%id, "raft apply: DeleteJob (stub)");
                Ok(())
            }
            RsRaftReq::SetGlobal { name, value } => {
                warn!(%name, %value, "raft apply: SetGlobal (stub)");
                Ok(())
            }
            RsRaftReq::InsertRun(run) => {
                warn!(run_id = %run.id, "raft apply: InsertRun (stub)");
                Ok(())
            }
            RsRaftReq::UpdateRun { run_id, state } => {
                warn!(%run_id, ?state, "raft apply: UpdateRun (stub)");
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsched_core::{JobBuilder, Trigger};

    async fn fresh_sm() -> RaftStateMachine {
        let pool = rsched_store::open_memory().await.unwrap();
        let store = Store::new(pool);
        store.migrate().await.unwrap();
        RaftStateMachine::new(Arc::new(store))
    }

    #[tokio::test]
    async fn apply_set_global_returns_ok() {
        let sm = fresh_sm().await;
        let r = sm
            .apply(RsRaftReq::SetGlobal {
                name: "k".into(),
                value: "v".into(),
            })
            .await;
        assert!(r.is_ok());
    }

    #[tokio::test]
    async fn apply_insert_job_returns_ok() {
        let sm = fresh_sm().await;
        let job = JobBuilder::new(
            "j1",
            "echo",
            Trigger::Cron {
                expr: "* * * * *".into(),
                timezone: None,
            },
        )
        .build()
        .unwrap();
        let r = sm.apply(RsRaftReq::InsertJob(job)).await;
        assert!(r.is_ok());
    }
}
