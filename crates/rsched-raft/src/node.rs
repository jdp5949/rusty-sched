//! Top-level Raft handle.
//!
//! [`RsRaft`] is what `rsched-bin` holds when `--peers` is set. v0.8-alpha
//! ships a *stub* that wires the pieces together (state machine, network
//! factory, recorded peer set) but does not actually boot an openraft
//! instance. `start()` returns Ok so the binary can keep running in
//! single-node mode while the rest of the Raft pipeline lands.

use std::sync::Arc;

use anyhow::Result;
use rsched_store::Store;
use tracing::info;

use crate::network::RaftNetworkFactory;
use crate::store::RaftStateMachine;
use crate::types::{Node, NodeId};

/// Handle to a running (or stubbed) Raft node.
pub struct RsRaft {
    node_id: NodeId,
    bind: String,
    peers: Vec<Node>,
    state_machine: RaftStateMachine,
    network: RaftNetworkFactory,
}

impl RsRaft {
    /// Boot a Raft node. v0.8-alpha returns an Ok stub — the real openraft
    /// `Raft::new` call lands in a follow-up.
    pub async fn start(
        node_id: NodeId,
        store: Arc<Store>,
        peers: Vec<Node>,
        bind: impl Into<String>,
    ) -> Result<Self> {
        let bind = bind.into();
        let network = RaftNetworkFactory::new();
        for p in &peers {
            network.register_peer(p.clone());
        }
        let state_machine = RaftStateMachine::new(store);
        info!(
            node_id,
            peer_count = peers.len(),
            %bind,
            "raft mode active (stub)"
        );
        Ok(Self {
            node_id,
            bind,
            peers,
            state_machine,
            network,
        })
    }

    /// This node's stable id.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Bind address used for the Raft gRPC listener.
    pub fn bind(&self) -> &str {
        &self.bind
    }

    /// Snapshot of the configured peer set.
    pub fn peers(&self) -> &[Node] {
        &self.peers
    }

    /// Borrow the state-machine wrapper.
    pub fn state_machine(&self) -> &RaftStateMachine {
        &self.state_machine
    }

    /// Borrow the network factory.
    pub fn network(&self) -> &RaftNetworkFactory {
        &self.network
    }

    /// True once a leader has been elected. Always false in the stub.
    pub fn is_leader(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fresh_store() -> Arc<Store> {
        let pool = rsched_store::open_memory().await.unwrap();
        let store = Store::new(pool);
        store.migrate().await.unwrap();
        Arc::new(store)
    }

    #[tokio::test]
    async fn start_returns_ok_stub_single_node() {
        let store = fresh_store().await;
        let r = RsRaft::start(1, store, vec![], "127.0.0.1:9100")
            .await
            .unwrap();
        assert_eq!(r.node_id(), 1);
        assert_eq!(r.bind(), "127.0.0.1:9100");
        assert!(r.peers().is_empty());
        assert!(!r.is_leader());
    }

    #[tokio::test]
    async fn start_records_peers() {
        let store = fresh_store().await;
        let peers = vec![Node::new(2, "10.0.0.2:9100"), Node::new(3, "10.0.0.3:9100")];
        let r = RsRaft::start(1, store, peers.clone(), "0.0.0.0:9100")
            .await
            .unwrap();
        assert_eq!(r.peers().len(), 2);
        assert_eq!(r.network().len(), 2);
    }
}
