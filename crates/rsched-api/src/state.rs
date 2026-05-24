//! Shared app state injected into all handlers.

use crate::webhook_dedup::{self, WebhookDedup};
use rsched_scheduler::HandleRegistry;
use rsched_store::Store;
use std::sync::Arc;
use std::time::Duration;

/// Background prune cadence for the webhook-dedup cache.
const DEDUP_PRUNE_INTERVAL: Duration = Duration::from_secs(60);

/// Application state held in axum extensions.
#[derive(Clone)]
pub struct AppState {
    /// Storage backend.
    pub store: Arc<Store>,
    /// Registry of live run kill senders.
    pub registry: Arc<HandleRegistry>,
    /// Replay-dedup cache for the webhook trigger receiver.
    ///
    /// Per-process only; multi-replica dedup is out of scope.
    pub webhook_dedup: Arc<WebhookDedup>,
}

impl AppState {
    /// Construct with an empty handle registry. Spawns a background task
    /// that prunes the webhook-dedup cache every 60s.
    pub fn new(store: Store) -> Self {
        let dedup = Arc::new(WebhookDedup::from_env());
        webhook_dedup::spawn_pruner(Arc::clone(&dedup), DEDUP_PRUNE_INTERVAL);
        Self {
            store: Arc::new(store),
            registry: Arc::new(HandleRegistry::new()),
            webhook_dedup: dedup,
        }
    }

    /// Construct with a provided registry (used by server to share state).
    pub fn with_registry(store: Store, registry: Arc<HandleRegistry>) -> Self {
        let dedup = Arc::new(WebhookDedup::from_env());
        webhook_dedup::spawn_pruner(Arc::clone(&dedup), DEDUP_PRUNE_INTERVAL);
        Self {
            store: Arc::new(store),
            registry,
            webhook_dedup: dedup,
        }
    }
}
