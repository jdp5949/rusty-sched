//! Tick loop: poll `Store::jobs().due()`, recompute next_fire, optionally
//! enqueue dispatch intents.
//!
//! Calendar gating and dep evaluation will be wired in once those resolvers
//! are integrated with the store; for now the tick handles cron/interval/
//! one-shot triggers and respects pause.

use crate::cron::next_fire;
use crate::{DispatchIntent, Dispatcher, SchedulerError};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use rsched_core::{Run, Trigger};
use rsched_store::Store;
use tracing::warn;

/// Tick loop configuration.
#[derive(Clone, Copy, Debug)]
pub struct SchedulerConfig {
    /// How far back from `now` we treat as "still missed" vs ignored.
    pub misfire_grace_secs: i64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            misfire_grace_secs: 300,
        }
    }
}

/// Run a single tick: dispatch all due jobs, advance their `next_fire_at`.
/// Returns the number of intents emitted.
pub async fn tick_once(
    store: &Store,
    dispatcher: &Dispatcher,
    now: DateTime<Utc>,
    _cfg: SchedulerConfig,
) -> Result<usize, SchedulerError> {
    let due = store.jobs().due(now).await?;
    let mut emitted = 0usize;
    for job in due {
        // Persist run BEFORE dispatch (so on crash, run is recoverable).
        let run = Run::new(job.id, 1);
        store.runs().insert(&run).await?;
        let intent = DispatchIntent {
            job: job.clone(),
            run,
        };
        if let Err(_full) = dispatcher.try_send(intent) {
            warn!(job = %job.name, "dispatch queue full, leaving run queued");
            // queue is bounded — caller should observe and alert.
            continue;
        }
        emitted += 1;
        let next = compute_next(&job.trigger, now)?;
        store.jobs().set_next_fire(job.id, next).await?;
    }
    Ok(emitted)
}

fn compute_next(
    trigger: &Trigger,
    now: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, SchedulerError> {
    Ok(match trigger {
        Trigger::Cron { expr, timezone } => Some(next_fire(expr, timezone.as_deref(), now)?),
        Trigger::Interval { every, .. } => Some(now + ChronoDuration::from_std(*every).unwrap()),
        Trigger::OneShot { .. }
        | Trigger::Dep { .. }
        | Trigger::File { .. }
        | Trigger::Webhook { .. }
        | Trigger::Manual => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsched_core::{JobBuilder, Trigger};
    use rsched_store::Store;

    async fn fresh_store() -> Store {
        rsched_store::init_drivers();
        let pool = rsched_store::open_pool("sqlite::memory:").await.unwrap();
        let s = Store::with_url(pool, "sqlite::memory:");
        s.migrate().await.unwrap();
        s
    }

    #[tokio::test]
    async fn tick_emits_due_jobs() {
        let store = fresh_store().await;
        let (tx, mut rx) = Dispatcher::bounded(16);

        let job = JobBuilder::new(
            "j",
            "echo",
            Trigger::Cron {
                expr: "*/5 * * * *".into(),
                timezone: None,
            },
        )
        .build()
        .unwrap();
        store.jobs().insert(&job).await.unwrap();
        store
            .jobs()
            .set_next_fire(job.id, Some(Utc::now() - ChronoDuration::seconds(1)))
            .await
            .unwrap();

        let n = tick_once(&store, &tx, Utc::now(), SchedulerConfig::default())
            .await
            .unwrap();
        assert_eq!(n, 1);
        let intent = rx.recv().await.unwrap();
        assert_eq!(intent.job.name, "j");

        // next_fire_at advanced.
        let updated = store.jobs().get(job.id).await.unwrap();
        assert!(updated.next_fire_at.unwrap() > Utc::now());
        // run persisted.
        let active = store.runs().list_active().await.unwrap();
        assert_eq!(active.len(), 1);
    }

    #[tokio::test]
    async fn paused_job_skipped() {
        let store = fresh_store().await;
        let (tx, _rx) = Dispatcher::bounded(16);
        let job = JobBuilder::new(
            "j",
            "echo",
            Trigger::Cron {
                expr: "*/5 * * * *".into(),
                timezone: None,
            },
        )
        .build()
        .unwrap();
        store.jobs().insert(&job).await.unwrap();
        store
            .jobs()
            .set_next_fire(job.id, Some(Utc::now() - ChronoDuration::seconds(1)))
            .await
            .unwrap();
        store.jobs().set_paused(job.id, true).await.unwrap();

        let n = tick_once(&store, &tx, Utc::now(), SchedulerConfig::default())
            .await
            .unwrap();
        assert_eq!(n, 0);
    }
}
