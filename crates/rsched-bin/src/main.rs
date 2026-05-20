//! rusty-sched — single-binary entrypoint. Modes: server | agent | cli.
//!
//! Single-node mode wired end-to-end:
//!   - SQLite at `$DATA_DIR/rusty.db` (auto-created)
//!   - REST API + embedded UI on 0.0.0.0:8080
//!   - Scheduler tick every 1s, dispatches to LocalExecutor in-process
//!   - Graceful shutdown on SIGINT/SIGTERM
//!
//! Raft HA mode (multi-node) is M10 (deferred).

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use directories::ProjectDirs;
use futures::StreamExt;
use rsched_agent::{Executor, LocalExecutor};
use rsched_api::{router as api_router, AppState};
use rsched_core::Run;
use rsched_core::RunState;
use rsched_scheduler::{should_retry, tick_once, DispatchIntent, Dispatcher, SchedulerConfig};
use rsched_store::Store;
use tokio::net::TcpListener;
use tracing::{error, info, warn};

#[derive(Parser)]
#[command(
    name = "rusty-sched",
    version,
    about = "Reliable job scheduler — one binary"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the scheduler server (single-node; Raft HA is M10).
    Server {
        /// Bind address.
        #[arg(long, env = "RSCHED_BIND", default_value = "0.0.0.0:8080")]
        bind: String,
        /// SQLite file path. Defaults to OS-specific data dir.
        #[arg(long, env = "RSCHED_DB")]
        db: Option<String>,
    },
    /// Run the execution agent on this host (M4 remote agent; today this is
    /// a placeholder — single-node `server` uses an embedded LocalExecutor).
    Agent,
    /// CLI client (list / apply / trigger / pause / resume).
    Cli(rsched_cli::Cli),
    /// Print version + build info.
    Version,
}

#[tokio::main]
async fn main() -> Result<()> {
    let fmt = tracing_subscriber::fmt().with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "info,rsched=info".into()),
    );
    if std::env::var_os("RSCHED_JSON").is_some() {
        fmt.json().init();
    } else {
        fmt.init();
    }

    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Server { bind, db } => run_server(&bind, db.as_deref()).await,
        Cmd::Agent => {
            anyhow::bail!("standalone agent process is M4 (gRPC). Today the server runs jobs in-process via LocalExecutor.");
        }
        Cmd::Cli(c) => rsched_cli::run_cli(c).await,
        Cmd::Version => {
            println!("rusty-sched {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}

async fn run_server(bind: &str, db_override: Option<&str>) -> Result<()> {
    let db_path = match db_override {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            let dirs = ProjectDirs::from("io", "rustysched", "rusty-sched")
                .context("could not resolve OS data dir")?;
            let dir = dirs.data_dir();
            std::fs::create_dir_all(dir).context("create data dir")?;
            dir.join("rusty.db")
        }
    };
    info!(db = %db_path.display(), "opening SQLite");
    let pool = rsched_store::open_pool(&db_path).await?;
    let store = Store::new(pool);
    store.migrate().await?;
    let store_arc = Arc::new(store.clone());

    let (dispatcher, mut dispatch_rx) = Dispatcher::bounded(10_000);
    let executor = Arc::new(LocalExecutor::new());

    // Dispatch consumer: pull intents off the queue, exec on LocalExecutor,
    // stream logs back, persist final run state.
    let store_disp = store_arc.clone();
    let executor_disp = executor.clone();
    let dispatcher_disp = dispatcher.clone();
    tokio::spawn(async move {
        while let Some(intent) = dispatch_rx.recv().await {
            let store = store_disp.clone();
            let executor = executor_disp.clone();
            let dispatcher_ref = dispatcher_disp.clone();
            tokio::spawn(async move {
                let run_id = intent.run.id;
                let job_name = intent.job.name.clone();
                let job_for_retry = intent.job.clone();
                let mut handle = match executor.dispatch(run_id, intent.job).await {
                    Ok(h) => h,
                    Err(e) => {
                        error!(%run_id, error = %e, "spawn failed");
                        let mut r = intent.run.clone();
                        r.state = RunState::Failed;
                        r.finished_at = Some(chrono::Utc::now());
                        let _ = store.runs().update(&r).await;
                        return;
                    }
                };

                // Mark as running.
                let mut run = intent.run.clone();
                run.state = RunState::Running;
                run.started_at = Some(chrono::Utc::now());
                let _ = store.runs().update(&run).await;

                // Drain logs into bytes counter (full log_logs row storage is M6.1).
                let mut bytes = 0u64;
                while let Some(chunk) = handle.logs.next().await {
                    bytes += chunk.bytes.len() as u64;
                }

                match handle.outcome.await {
                    Ok(Ok(o)) => {
                        run.state = if o.timed_out {
                            RunState::Failed
                        } else if o.exit_code == Some(0) {
                            RunState::Success
                        } else {
                            RunState::Failed
                        };
                        run.exit_code = o.exit_code;
                        run.finished_at = Some(o.finished_at);
                        run.log_bytes = bytes;
                        info!(%run_id, job=%job_name, state=?run.state, "run finished");
                    }
                    Ok(Err(_killed)) => {
                        run.state = RunState::Killed;
                        run.finished_at = Some(chrono::Utc::now());
                        warn!(%run_id, "run killed");
                    }
                    Err(e) => {
                        run.state = RunState::Lost;
                        run.finished_at = Some(chrono::Utc::now());
                        error!(%run_id, error=%e, "outcome task panicked");
                    }
                }
                let _ = store.runs().update(&run).await;

                // Retry: schedule next attempt if policy says so.
                if should_retry(&job_for_retry, &run) {
                    let next_attempt = run.attempt + 1;
                    schedule_retry(
                        store.clone(),
                        dispatcher_ref.clone(),
                        job_for_retry,
                        run.id,
                        next_attempt,
                    );
                }
            });
        }
    });

    // Scheduler tick loop (1s).
    let tick_store = store.clone();
    let tick_dispatcher = dispatcher.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            if let Err(e) = tick_once(
                &tick_store,
                &tick_dispatcher,
                chrono::Utc::now(),
                SchedulerConfig::default(),
            )
            .await
            {
                warn!(error = %e, "tick failed");
            }
        }
    });

    // HTTP server: API + UI both routed.
    let state = AppState::new(store);
    let app = api_router(state).merge(rsched_ui::router());
    let listener = TcpListener::bind(bind)
        .await
        .with_context(|| format!("bind {bind}"))?;
    info!(addr = %bind, ui = "/", api = "/api/v1", "rusty-sched server up");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("axum serve")?;
    info!("shutdown complete");
    Ok(())
}

/// Persist a retry run and enqueue it after the backoff delay.
///
/// Extracted for testability — `schedule_retry` owns its inputs and runs
/// the sleep + insert + send in a spawned task.  Returns the `JoinHandle`
/// so callers can await it in tests.
fn schedule_retry(
    store: Arc<rsched_store::Store>,
    dispatcher: Dispatcher,
    job: rsched_core::Job,
    prev_run_id: rsched_core::RunId,
    next_attempt: u32,
) -> tokio::task::JoinHandle<()> {
    let delay = job.retry.backoff.delay_for(next_attempt);
    tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        let mut new_run = Run::new(job.id, next_attempt);
        new_run.parent_run_ids = vec![prev_run_id];
        if let Err(e) = store.runs().insert(&new_run).await {
            error!(error=%e, "failed to persist retry run");
            return;
        }
        info!(job=%job.name, attempt=next_attempt, "scheduling retry");
        let _ = dispatcher.send(DispatchIntent { job, run: new_run }).await;
    })
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        let mut s = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler");
        s.recv().await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    info!("shutdown signal received");
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsched_core::{BackoffKind, JobBuilder, RetryPolicy, Run, RunId, RunState, Trigger};
    use rsched_store::Store;

    async fn fresh_store() -> Arc<Store> {
        let pool = rsched_store::open_memory().await.unwrap();
        let store = Store::new(pool);
        store.migrate().await.unwrap();
        Arc::new(store)
    }

    fn failing_job(max_attempts: u32) -> rsched_core::Job {
        JobBuilder::new(
            "retry-test",
            "false",
            Trigger::Cron {
                expr: "* * * * *".into(),
                timezone: None,
            },
        )
        .retry(RetryPolicy {
            max_attempts,
            backoff: BackoffKind::Fixed { delay_secs: 0 },
        })
        .build()
        .unwrap()
    }

    /// Simulate N-1 retries for a job with max_attempts=N using schedule_retry.
    /// Verifies: exactly N runs in store, monotonically increasing attempt,
    /// and each non-first run records previous run id in parent_run_ids.
    #[tokio::test]
    async fn retry_chain_persists_attempts() {
        let store = fresh_store().await;
        let job = failing_job(3);
        store.jobs().insert(&job).await.unwrap();

        let (dispatcher, mut rx) = Dispatcher::bounded(16);

        // Insert the initial run (attempt 1) as if it was created by tick.
        let mut run1 = Run::new(job.id, 1);
        run1.state = RunState::Failed;
        run1.finished_at = Some(chrono::Utc::now());
        store.runs().insert(&run1).await.unwrap();

        // Attempt 1 failed — schedule retry for attempt 2.
        assert!(should_retry(&job, &run1));
        let h1 = schedule_retry(store.clone(), dispatcher.clone(), job.clone(), run1.id, 2);
        h1.await.unwrap();

        // Receive the dispatched intent for attempt 2.
        let intent2 = rx.recv().await.unwrap();
        assert_eq!(intent2.run.attempt, 2);
        assert_eq!(intent2.run.parent_run_ids, vec![run1.id]);

        // Mark attempt 2 as failed; schedule retry for attempt 3.
        let mut run2 = intent2.run.clone();
        run2.state = RunState::Failed;
        run2.finished_at = Some(chrono::Utc::now());
        store.runs().update(&run2).await.unwrap();

        assert!(should_retry(&job, &run2));
        let h2 = schedule_retry(store.clone(), dispatcher.clone(), job.clone(), run2.id, 3);
        h2.await.unwrap();

        let intent3 = rx.recv().await.unwrap();
        assert_eq!(intent3.run.attempt, 3);
        assert_eq!(intent3.run.parent_run_ids, vec![run2.id]);

        // Attempt 3 = max_attempts; should NOT retry.
        let mut run3 = intent3.run.clone();
        run3.state = RunState::Failed;
        run3.finished_at = Some(chrono::Utc::now());
        store.runs().update(&run3).await.unwrap();
        assert!(!should_retry(&job, &run3));

        // Verify 3 runs in store with monotonically increasing attempts.
        let runs = store.runs().list_for_job(job.id, 10).await.unwrap();
        assert_eq!(runs.len(), 3);
        let mut attempts: Vec<u32> = runs.iter().map(|r| r.attempt).collect();
        attempts.sort();
        assert_eq!(attempts, vec![1, 2, 3]);

        // Suppress unused RunId warning
        let _ = RunId::new();
    }

    #[tokio::test]
    async fn no_retry_on_success() {
        let job = failing_job(3);
        let mut run = Run::new(job.id, 1);
        run.state = RunState::Success;
        assert!(!should_retry(&job, &run));
    }

    #[tokio::test]
    async fn no_retry_on_killed() {
        let job = failing_job(3);
        let mut run = Run::new(job.id, 1);
        run.state = RunState::Killed;
        assert!(!should_retry(&job, &run));
    }
}
