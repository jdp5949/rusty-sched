//! In-process replay-dedup cache for webhook receiver.
//!
//! Keeps a `(slug, sha256(body))` fingerprint for a configurable TTL
//! (default 5 minutes) so that a captured webhook request cannot be
//! replayed indefinitely once its HMAC has been observed.
//!
//! ## Scope
//! The cache is **per-process** only. Multi-replica deployments need an
//! external coordinator (Raft, Redis, etc.) to dedup across replicas —
//! that is out of scope for v0.3.4.

use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Default TTL window for replay dedup (5 minutes).
pub const DEFAULT_WINDOW_SECS: u64 = 300;
/// Default upper bound on cached fingerprints.
pub const DEFAULT_MAX_ENTRIES: usize = 10_000;
/// Env var: TTL window in whole seconds.
pub const ENV_WINDOW: &str = "RSCHED_WEBHOOK_DEDUP_WINDOW_SECS";
/// Env var: max cached fingerprints.
pub const ENV_MAX: &str = "RSCHED_WEBHOOK_DEDUP_MAX";

/// In-memory LRU-ish dedup cache. Keyed by `"<slug>:<body_hash_hex>"`.
#[derive(Debug)]
pub struct WebhookDedup {
    seen: DashMap<String, Instant>,
    window: Duration,
    max_entries: usize,
}

impl WebhookDedup {
    /// Build a cache with explicit window + size cap.
    pub fn new(window: Duration, max_entries: usize) -> Self {
        Self {
            seen: DashMap::new(),
            window,
            max_entries: max_entries.max(1),
        }
    }

    /// Build from `RSCHED_WEBHOOK_DEDUP_WINDOW_SECS` + `RSCHED_WEBHOOK_DEDUP_MAX`,
    /// falling back to defaults on missing/invalid values.
    pub fn from_env() -> Self {
        let window_secs = std::env::var(ENV_WINDOW)
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_WINDOW_SECS);
        let max = std::env::var(ENV_MAX)
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(DEFAULT_MAX_ENTRIES);
        Self::new(Duration::from_secs(window_secs), max)
    }

    /// TTL window.
    pub fn window(&self) -> Duration {
        self.window
    }

    /// Current count of live + stale entries.
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    /// `true` if no entries are tracked.
    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }

    /// Check + insert atomically.
    ///
    /// Returns `true` if `key` was *already* present with an unexpired
    /// timestamp (caller should reject as duplicate). Returns `false`
    /// otherwise and records `Instant::now()` as the fingerprint time.
    pub fn check_and_insert(&self, key: String) -> bool {
        let now = Instant::now();
        if let Some(mut existing) = self.seen.get_mut(&key) {
            if now.duration_since(*existing) < self.window {
                return true;
            }
            // Stale entry — refresh and treat as new.
            *existing = now;
            return false;
        }
        if self.seen.len() >= self.max_entries {
            self.evict_oldest();
        }
        self.seen.insert(key, now);
        false
    }

    /// Drop expired entries.
    pub fn prune(&self) {
        let now = Instant::now();
        let window = self.window;
        self.seen.retain(|_, ts| now.duration_since(*ts) < window);
    }

    /// Evict the single oldest entry to make room for an insert.
    fn evict_oldest(&self) {
        let mut oldest_key: Option<String> = None;
        let mut oldest_ts: Option<Instant> = None;
        for entry in self.seen.iter() {
            let ts = *entry.value();
            if oldest_ts.is_none_or(|t| ts < t) {
                oldest_ts = Some(ts);
                oldest_key = Some(entry.key().clone());
            }
        }
        if let Some(k) = oldest_key {
            self.seen.remove(&k);
        }
    }
}

/// Spawn a background task that prunes the cache every `interval`.
///
/// Returns the spawned [`tokio::task::JoinHandle`]; callers may drop it
/// to detach. Held `Arc` is the only thing keeping the cache alive.
pub fn spawn_pruner(cache: Arc<WebhookDedup>, interval: Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        // Skip the immediate first tick.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            cache.prune();
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_insert_is_unique() {
        let c = WebhookDedup::new(Duration::from_secs(60), 16);
        assert!(!c.check_and_insert("a".into()));
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn second_insert_within_window_is_duplicate() {
        let c = WebhookDedup::new(Duration::from_secs(60), 16);
        assert!(!c.check_and_insert("a".into()));
        assert!(c.check_and_insert("a".into()));
    }

    #[test]
    fn distinct_keys_are_independent() {
        let c = WebhookDedup::new(Duration::from_secs(60), 16);
        assert!(!c.check_and_insert("a".into()));
        assert!(!c.check_and_insert("b".into()));
    }

    #[test]
    fn prune_removes_expired() {
        let c = WebhookDedup::new(Duration::from_millis(1), 16);
        let _ = c.check_and_insert("a".into());
        std::thread::sleep(Duration::from_millis(10));
        c.prune();
        assert!(c.is_empty());
    }

    #[test]
    fn evicts_when_full() {
        let c = WebhookDedup::new(Duration::from_secs(60), 2);
        let _ = c.check_and_insert("a".into());
        std::thread::sleep(Duration::from_millis(2));
        let _ = c.check_and_insert("b".into());
        std::thread::sleep(Duration::from_millis(2));
        let _ = c.check_and_insert("c".into());
        assert_eq!(c.len(), 2);
        // Oldest (`a`) should have been evicted.
        assert!(!c.check_and_insert("a".into()));
    }
}
