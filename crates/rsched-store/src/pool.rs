//! Connection pool helpers — supports SQLite and Postgres via `sqlx::Any`.

use crate::StoreError;
use sqlx::any::install_default_drivers;
use sqlx::AnyPool;
use std::time::Duration;

/// Install all Any-driver backends (call once before opening Any pools).
pub fn init_drivers() {
    install_default_drivers();
}

/// Open an [`AnyPool`] from a URL string.
///
/// URL prefix selects the backend:
/// - `sqlite:…`                        → SQLite (WAL, foreign-keys on)
/// - `postgres://…` / `postgresql://…` → Postgres
pub async fn open_pool(url: &str) -> Result<AnyPool, StoreError> {
    init_drivers();
    // In-memory SQLite must use a single connection so all queries share the same DB.
    let max_conn = if url == "sqlite::memory:" { 1 } else { 16 };
    let pool = sqlx::any::AnyPoolOptions::new()
        .max_connections(max_conn)
        .acquire_timeout(Duration::from_secs(5))
        .connect(url)
        .await?;
    Ok(pool)
}

/// Ephemeral in-memory SQLite pool for unit tests (single shared connection).
pub async fn open_memory() -> Result<AnyPool, StoreError> {
    open_pool("sqlite::memory:").await
}
