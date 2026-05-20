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
use rsched_core::RunState;
use rsched_scheduler::{tick_once, Dispatcher, SchedulerConfig};
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
    tokio::spawn(async move {
        while let Some(intent) = dispatch_rx.recv().await {
            let store = store_disp.clone();
            let executor = executor_disp.clone();
            tokio::spawn(async move {
                let run_id = intent.run.id;
                let job_name = intent.job.name.clone();
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
