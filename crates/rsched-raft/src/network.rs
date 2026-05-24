//! Raft network skeleton — gRPC peer transport stub.
//!
//! The full implementation will speak the openraft `RaftNetwork` trait over
//! tonic, reusing the agent-channel TLS bits already present in
//! `rsched-agent`. For v0.8-alpha this file only sketches the entrypoints
//! so callers can wire them through; every method logs and returns
//! `unimplemented!()`-style errors.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use tracing::warn;

use crate::types::{Node, NodeId};

/// Factory that produces per-peer network clients keyed by [`NodeId`].
///
/// In the full impl this owns a tonic channel pool and reuses connections
/// across replication batches.
#[derive(Clone, Default)]
pub struct RaftNetworkFactory {
    peers: Arc<Mutex<HashMap<NodeId, Node>>>,
}

impl RaftNetworkFactory {
    /// Construct an empty factory — peers are registered later via
    /// [`Self::register_peer`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a peer descriptor so future RPCs can dial it.
    pub fn register_peer(&self, node: Node) {
        self.peers.lock().insert(node.id, node);
    }

    /// Number of registered peers.
    pub fn len(&self) -> usize {
        self.peers.lock().len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.peers.lock().is_empty()
    }

    /// Snapshot the registered peers (used by membership queries).
    pub fn peers(&self) -> Vec<Node> {
        self.peers.lock().values().cloned().collect()
    }

    /// Build a per-peer client for `target`.
    ///
    /// Stub: returns [`RaftNetworkClient`] which always errors on
    /// `unimplemented`.
    pub fn client(&self, target: NodeId) -> RaftNetworkClient {
        let node = self.peers.lock().get(&target).cloned();
        RaftNetworkClient { target, node }
    }
}

/// Per-peer Raft RPC client stub. Every method logs and returns an error —
/// no real RPC is issued.
pub struct RaftNetworkClient {
    target: NodeId,
    node: Option<Node>,
}

impl RaftNetworkClient {
    /// Send an AppendEntries RPC — stub.
    pub async fn append_entries(&self) -> Result<(), String> {
        warn!(target = self.target, addr = ?self.node.as_ref().map(|n| &n.addr),
              "raft net: append_entries called (stub, unimplemented)");
        Err("raft network: append_entries not implemented".into())
    }

    /// Send an InstallSnapshot RPC — stub.
    pub async fn install_snapshot(&self) -> Result<(), String> {
        warn!(
            target = self.target,
            "raft net: install_snapshot called (stub, unimplemented)"
        );
        Err("raft network: install_snapshot not implemented".into())
    }

    /// Send a Vote RPC — stub.
    pub async fn vote(&self) -> Result<(), String> {
        warn!(
            target = self.target,
            "raft net: vote called (stub, unimplemented)"
        );
        Err("raft network: vote not implemented".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_registers_and_lists_peers() {
        let f = RaftNetworkFactory::new();
        assert!(f.is_empty());
        f.register_peer(Node::new(2, "10.0.0.2:9000"));
        f.register_peer(Node::new(3, "10.0.0.3:9000"));
        assert_eq!(f.len(), 2);
        let mut ids: Vec<_> = f.peers().into_iter().map(|p| p.id).collect();
        ids.sort();
        assert_eq!(ids, vec![2, 3]);
    }

    #[tokio::test]
    async fn client_stubs_return_unimplemented() {
        let f = RaftNetworkFactory::new();
        f.register_peer(Node::new(5, "x:1"));
        let c = f.client(5);
        assert!(c.append_entries().await.is_err());
        assert!(c.vote().await.is_err());
        assert!(c.install_snapshot().await.is_err());
    }
}
