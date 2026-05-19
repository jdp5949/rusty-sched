//! Connection pool helpers.

use crate::StoreError;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

/// Open a pool against an on-disk SQLite file (WAL mode, foreign keys on).
/// Creates file if missing.
pub async fn open_pool(path: impl AsRef<Path>) -> Result<SqlitePool, StoreError> {
    let opts = SqliteConnectOptions::new()
        .filename(path.as_ref())
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(16)
        .connect_with(opts)
        .await?;
    Ok(pool)
}

/// Ephemeral in-memory pool (shared across all connections via URI).
pub async fn open_memory() -> Result<SqlitePool, StoreError> {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")?
        .foreign_keys(true)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await?;
    Ok(pool)
}
