//! rsched-store — SQLite repository layer.
//!
//! Embedded migrations, async repos for every domain entity.

#![warn(missing_docs)]

mod error;
mod pool;
mod repo;

pub use error::StoreError;
pub use pool::{open_memory, open_pool};
pub use repo::{AgentRepo, CalendarRepo, JobRepo, RunRepo, Store};

/// Embedded migrations (run on [`Store::migrate`]).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
