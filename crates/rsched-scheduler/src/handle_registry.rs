//! Registry of live run kill senders. Allows the API layer to cancel
//! a running job by sending on the existing `kill_tx` channel.

use dashmap::DashMap;
use tokio::sync::mpsc;

/// A string run id key.
pub type RunId = String;

/// Holds one `kill_tx` sender per active run.
///
/// Insertion happens when a run starts; removal happens when it finishes
/// or when `kill()` fires the sender.
pub struct HandleRegistry {
    handles: DashMap<RunId, mpsc::Sender<()>>,
}

impl HandleRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            handles: DashMap::new(),
        }
    }

    /// Register a kill sender for `run_id`.
    pub fn insert(&self, run_id: RunId, kill_tx: mpsc::Sender<()>) {
        self.handles.insert(run_id, kill_tx);
    }

    /// Remove a completed run from the registry without killing it.
    pub fn remove(&self, run_id: &str) {
        self.handles.remove(run_id);
    }

    /// Send kill signal to `run_id`. Returns `true` if the run was found.
    pub fn kill(&self, run_id: &str) -> bool {
        if let Some((_, tx)) = self.handles.remove(run_id) {
            // Best-effort: ignore errors (receiver may already be gone).
            let _ = tx.try_send(());
            true
        } else {
            false
        }
    }
}

impl Default for HandleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn kill_returns_true_and_sends() {
        let reg = HandleRegistry::new();
        let (tx, mut rx) = mpsc::channel::<()>(1);
        reg.insert("run-1".to_string(), tx);
        assert!(reg.kill("run-1"));
        assert!(rx.recv().await.is_some());
    }

    #[tokio::test]
    async fn kill_missing_returns_false() {
        let reg = HandleRegistry::new();
        assert!(!reg.kill("no-such-run"));
    }

    #[test]
    fn remove_cleans_up() {
        let reg = HandleRegistry::new();
        let (tx, _rx) = mpsc::channel::<()>(1);
        reg.insert("run-2".to_string(), tx);
        reg.remove("run-2");
        assert!(!reg.kill("run-2"));
    }
}
