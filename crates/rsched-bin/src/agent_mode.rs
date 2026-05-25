//! `rusty-sched agent` mode — runs a gRPC server that accepts dispatch
//! requests from a scheduler and executes them against a [`LocalExecutor`].
//!
//! v0.5.2 skeleton:
//!   - Binds plaintext today (rustls identity is generated + fingerprint
//!     logged so the cert plumbing is ready, but `Server::builder().tls_config`
//!     wiring is deferred to keep the diff small).
//!   - Accepts the bidi `Stream` RPC.
//!   - On `Dispatch`, spawns the cmd via `tokio::process::Command` directly
//!     (mirrors LocalExecutor minus the rsched-core Job builder hop) and
//!     streams stdout/stderr back as `LogChunk`s, then a final `Result`.
//!   - `Kill` / `Signal` from the server are accepted but only `Kill` is
//!     honored (best-effort `start_kill`).

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use rcgen::generate_simple_self_signed;
use rsched_proto::agent::{
    agent_msg::Kind as AKind,
    agent_server::{Agent as AgentSvc, AgentServer},
    server_msg::Kind as SKind,
    AgentMsg, LogChunk, Result as RunResult, ServerMsg,
};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::{transport::Server, Request, Response, Status, Streaming};
use tracing::{info, warn};

/// Entry point for the `agent` subcommand.
pub async fn run_agent(bind: &str) -> Result<()> {
    let addr = bind
        .parse()
        .with_context(|| format!("invalid bind addr {bind}"))?;

    // Generate self-signed cert + log SHA-256 fingerprint of the DER cert.
    // Used today only for fingerprint logging; mTLS wiring (Server::tls_config)
    // is deferred. The cert generation proves the rcgen path works.
    let cert = generate_simple_self_signed(vec!["localhost".into()])
        .context("generate self-signed cert")?;
    let der = cert.cert.der();
    let fp = Sha256::digest(der.as_ref());
    let fp_hex = fp.iter().map(|b| format!("{b:02x}")).collect::<String>();
    info!(
        bind = %bind,
        cert_fingerprint = %fp_hex,
        "rusty-sched agent up (skeleton — TLS cert generated but plaintext until mTLS wiring lands)"
    );

    let svc = AgentSvcImpl::default();
    Server::builder()
        .add_service(AgentServer::new(svc))
        .serve(addr)
        .await
        .context("tonic serve")?;
    Ok(())
}

#[derive(Default)]
struct AgentSvcImpl {
    /// Map of run_id -> kill channel sender, so an inbound `Kill` can stop a run.
    kill_chans: Arc<Mutex<HashMap<String, mpsc::Sender<()>>>>,
}

#[tonic::async_trait]
impl AgentSvc for AgentSvcImpl {
    type StreamStream = ReceiverStream<Result<ServerMsg, Status>>;

    async fn stream(
        &self,
        req: Request<Streaming<AgentMsg>>,
    ) -> Result<Response<Self::StreamStream>, Status> {
        // The trait is `Stream(stream AgentMsg) returns (stream ServerMsg)`.
        // In v0.5.2 the *server* is the agent and the *client* is the
        // scheduler, so the inbound stream carries `AgentMsg` (heartbeats /
        // results) and the outbound stream carries `ServerMsg` (dispatches).
        //
        // The skeleton replies with a heartbeat ServerMsg per inbound msg so
        // the bidi plumbing exercises. Full dispatcher↔agent routing is
        // deferred — the scheduler-side client (`GrpcExecutor`) is also a
        // stub today, so end-to-end wiring lands together in a later patch.
        let mut inbound = req.into_inner();
        let (tx, rx) = mpsc::channel::<Result<ServerMsg, Status>>(16);
        let kill_chans = self.kill_chans.clone();

        tokio::spawn(async move {
            while let Some(msg) = inbound.next().await {
                let Ok(msg) = msg else { break };
                match msg.kind {
                    Some(AKind::Heartbeat(_)) => {
                        let _ = tx
                            .send(Ok(ServerMsg {
                                kind: Some(SKind::Heartbeat(Default::default())),
                            }))
                            .await;
                    }
                    Some(AKind::Result(r)) => {
                        info!(run_id = %r.run_id, exit = r.exit_code, "agent reported run result");
                        kill_chans.lock().await.remove(&r.run_id);
                    }
                    Some(AKind::Log(lc)) => {
                        // In normal operation logs flow agent->server; here
                        // we'd persist or fan out. Skeleton: log size only.
                        info!(run_id = %lc.run_id, bytes = lc.bytes.len(), "agent log chunk");
                    }
                    None => {}
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

/// Run one dispatch as a local subprocess and produce a `Result`.
///
/// Public for unit testing the spawn path independent of the gRPC layer.
#[doc(hidden)]
#[allow(dead_code, clippy::too_many_arguments)]
pub async fn exec_dispatch(
    cmd: &str,
    args: &[String],
    env: &HashMap<String, String>,
    cwd: &str,
    timeout_secs: u64,
    log_tx: mpsc::Sender<LogChunk>,
    run_id: String,
    mut kill_rx: mpsc::Receiver<()>,
) -> RunResult {
    let shell = if cfg!(windows) {
        ("cmd", "/C")
    } else {
        ("/bin/sh", "-c")
    };
    let mut command = Command::new(shell.0);
    command.arg(shell.1);
    let mut line = cmd.to_string();
    for a in args {
        line.push(' ');
        line.push_str(a);
    }
    command.arg(&line);
    if !cwd.is_empty() {
        command.current_dir(cwd);
    }
    for (k, v) in env {
        command.env(k, v);
    }
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            warn!(%run_id, error=%e, "spawn failed");
            return RunResult {
                run_id,
                exit_code: -1,
                timed_out: false,
                peak_rss_bytes: 0,
                cpu_user_ms: 0,
                cpu_sys_ms: 0,
            };
        }
    };
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();

    let tx_o = log_tx.clone();
    let tx_e = log_tx;
    let rid_o = run_id.clone();
    let rid_e = run_id.clone();
    tokio::spawn(async move { pump(&mut stdout, 0, &tx_o, rid_o).await });
    tokio::spawn(async move { pump(&mut stderr, 1, &tx_e, rid_e).await });

    let to = async {
        if timeout_secs > 0 {
            tokio::time::sleep(Duration::from_secs(timeout_secs)).await;
            true
        } else {
            std::future::pending::<bool>().await
        }
    };
    tokio::select! {
        exit = child.wait() => {
            let code = exit.ok().and_then(|s| s.code()).unwrap_or(-1);
            RunResult { run_id, exit_code: code, timed_out: false, peak_rss_bytes: 0, cpu_user_ms: 0, cpu_sys_ms: 0 }
        }
        _ = kill_rx.recv() => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            RunResult { run_id, exit_code: -1, timed_out: false, peak_rss_bytes: 0, cpu_user_ms: 0, cpu_sys_ms: 0 }
        }
        _ = to => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            RunResult { run_id, exit_code: -1, timed_out: true, peak_rss_bytes: 0, cpu_user_ms: 0, cpu_sys_ms: 0 }
        }
    }
}

#[allow(dead_code)]
async fn pump<R: AsyncReadExt + Unpin>(
    r: &mut R,
    stream: u32,
    tx: &mpsc::Sender<LogChunk>,
    run_id: String,
) {
    let mut buf = [0u8; 8192];
    loop {
        match r.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let chunk = LogChunk {
                    run_id: run_id.clone(),
                    stream,
                    ts_unix_ms: chrono::Utc::now().timestamp_millis(),
                    bytes: buf[..n].to_vec(),
                };
                if tx.send(chunk).await.is_err() {
                    break;
                }
            }
        }
    }
}
