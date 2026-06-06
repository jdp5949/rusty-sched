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
/// SQLite file URLs are normalized to create the database if missing
/// (`?mode=rwc`) so `rusty-sched server` works on a fresh host.
pub async fn open_pool(url: &str) -> Result<AnyPool, StoreError> {
    init_drivers();
    let url = normalize_sqlite_url(url);
    // In-memory SQLite must use a single connection so all queries share the same DB.
    let max_conn = if url == "sqlite::memory:" { 1 } else { 16 };
    let pool = sqlx::any::AnyPoolOptions::new()
        .max_connections(max_conn)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await?;
    Ok(pool)
}

/// Ensure a SQLite file URL creates the database if it does not yet exist.
///
/// sqlx opens SQLite read-only by default; without `mode=rwc` a first boot on
/// a fresh host fails with "unable to open database file" (code 14). Leaves
/// `sqlite::memory:`, already-parameterized, and non-sqlite URLs untouched.
fn normalize_sqlite_url(url: &str) -> String {
    if url.starts_with("sqlite:") && url != "sqlite::memory:" && !url.contains("mode=") {
        let sep = if url.contains('?') { '&' } else { '?' };
        format!("{url}{sep}mode=rwc")
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_mode_to_plain_sqlite_path() {
        assert_eq!(
            normalize_sqlite_url("sqlite:///var/lib/x/rusty.db"),
            "sqlite:///var/lib/x/rusty.db?mode=rwc"
        );
    }

    #[test]
    fn appends_with_amp_when_query_present() {
        assert_eq!(
            normalize_sqlite_url("sqlite:///x.db?cache=shared"),
            "sqlite:///x.db?cache=shared&mode=rwc"
        );
    }

    #[test]
    fn leaves_memory_and_existing_mode_alone() {
        assert_eq!(normalize_sqlite_url("sqlite::memory:"), "sqlite::memory:");
        assert_eq!(
            normalize_sqlite_url("sqlite:///x.db?mode=ro"),
            "sqlite:///x.db?mode=ro"
        );
    }

    #[tokio::test]
    async fn creates_sqlite_file_on_open() {
        let dir = std::env::temp_dir().join(format!("rsched-pool-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("fresh.db");
        let _ = std::fs::remove_file(&db);
        let url = format!("sqlite://{}", db.display());
        let pool = open_pool(&url).await.expect("open creates file");
        assert!(db.exists(), "db file should be created");
        pool.close().await;
        let _ = std::fs::remove_file(&db);
    }
}

/// Ephemeral in-memory SQLite pool for unit tests (single shared connection).
pub async fn open_memory() -> Result<AnyPool, StoreError> {
    open_pool("sqlite::memory:").await
}
