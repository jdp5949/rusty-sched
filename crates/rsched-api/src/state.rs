//! Shared app state injected into all handlers.

use rsched_scheduler::HandleRegistry;
use rsched_store::Store;
use std::sync::Arc;

/// Application state held in axum extensions.
#[derive(Clone)]
pub struct AppState {
    /// Storage backend.
    pub store: Arc<Store>,
    /// Registry of live run kill senders.
    pub registry: Arc<HandleRegistry>,
}

impl AppState {
    /// Construct with an empty handle registry.
    pub fn new(store: Store) -> Self {
        Self {
            store: Arc::new(store),
            registry: Arc::new(HandleRegistry::new()),
        }
    }

    /// Construct with a provided registry (used by server to share state).
    pub fn with_registry(store: Store, registry: Arc<HandleRegistry>) -> Self {
        Self {
            store: Arc::new(store),
            registry,
        }
    }
}
