//! Shared app state injected into all handlers.

use rsched_store::Store;
use std::sync::Arc;

/// Application state held in axum extensions.
#[derive(Clone)]
pub struct AppState {
    /// Storage backend.
    pub store: Arc<Store>,
}

impl AppState {
    /// Construct.
    pub fn new(store: Store) -> Self {
        Self {
            store: Arc::new(store),
        }
    }
}
