//! rsched-store — storage layer supporting SQLite and Postgres.
//!
//! Embedded migrations, async repos for every domain entity.

#![warn(missing_docs)]

mod error;
mod pool;
mod repo;

pub use error::StoreError;
pub use pool::{init_drivers, open_memory, open_pool};
pub use repo::{AgentRepo, CalendarRepo, JobRepo, LogRow, RunLogRepo, RunRepo, Store};

/// Embedded SQLite migrations.
pub static MIGRATOR_SQLITE: sqlx::migrate::Migrator = sqlx::migrate!("./migrations/sqlite");

/// Embedded Postgres migrations.
pub static MIGRATOR_POSTGRES: sqlx::migrate::Migrator = sqlx::migrate!("./migrations/postgres");

/// Select the right migrator based on the database URL prefix.
pub fn migrator_for_url(url: &str) -> &'static sqlx::migrate::Migrator {
    if url.starts_with("postgres:") || url.starts_with("postgresql:") {
        &MIGRATOR_POSTGRES
    } else {
        &MIGRATOR_SQLITE
    }
}
