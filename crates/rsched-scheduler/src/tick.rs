//! Tick loop: poll `Store::jobs().due()`, recompute next_fire, optionally
//! enqueue dispatch intents. Condition-triggered jobs are evaluated each tick.

use crate::condition_ctx::StoreUpstreamState;
use crate::cron::next_fire;
use crate::{DispatchIntent, Dispatcher, SchedulerError};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use rsched_conditions::{evaluate, parse};
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

/// Run a single tick: dispatch due jobs and evaluate Condition-triggered jobs.
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
        let run = Run::new(job.id, 1);
        store.runs().insert(&run).await?;
        let intent = DispatchIntent {
            job: job.clone(),
            run,
        };
        if dispatcher.try_send(intent).is_err() {
            warn!(job = %job.name, "dispatch queue full, leaving run queued");
            continue;
        }
        emitted += 1;
        let next = compute_next(&job.trigger, now)?;
        store.jobs().set_next_fire(job.id, next).await?;
    }
    emitted += tick_conditions(store, dispatcher).await?;
    Ok(emitted)
}

async fn tick_conditions(store: &Store, dispatcher: &Dispatcher) -> Result<usize, SchedulerError> {
    let all_jobs = store.jobs().list().await?;
    let mut emitted = 0;
    for job in all_jobs {
        if job.paused {
            continue;
        }
        let expr_str = match &job.trigger {
            Trigger::Condition { expr } => expr.clone(),
            _ => continue,
        };
        let expr = match parse(&expr_str) {
            Ok(e) => e,
            Err(e) => {
                warn!(job = %job.name, err = %e, "condition expr parse error");
                continue;
            }
        };
        let ctx = StoreUpstreamState::new(store.clone()).await?;
        if evaluate(&expr, &ctx) == Some(true) {
            if store.runs().has_active_for_job(job.id).await? {
                continue;
            }
            let run = Run::new(job.id, 1);
            store.runs().insert(&run).await?;
            let intent = DispatchIntent {
                job: job.clone(),
                run,
            };
            if dispatcher.try_send(intent).is_err() {
                warn!(job = %job.name, "dispatch queue full for condition job");
                continue;
            }
            emitted += 1;
            store.jobs().set_next_fire(job.id, None).await?;
        }
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
        | Trigger::Manual
        | Trigger::Condition { .. } => None,
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
        let updated = store.jobs().get(job.id).await.unwrap();
        assert!(updated.next_fire_at.unwrap() > Utc::now());
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

    #[tokio::test]
    async fn condition_job_fires_when_upstream_succeeds() {
        use rsched_core::RunState;
        let store = fresh_store().await;
        let (tx, mut rx) = Dispatcher::bounded(16);
        let job_a = JobBuilder::new("A", "echo", Trigger::Manual)
            .build()
            .unwrap();
        store.jobs().insert(&job_a).await.unwrap();
        let job_b = JobBuilder::new(
            "B",
            "echo",
            Trigger::Condition {
                expr: "success(A)".into(),
            },
        )
        .build()
        .unwrap();
        store.jobs().insert(&job_b).await.unwrap();
        // B should not fire without A having run.
        let n = tick_once(&store, &tx, Utc::now(), SchedulerConfig::default())
            .await
            .unwrap();
        assert_eq!(n, 0);
        // Simulate A completing successfully.
        let run_a = Run::new(job_a.id, 1);
        store.runs().insert(&run_a).await.unwrap();
        store
            .runs()
            .set_state(run_a.id, RunState::Success)
            .await
            .unwrap();
        // B should fire now.
        let n = tick_once(&store, &tx, Utc::now(), SchedulerConfig::default())
            .await
            .unwrap();
        assert_eq!(n, 1);
        let intent = rx.recv().await.unwrap();
        assert_eq!(intent.job.name, "B");
    }
}
