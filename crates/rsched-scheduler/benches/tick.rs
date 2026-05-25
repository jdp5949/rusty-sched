//! Criterion benches for the scheduler hot path.
//!
//! These exercise:
//!   - `tick_once` with N seeded cron jobs (1k, 10k)
//!   - Condition expression evaluation at varying depth
//!   - Virtual-resource acquire/release contention
//!
//! In-memory SQLite is used as the store so benches run anywhere.
//! Each bench measures wall-clock latency of one operation (not throughput);
//! Criterion handles outlier rejection + statistical summary.

use std::time::Duration as StdDuration;

use chrono::{Duration as ChronoDuration, Utc};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use rsched_conditions::{evaluate, parse, UpstreamState};
use rsched_core::{JobBuilder, Resource, ResourceClaim, ResourceId, RunId, RunState, Trigger};
use rsched_scheduler::{tick_once, Dispatcher, SchedulerConfig};
use rsched_store::Store;
use tokio::runtime::Runtime;

// ---------- helpers ----------------------------------------------------------

async fn fresh_store() -> Store {
    rsched_store::init_drivers();
    let pool = rsched_store::open_pool("sqlite::memory:")
        .await
        .expect("open in-memory sqlite");
    let store = Store::with_url(pool, "sqlite::memory:");
    store.migrate().await.expect("run migrations");
    store
}

/// Seed `n` cron jobs whose `next_fire_at` is in the past so the next tick
/// considers all of them due.
async fn seed_n_due_jobs(store: &Store, n: usize) {
    let past = Utc::now() - ChronoDuration::seconds(1);
    for i in 0..n {
        let job = JobBuilder::new(
            format!("job{i}"),
            "echo",
            Trigger::Cron {
                expr: "*/5 * * * *".into(),
                timezone: None,
            },
        )
        .build()
        .expect("build job");
        store.jobs().insert(&job).await.expect("insert job");
        store
            .jobs()
            .set_next_fire(job.id, Some(past))
            .await
            .expect("set next_fire");
    }
}

/// Build a synthetic `success(jobN) and success(jobN-1) and ...` expression
/// of the requested depth.
fn build_condition_expr(depth: usize) -> String {
    assert!(depth >= 1);
    (0..depth)
        .map(|i| format!("success(job{i})"))
        .collect::<Vec<_>>()
        .join(" and ")
}

/// Minimal in-memory `UpstreamState` mock used by `condition_eval` so the
/// bench doesn't pay for DB round-trips. The field records the synthetic
/// fleet size (kept so future regressions can tie cost to fleet size).
struct AllSucceeded(#[allow(dead_code)] usize);

impl UpstreamState for AllSucceeded {
    fn last_run_state(&self, _job: &str) -> Option<RunState> {
        Some(RunState::Success)
    }
    fn last_exit_code(&self, _job: &str) -> Option<i32> {
        Some(0)
    }
    fn is_running(&self, _job: &str) -> bool {
        false
    }
}

// ---------- benches ----------------------------------------------------------

fn bench_tick_with_n_jobs(c: &mut Criterion) {
    let rt = Runtime::new().expect("tokio rt");
    let mut group = c.benchmark_group("tick_with_n_jobs");
    // Keep the bench tractable on laptops + CI.
    group.sample_size(10);
    group.measurement_time(StdDuration::from_secs(15));

    for &n in &[1_000usize, 10_000] {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            // Set up store + jobs once per parameter — the bench measures only
            // the tick itself (which is what we want; seeding 10k jobs is the
            // slow part and shouldn't dominate the sample).
            let store = rt.block_on(async {
                let s = fresh_store().await;
                seed_n_due_jobs(&s, n).await;
                s
            });
            // Channel is large enough for n intents so try_send never blocks.
            let (tx, _rx) = Dispatcher::bounded(n + 16);
            b.to_async(&rt).iter(|| async {
                // We measure a single `tick_once`. After the first iter all
                // jobs have `next_fire_at` in the future so subsequent
                // iterations would measure the "no work" fast path. To keep
                // the work representative we reset next_fire before each iter.
                let past = Utc::now() - ChronoDuration::seconds(1);
                let jobs = store.jobs().list().await.unwrap();
                for j in jobs {
                    let _ = store.jobs().set_next_fire(j.id, Some(past)).await;
                }
                tick_once(&store, &tx, Utc::now(), SchedulerConfig::default())
                    .await
                    .expect("tick_once");
            });
        });
    }
    group.finish();
}

fn bench_condition_eval(c: &mut Criterion) {
    let mut group = c.benchmark_group("condition_eval");
    group.sample_size(50);

    // Two axes: depth (number of ANDed clauses) and n_jobs (size of the
    // upstream mock). Our mock is O(1) so n_jobs is a noise parameter — we
    // include it to surface any future regression that ties cost to fleet size.
    for &(n_jobs, depth) in &[(10usize, 4usize), (100, 8), (1_000, 16)] {
        let expr_str = build_condition_expr(depth);
        let expr = parse(&expr_str).expect("parse condition");
        let ctx = AllSucceeded(n_jobs);
        let id = format!("n{n_jobs}_d{depth}");
        group.bench_with_input(BenchmarkId::from_parameter(&id), &expr, |b, expr| {
            b.iter(|| {
                let v = evaluate(expr, &ctx);
                assert_eq!(v, Some(true));
            });
        });
    }
    group.finish();
}

fn bench_acquire_release_resource(c: &mut Criterion) {
    let rt = Runtime::new().expect("tokio rt");
    let mut group = c.benchmark_group("acquire_release_resource");
    group.sample_size(20);
    group.measurement_time(StdDuration::from_secs(10));

    for &n_resources in &[1usize, 8, 64] {
        group.bench_with_input(
            BenchmarkId::from_parameter(n_resources),
            &n_resources,
            |b, &n_resources| {
                // Seed the resources once.
                let (store, claims) = rt.block_on(async {
                    let s = fresh_store().await;
                    let mut claims = Vec::with_capacity(n_resources);
                    for i in 0..n_resources {
                        let res = Resource {
                            id: ResourceId::new(),
                            name: format!("res{i}"),
                            capacity: 1_000,
                            description: None,
                            created_at: Utc::now(),
                        };
                        s.resources().insert(&res).await.expect("insert resource");
                        claims.push(ResourceClaim {
                            resource_name: res.name.clone(),
                            units: 1,
                        });
                    }
                    (s, claims)
                });
                b.to_async(&rt).iter(|| async {
                    let run_id = RunId::new();
                    let ok = store
                        .resources()
                        .try_acquire(run_id, &claims)
                        .await
                        .expect("acquire");
                    assert!(ok);
                    store.resources().release(run_id).await.expect("release");
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_tick_with_n_jobs,
    bench_condition_eval,
    bench_acquire_release_resource,
);
criterion_main!(benches);
