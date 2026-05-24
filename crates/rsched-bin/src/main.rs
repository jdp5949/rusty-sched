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
use rsched_api::{router as api_router, seed_admin_if_empty, AppState};
use rsched_core::Run;
use rsched_core::RunState;
use rsched_scheduler::{
    should_retry, tick_once, DispatchIntent, Dispatcher, HandleRegistry, SchedulerConfig,
};
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
        /// Database URL (sqlite:… or postgres://…). Overrides --db.
        /// Defaults to SQLite at the OS data dir when absent.
        #[arg(long, env = "RSCHED_DB_URL")]
        db_url: Option<String>,
        /// SQLite file path (legacy). Ignored when --db-url is set.
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
        Cmd::Server { bind, db_url, db } => {
            run_server(&bind, db_url.as_deref(), db.as_deref()).await
        }
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

async fn run_server(
    bind: &str,
    db_url: Option<&str>,
    db_path_override: Option<&str>,
) -> Result<()> {
    let url: String = if let Some(u) = db_url {
        u.to_string()
    } else {
        let path = match db_path_override {
            Some(p) => std::path::PathBuf::from(p),
            None => {
                let dirs = ProjectDirs::from("io", "rustysched", "rusty-sched")
                    .context("could not resolve OS data dir")?;
                let dir = dirs.data_dir();
                std::fs::create_dir_all(dir).context("create data dir")?;
                dir.join("rusty.db")
            }
        };
        format!("sqlite://{}", path.display())
    };
    info!(url = %url, "opening database");
    rsched_store::init_drivers();
    let pool = rsched_store::open_pool(&url).await?;
    let store = Store::with_url(pool, &url);
    store.migrate().await?;
    let store_arc = Arc::new(store.clone());

    let registry = Arc::new(HandleRegistry::new());
    let (dispatcher, mut dispatch_rx) = Dispatcher::bounded(10_000);
    let executor = Arc::new(LocalExecutor::new());

    // Dispatch consumer: pull intents off the queue, exec on LocalExecutor,
    // stream logs back, persist final run state.
    let store_disp = store_arc.clone();
    let executor_disp = executor.clone();
    let dispatcher_disp = dispatcher.clone();
    let registry_disp = registry.clone();
    tokio::spawn(async move {
        while let Some(intent) = dispatch_rx.recv().await {
            let store = store_disp.clone();
            let executor = executor_disp.clone();
            let dispatcher_ref = dispatcher_disp.clone();
            let registry_ref = registry_disp.clone();
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
                registry_ref.insert(
                    run_id.to_string(),
                    handle.kill_tx.clone(),
                    handle.signal_tx.clone(),
                );

                // Mark as running.
                let mut run = intent.run.clone();
                run.state = RunState::Running;
                run.started_at = Some(chrono::Utc::now());
                let _ = store.runs().update(&run).await;

                // Persist log chunks (capped at 100 MB per run).
                const LOG_CAP: u64 = 100 * 1024 * 1024;
                let mut bytes = 0u64;
                let mut seq: i64 = 0;
                let mut truncated = false;
                while let Some(chunk) = handle.logs.next().await {
                    let chunk_len = chunk.bytes.len() as u64;
                    if !truncated && bytes + chunk_len <= LOG_CAP {
                        let stream_str = match chunk.stream {
                            rsched_agent::Stream::Stdout => "stdout",
                            rsched_agent::Stream::Stderr => "stderr",
                        };
                        let ts = chunk.ts.to_rfc3339();
                        let _ = store
                            .run_logs()
                            .append(&run_id.to_string(), seq, &ts, stream_str, &chunk.bytes)
                            .await;
                        seq += 1;
                    } else if !truncated {
                        truncated = true;
                        run.log_truncated = true;
                    }
                    bytes += chunk_len;
                }

                match handle.outcome.await {
                    Ok(Ok(o)) => {
                        run.state = if o.timed_out {
                            RunState::Failed
                        } else {
                            match o.exit_code {
                                Some(code) => match job_for_retry.exit_policy.evaluate(code) {
                                    rsched_core::RunOutcome::Success
                                    | rsched_core::RunOutcome::Conditional => RunState::Success,
                                    rsched_core::RunOutcome::Failure => RunState::Failed,
                                },
                                None => RunState::Failed,
                            }
                        };
                        run.exit_code = o.exit_code;
                        run.finished_at = Some(o.finished_at);
                        run.log_bytes = bytes;
                        run.peak_rss_bytes = o.peak_rss_bytes;
                        run.cpu_user_secs = o.cpu_user_secs;
                        run.cpu_sys_secs = o.cpu_sys_secs;
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
                registry_ref.remove(&run_id.to_string());
                let _ = store.runs().update(&run).await;
                // Release any virtual-resource holds this run acquired.
                let _ = store.resources().release(run_id).await;

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

    // File-trigger watcher: rescan jobs every 10s, maintain a notify Watcher
    // over the union of all `Trigger::File { path }` paths. On a matching
    // filesystem event, set next_fire_at = now for any job whose mask
    // accepts the event.
    let fw_store = store.clone();
    tokio::spawn(async move {
        file_watcher_loop(fw_store).await;
    });

    // HTTP server: API + UI both routed.
    let state = AppState::with_registry(store, registry);
    if let Err(e) = seed_admin_if_empty(&state).await {
        warn!(error = %e, "failed to seed initial admin user");
    }
    let app = api_router(state.clone()).merge(rsched_ui::router());
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

/// File-trigger watcher loop.
///
/// Every 10s, scan all jobs with `Trigger::File { path, event }` and rebuild
/// the notify Watcher when the path set changes. On any inotify event, look
/// up the matching jobs and set `next_fire_at = now` for those whose event
/// mask accepts the kind.
async fn file_watcher_loop(store: rsched_store::Store) {
    use notify::{RecursiveMode, Watcher};
    use rsched_core::Trigger;
    use std::collections::{HashMap, HashSet};

    let (ev_tx, mut ev_rx) = tokio::sync::mpsc::unbounded_channel::<notify::Event>();
    let watcher_result = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            let _ = ev_tx.send(ev);
        }
    });
    let mut watcher = match watcher_result {
        Ok(w) => w,
        Err(e) => {
            warn!(error = %e, "file watcher init failed; file triggers disabled");
            return;
        }
    };
    let mut watched: HashSet<String> = HashSet::new();
    let mut last_rescan = std::time::Instant::now()
        .checked_sub(Duration::from_secs(60))
        .unwrap_or_else(std::time::Instant::now);

    loop {
        // Periodic rescan to pick up new/removed file-trigger jobs.
        if last_rescan.elapsed() >= Duration::from_secs(10) {
            last_rescan = std::time::Instant::now();
            let jobs = match store.jobs().list().await {
                Ok(j) => j,
                Err(e) => {
                    warn!(error = %e, "file watcher: jobs list failed");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };
            let mut wanted: HashSet<String> = HashSet::new();
            for j in &jobs {
                if let Trigger::File { path, .. } = &j.trigger {
                    if !j.paused {
                        wanted.insert(path.clone());
                    }
                }
            }
            for new in wanted.difference(&watched) {
                let p = std::path::Path::new(new);
                if let Err(e) = watcher.watch(p, RecursiveMode::NonRecursive) {
                    warn!(path = %new, error = %e, "file watcher: watch() failed");
                }
            }
            for gone in watched.difference(&wanted) {
                let p = std::path::Path::new(gone);
                let _ = watcher.unwatch(p);
            }
            watched = wanted;
        }

        // Drain events with a short timeout so we re-check rescan time.
        let recv_timeout = tokio::time::timeout(Duration::from_secs(1), ev_rx.recv()).await;
        let Ok(Some(ev)) = recv_timeout else {
            continue;
        };
        // Re-load jobs to map paths → jobs (could cache but rescans are cheap).
        let jobs = match store.jobs().list().await {
            Ok(j) => j,
            Err(_) => continue,
        };
        let mut by_path: HashMap<String, Vec<&rsched_core::Job>> = HashMap::new();
        for j in &jobs {
            if let Trigger::File { path, .. } = &j.trigger {
                by_path.entry(path.clone()).or_default().push(j);
            }
        }
        for p in &ev.paths {
            let key = p.to_string_lossy().to_string();
            // Match the exact path or its parent.
            let mut targets: Vec<&rsched_core::Job> = Vec::new();
            if let Some(v) = by_path.get(&key) {
                targets.extend(v.iter().copied());
            }
            if let Some(parent) = p.parent() {
                let pk = parent.to_string_lossy().to_string();
                if pk != key {
                    if let Some(v) = by_path.get(&pk) {
                        targets.extend(v.iter().copied());
                    }
                }
            }
            for j in targets {
                let want = match &j.trigger {
                    Trigger::File { event, .. } => event.clone(),
                    _ => continue,
                };
                if !event_matches(&ev.kind, &want) {
                    continue;
                }
                let _ = store
                    .jobs()
                    .set_next_fire(j.id, Some(chrono::Utc::now()))
                    .await;
                info!(job = %j.name, path = %key, event = ?ev.kind, "file trigger fired");
            }
        }
    }
}

fn event_matches(kind: &notify::EventKind, want: &str) -> bool {
    use notify::EventKind;
    match want {
        "any" => true,
        "create" => matches!(kind, EventKind::Create(_)),
        "modify" => matches!(kind, EventKind::Modify(_)),
        "delete" => matches!(kind, EventKind::Remove(_)),
        _ => false,
    }
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
