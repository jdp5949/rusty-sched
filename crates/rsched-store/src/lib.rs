//! rsched-store — SQLite storage layer.
//!
//! Embedded migrations, async repos for every domain entity. Postgres support
//! was dropped in v2 (Cronicle-model simplification) for a single-store design.

#![warn(missing_docs)]

mod error;
mod pool;
mod repo;

pub use error::StoreError;
pub use pool::{init_drivers, open_memory, open_pool};
pub use repo::{
    AgentRepo, ApiKeyRepo, AuditEntry, AuditRepo, CalendarRepo, DashboardSummary, GlobalsRepo,
    JobRepo, JobStats, LogRow, RecentFailure, ResourceRepo, RunLogRepo, RunRepo, SessionRepo,
    Store, UpcomingJob, UserRepo,
};

/// Embedded SQLite migrations.
pub static MIGRATOR_SQLITE: sqlx::migrate::Migrator = sqlx::migrate!("./migrations/sqlite");

/// The migrator to run. SQLite is the only supported backend.
pub fn migrator_for_url(_url: &str) -> &'static sqlx::migrate::Migrator {
    &MIGRATOR_SQLITE
}
