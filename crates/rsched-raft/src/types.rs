//! Raft type definitions for `rsched-raft`.
//!
//! Defines the [`TypeConfig`] expected by `openraft`, the application-level
//! request/response payloads (`RsRaftReq`, `RsRaftResp`), and the cluster
//! [`Node`] / [`NodeId`] types. The state machine that consumes these is in
//! [`crate::store`].
//!
//! This is the v0.8-alpha skeleton — `TypeConfig` is declared via
//! `openraft::declare_raft_types!`, but full storage / network plumbing is
//! deferred to a later milestone.
//!
//! See `docs/specs/` for the rationale.

use serde::{Deserialize, Serialize};

use rsched_core::{Job, JobId, Run, RunId, RunState};

/// Stable cluster node identifier — a small monotonic integer assigned at
/// `cluster join` time. `0` is reserved for "unset".
pub type NodeId = u64;

/// Addressable peer descriptor stored in the cluster membership.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    /// Stable node id.
    pub id: NodeId,
    /// `host:port` (gRPC) — used by the network layer to dial this peer.
    pub addr: String,
}

impl Node {
    /// Construct a new node descriptor.
    pub fn new(id: NodeId, addr: impl Into<String>) -> Self {
        Self {
            id,
            addr: addr.into(),
        }
    }
}

/// Replicated mutation requests — every write that today goes directly to
/// SQLite will tomorrow be funnelled through Raft as one of these variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RsRaftReq {
    /// Insert a new job definition.
    InsertJob(Job),
    /// Replace the mutable fields of an existing job.
    UpdateJob(Job),
    /// Delete a job by id.
    DeleteJob(JobId),
    /// Set a global variable used by `value(name)` conditions.
    SetGlobal {
        /// Variable name.
        name: String,
        /// String value.
        value: String,
    },
    /// Persist a freshly enqueued run.
    InsertRun(Run),
    /// Update a run row — state transitions, exit codes, log byte counts.
    UpdateRun {
        /// Run id being updated.
        run_id: RunId,
        /// New run state (e.g. running, success, failed).
        state: RunState,
    },
}

/// Application reply for a replicated request — `Ok(())` on success or
/// `Err(message)` on a state-machine-level failure.
pub type RsRaftResp = Result<(), String>;

openraft::declare_raft_types!(
    /// Static type configuration for the rusty-sched Raft cluster.
    pub TypeConfig:
        D = RsRaftReq,
        R = RsRaftResp,
        NodeId = NodeId,
        Node = Node,
        Entry = openraft::Entry<TypeConfig>,
        SnapshotData = std::io::Cursor<Vec<u8>>,
        AsyncRuntime = openraft::TokioRuntime,
);

#[cfg(test)]
mod tests {
    use super::*;
    use rsched_core::{JobBuilder, Trigger};

    fn sample_job(name: &str) -> Job {
        JobBuilder::new(
            name,
            "echo hi",
            Trigger::Cron {
                expr: "* * * * *".into(),
                timezone: None,
            },
        )
        .build()
        .unwrap()
    }

    #[test]
    fn node_round_trips_through_json() {
        let n = Node::new(7, "10.0.0.5:9000");
        let j = serde_json::to_string(&n).unwrap();
        let back: Node = serde_json::from_str(&j).unwrap();
        assert_eq!(n, back);
        assert_eq!(back.id, 7);
        assert_eq!(back.addr, "10.0.0.5:9000");
    }

    #[test]
    fn rsraft_req_set_global_round_trips() {
        let req = RsRaftReq::SetGlobal {
            name: "FX_RATE".into(),
            value: "1.07".into(),
        };
        let j = serde_json::to_string(&req).unwrap();
        let back: RsRaftReq = serde_json::from_str(&j).unwrap();
        assert_eq!(req, back);
    }

    #[test]
    fn rsraft_req_insert_job_round_trips() {
        let job = sample_job("etl-nightly");
        let req = RsRaftReq::InsertJob(job.clone());
        let bytes = serde_json::to_vec(&req).unwrap();
        let back: RsRaftReq = serde_json::from_slice(&bytes).unwrap();
        match back {
            RsRaftReq::InsertJob(j) => assert_eq!(j.name, job.name),
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn rsraft_resp_ok_and_err() {
        let ok: RsRaftResp = Ok(());
        let err: RsRaftResp = Err("nope".into());
        assert!(ok.is_ok());
        assert_eq!(err, Err("nope".to_string()));
    }
}
