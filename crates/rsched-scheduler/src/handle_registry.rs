//! Registry of live run kill + signal senders. Allows the API layer to
//! cancel or signal a running job via the existing channels.

use dashmap::DashMap;
use tokio::sync::mpsc;

/// A string run id key.
pub type RunId = String;

struct Handles {
    kill_tx: mpsc::Sender<()>,
    signal_tx: mpsc::Sender<i32>,
}

/// Holds one `kill_tx` + `signal_tx` sender per active run.
///
/// Insertion happens when a run starts; removal happens when it finishes
/// or when `kill()` fires the kill sender.
pub struct HandleRegistry {
    handles: DashMap<RunId, Handles>,
}

impl HandleRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            handles: DashMap::new(),
        }
    }

    /// Register kill + signal senders for `run_id`.
    pub fn insert(&self, run_id: RunId, kill_tx: mpsc::Sender<()>, signal_tx: mpsc::Sender<i32>) {
        self.handles.insert(run_id, Handles { kill_tx, signal_tx });
    }

    /// Remove a completed run from the registry without killing it.
    pub fn remove(&self, run_id: &str) {
        self.handles.remove(run_id);
    }

    /// Send kill signal to `run_id`. Returns `true` if the run was found.
    pub fn kill(&self, run_id: &str) -> bool {
        if let Some((_, h)) = self.handles.remove(run_id) {
            // Best-effort: ignore errors (receiver may already be gone).
            let _ = h.kill_tx.try_send(());
            true
        } else {
            false
        }
    }

    /// Send a unix signal to `run_id`. Returns `true` if the run was found.
    /// Unlike `kill`, the run is NOT removed from the registry — the process
    /// may continue (e.g. SIGHUP for reload).
    pub fn signal(&self, run_id: &str, sig: i32) -> bool {
        if let Some(h) = self.handles.get(run_id) {
            let _ = h.signal_tx.try_send(sig);
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
        let (ktx, mut krx) = mpsc::channel::<()>(1);
        let (stx, _srx) = mpsc::channel::<i32>(1);
        reg.insert("run-1".to_string(), ktx, stx);
        assert!(reg.kill("run-1"));
        assert!(krx.recv().await.is_some());
    }

    #[tokio::test]
    async fn signal_keeps_run_in_registry() {
        let reg = HandleRegistry::new();
        let (ktx, _krx) = mpsc::channel::<()>(1);
        let (stx, mut srx) = mpsc::channel::<i32>(2);
        reg.insert("run-2".to_string(), ktx, stx);
        assert!(reg.signal("run-2", 15));
        assert_eq!(srx.recv().await, Some(15));
        // signal does NOT remove the run.
        assert!(reg.signal("run-2", 1));
    }

    #[tokio::test]
    async fn kill_missing_returns_false() {
        let reg = HandleRegistry::new();
        assert!(!reg.kill("no-such-run"));
        assert!(!reg.signal("no-such-run", 15));
    }

    #[test]
    fn remove_cleans_up() {
        let reg = HandleRegistry::new();
        let (ktx, _krx) = mpsc::channel::<()>(1);
        let (stx, _srx) = mpsc::channel::<i32>(1);
        reg.insert("run-3".to_string(), ktx, stx);
        reg.remove("run-3");
        assert!(!reg.kill("run-3"));
    }
}
