//! Local executor — runs jobs via `tokio::process` on the same host.

use crate::exec::{Executor, LogChunk, RunHandle, RunOutcome, Stream};
use crate::AgentError;
use async_trait::async_trait;
use bytes::Bytes;
use chrono::Utc;
use rsched_core::{Job, RunId, Shell};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, warn};

/// Local executor — single-process job runner.
#[derive(Default, Clone)]
pub struct LocalExecutor;

impl LocalExecutor {
    /// Construct.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Executor for LocalExecutor {
    async fn dispatch(&self, run_id: RunId, job: Job) -> Result<RunHandle, AgentError> {
        let (log_tx, log_rx) = mpsc::channel::<LogChunk>(256);
        let (kill_tx, mut kill_rx) = mpsc::channel::<()>(1);

        let mut child = spawn_child(&job)?;
        let mut stdout = child.stdout.take().expect("stdout piped");
        let mut stderr = child.stderr.take().expect("stderr piped");

        let log_tx_o = log_tx.clone();
        let log_tx_e = log_tx;
        let stdout_task =
            tokio::spawn(async move { stream_pipe(&mut stdout, Stream::Stdout, log_tx_o).await });
        let stderr_task =
            tokio::spawn(async move { stream_pipe(&mut stderr, Stream::Stderr, log_tx_e).await });

        let timeout_secs = job.timeout_secs;
        let outcome = tokio::spawn(async move {
            let timeout_fut = async {
                if timeout_secs > 0 {
                    tokio::time::sleep(Duration::from_secs(timeout_secs)).await;
                    true
                } else {
                    std::future::pending::<bool>().await
                }
            };

            tokio::select! {
                exit = child.wait() => {
                    let status = exit?;
                    let bytes_o = stdout_task.await.unwrap_or(0);
                    let bytes_e = stderr_task.await.unwrap_or(0);
                    Ok(RunOutcome {
                        exit_code: status.code(),
                        timed_out: false,
                        log_bytes: bytes_o + bytes_e,
                        finished_at: Utc::now(),
                    })
                }
                _ = kill_rx.recv() => {
                    debug!(%run_id, "kill requested");
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                    Err(AgentError::Killed)
                }
                _ = timeout_fut => {
                    warn!(%run_id, "timeout exceeded, killing");
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                    let bytes_o = stdout_task.await.unwrap_or(0);
                    let bytes_e = stderr_task.await.unwrap_or(0);
                    Ok(RunOutcome {
                        exit_code: None,
                        timed_out: true,
                        log_bytes: bytes_o + bytes_e,
                        finished_at: Utc::now(),
                    })
                }
            }
        });

        Ok(RunHandle {
            run_id,
            logs: ReceiverStream::new(log_rx),
            outcome,
            kill_tx,
        })
    }
}

fn spawn_child(job: &Job) -> Result<Child, AgentError> {
    let mut cmd = build_command(job);
    if let Some(cwd) = &job.cwd {
        cmd.current_dir(cwd);
    }
    for (k, v) in &job.env {
        cmd.env(k, v);
    }
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.stdin(Stdio::null());
    Ok(cmd.spawn()?)
}

fn build_command(job: &Job) -> Command {
    let shell = match job.shell {
        Shell::Auto => {
            if cfg!(windows) {
                Shell::Cmd
            } else {
                Shell::Sh
            }
        }
        other => other,
    };
    match shell {
        Shell::Cmd => {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(&job.cmd);
            for a in &job.args {
                c.arg(a);
            }
            c
        }
        Shell::Powershell => {
            let mut c = Command::new("powershell");
            c.arg("-NoProfile").arg("-Command").arg(&job.cmd);
            for a in &job.args {
                c.arg(a);
            }
            c
        }
        Shell::Sh => {
            let mut c = Command::new("/bin/sh");
            c.arg("-c").arg(format_shell_line(job));
            c
        }
        Shell::Bash => {
            let mut c = Command::new("bash");
            c.arg("-c").arg(format_shell_line(job));
            c
        }
        Shell::None | Shell::Auto => {
            let mut c = Command::new(&job.cmd);
            for a in &job.args {
                c.arg(a);
            }
            c
        }
    }
}

fn format_shell_line(job: &Job) -> String {
    if job.args.is_empty() {
        job.cmd.clone()
    } else {
        let mut s = job.cmd.clone();
        for a in &job.args {
            s.push(' ');
            s.push_str(a);
        }
        s
    }
}

async fn stream_pipe<R: AsyncReadExt + Unpin>(
    r: &mut R,
    stream: Stream,
    tx: mpsc::Sender<LogChunk>,
) -> u64 {
    let mut buf = [0u8; 8192];
    let mut total = 0u64;
    loop {
        match r.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                total += n as u64;
                let chunk = LogChunk {
                    stream,
                    ts: Utc::now(),
                    bytes: Bytes::copy_from_slice(&buf[..n]),
                };
                if tx.send(chunk).await.is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use rsched_core::{JobBuilder, RunId, Trigger};

    fn cron_trigger() -> Trigger {
        Trigger::Cron {
            expr: "* * * * *".into(),
            timezone: None,
        }
    }

    #[tokio::test]
    async fn runs_echo_to_completion() {
        let exe = LocalExecutor::new();
        let cmd = "echo hello";
        let job = JobBuilder::new("t1", cmd, cron_trigger()).build().unwrap();
        let mut handle = exe.dispatch(RunId::new(), job).await.unwrap();

        // collect logs
        let mut collected = Vec::new();
        while let Some(chunk) = handle.logs.next().await {
            collected.extend(chunk.bytes);
        }
        let outcome = handle.outcome.await.unwrap().unwrap();
        assert_eq!(outcome.exit_code, Some(0));
        let s = String::from_utf8_lossy(&collected);
        assert!(s.contains("hello"), "got: {s}");
    }

    #[tokio::test]
    async fn nonzero_exit_captured() {
        let exe = LocalExecutor::new();
        let cmd = "exit 7";
        let job = JobBuilder::new("t2", cmd, cron_trigger()).build().unwrap();
        let mut handle = exe.dispatch(RunId::new(), job).await.unwrap();
        while handle.logs.next().await.is_some() {}
        let outcome = handle.outcome.await.unwrap().unwrap();
        assert_eq!(outcome.exit_code, Some(7));
    }

    #[tokio::test]
    async fn timeout_kills_long_running() {
        let exe = LocalExecutor::new();
        let cmd = if cfg!(windows) {
            "ping -n 30 127.0.0.1"
        } else {
            "sleep 30"
        };
        let mut job = JobBuilder::new("t3", cmd, cron_trigger()).build().unwrap();
        job.timeout_secs = 1;
        let mut handle = exe.dispatch(RunId::new(), job).await.unwrap();
        while handle.logs.next().await.is_some() {}
        let outcome = handle.outcome.await.unwrap().unwrap();
        assert!(outcome.timed_out, "expected timeout: {:?}", outcome);
    }

    #[tokio::test]
    async fn manual_kill() {
        let exe = LocalExecutor::new();
        let cmd = if cfg!(windows) {
            "ping -n 30 127.0.0.1"
        } else {
            "sleep 30"
        };
        let job = JobBuilder::new("t4", cmd, cron_trigger()).build().unwrap();
        let handle = exe.dispatch(RunId::new(), job).await.unwrap();
        handle.kill_tx.send(()).await.unwrap();
        let result = handle.outcome.await.unwrap();
        assert!(matches!(result, Err(AgentError::Killed)));
    }
}
