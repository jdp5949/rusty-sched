//! `rsched-raft` — Raft HA scaffolding for rusty-sched.
//!
//! v0.8-alpha ships a compiling skeleton: type config, state-machine wrapper,
//! gRPC network stub, and a [`RsRaft`] handle whose `start()` returns an
//! Ok-stub. The full openraft wiring (real `Raft::new`, replication,
//! snapshots, peer discovery) lands incrementally — single-node default
//! behavior is unchanged.
//!
//! Production users should keep the default single-node mode until the
//! Raft pipeline is marked GA.
#![cfg_attr(not(test), warn(missing_docs))]

pub mod network;
pub mod node;
pub mod store;
pub mod types;

pub use network::{RaftNetworkClient, RaftNetworkFactory};
pub use node::RsRaft;
pub use store::RaftStateMachine;
pub use types::{Node, NodeId, RsRaftReq, RsRaftResp, TypeConfig};
