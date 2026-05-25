//! Local executor — runs jobs via `tokio::process` on the same host.

use crate::exec::{Executor, LogChunk, RunHandle, RunOutcome, Stream};
use crate::AgentError;
use async_trait::async_trait;
use bytes::Bytes;
use chrono::Utc;
use rsched_core::{Job, RunId, Shell};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdout, Command};
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
        let (signal_tx, mut signal_rx) = mpsc::channel::<i32>(4);

        let is_plugin = matches!(job.shell, Shell::Plugin);
        let plugin_exit: Arc<Mutex<Option<i32>>> = Arc::new(Mutex::new(None));
        let plugin_complete: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));

        let mut child = spawn_child(&job)?;
        let pid = child.id();
        let stdout = child.stdout.take().expect("stdout piped");
        let mut stderr = child.stderr.take().expect("stderr piped");

        // Plugin: write a single JSON line to stdin then close.
        if is_plugin {
            if let Some(mut stdin) = child.stdin.take() {
                let payload = build_plugin_stdin(run_id, &job);
                tokio::spawn(async move {
                    if let Err(e) = stdin.write_all(payload.as_bytes()).await {
                        warn!(%run_id, error = %e, "plugin stdin write failed");
                    }
                    let _ = stdin.shutdown().await;
                });
            }
        }

        let log_tx_o = log_tx.clone();
        let log_tx_e = log_tx;
        let stdout_task = if is_plugin {
            let exit_slot = plugin_exit.clone();
            let complete_slot = plugin_complete.clone();
            tokio::spawn(async move {
                stream_plugin_stdout(stdout, log_tx_o, exit_slot, complete_slot).await
            })
        } else {
            let mut stdout = stdout;
            tokio::spawn(async move { stream_pipe(&mut stdout, Stream::Stdout, log_tx_o).await })
        };
        let stderr_task =
            tokio::spawn(async move { stream_pipe(&mut stderr, Stream::Stderr, log_tx_e).await });

        // Forward signal requests to libc::kill in a side task — keeps the
        // outcome `select!` clean and lets us send multiple signals over the
        // lifetime of a run.
        tokio::spawn(async move {
            while let Some(s) = signal_rx.recv().await {
                send_unix_signal(pid, s, run_id);
            }
        });

        let timeout_secs = job.timeout_secs;
        let plugin_exit_o = plugin_exit.clone();
        let plugin_complete_o = plugin_complete.clone();
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
                    let (rss, cu, cs) = capture_rusage_children();
                    let exit_code = if is_plugin {
                        let completed = *plugin_complete_o.lock().unwrap();
                        if completed {
                            *plugin_exit_o.lock().unwrap()
                        } else {
                            // No `complete` event — surface as failure. Prefer
                            // the process exit code; fall back to a synthetic
                            // non-zero so the scheduler treats this as Failed.
                            Some(status.code().filter(|c| *c != 0).unwrap_or(-1))
                        }
                    } else {
                        status.code()
                    };
                    Ok(RunOutcome {
                        exit_code,
                        timed_out: false,
                        log_bytes: bytes_o + bytes_e,
                        finished_at: Utc::now(),
                        peak_rss_bytes: rss,
                        cpu_user_secs: cu,
                        cpu_sys_secs: cs,
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
                    let (rss, cu, cs) = capture_rusage_children();
                    Ok(RunOutcome {
                        exit_code: None,
                        timed_out: true,
                        log_bytes: bytes_o + bytes_e,
                        finished_at: Utc::now(),
                        peak_rss_bytes: rss,
                        cpu_user_secs: cu,
                        cpu_sys_secs: cs,
                    })
                }
            }
        });

        Ok(RunHandle {
            run_id,
            logs: ReceiverStream::new(log_rx),
            outcome,
            kill_tx,
            signal_tx,
        })
    }
}

/// Build the JSON payload written to a Cronicle-compatible plugin's stdin.
///
/// Single line: `{"id":"<run_id>","params":<env-as-object>}\n`.
pub(crate) fn build_plugin_stdin(run_id: RunId, job: &Job) -> String {
    let params: serde_json::Map<String, serde_json::Value> = job
        .env
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect();
    let payload = serde_json::json!({
        "id": run_id.to_string(),
        "params": serde_json::Value::Object(params),
    });
    let mut s = payload.to_string();
    s.push('\n');
    s
}

/// Returns true if the JSON value carries at least one recognized
/// Cronicle-plugin event key (`progress`, `perf`, `complete`, `description`).
pub(crate) fn is_plugin_event(v: &serde_json::Value) -> bool {
    if let Some(obj) = v.as_object() {
        return obj.contains_key("progress")
            || obj.contains_key("perf")
            || obj.contains_key("complete")
            || obj.contains_key("description");
    }
    false
}

/// Read plugin stdout line-by-line. JSON lines with recognized keys emit
/// `Stream::Plugin` chunks and may set the run's exit code via a
/// `{"complete":1,"code":N}` event. Non-JSON / unrecognized lines fall
/// through to `Stream::Stdout`.
async fn stream_plugin_stdout(
    stdout: ChildStdout,
    tx: mpsc::Sender<LogChunk>,
    exit_slot: Arc<Mutex<Option<i32>>>,
    complete_slot: Arc<Mutex<bool>>,
) -> u64 {
    let mut total: u64 = 0;
    let mut reader = BufReader::new(stdout).lines();
    loop {
        match reader.next_line().await {
            Ok(Some(line)) => {
                total += line.len() as u64 + 1; // include \n
                let trimmed = line.trim();
                let parsed: Option<serde_json::Value> = if trimmed.starts_with('{') {
                    serde_json::from_str(trimmed).ok()
                } else {
                    None
                };
                let stream = match parsed.as_ref() {
                    Some(v) if is_plugin_event(v) => {
                        if let Some(obj) = v.as_object() {
                            if let Some(c) = obj.get("complete") {
                                let is_complete = match c {
                                    serde_json::Value::Bool(b) => *b,
                                    serde_json::Value::Number(n) => {
                                        n.as_i64().map(|i| i != 0).unwrap_or(false)
                                    }
                                    _ => false,
                                };
                                if is_complete {
                                    *complete_slot.lock().unwrap() = true;
                                    if let Some(code_v) = obj.get("code") {
                                        if let Some(n) = code_v.as_i64() {
                                            *exit_slot.lock().unwrap() = Some(n as i32);
                                        }
                                    } else {
                                        // `complete` without `code` => success.
                                        let mut slot = exit_slot.lock().unwrap();
                                        if slot.is_none() {
                                            *slot = Some(0);
                                        }
                                    }
                                }
                            }
                        }
                        Stream::Plugin
                    }
                    _ => Stream::Stdout,
                };
                let mut bytes = line.into_bytes();
                bytes.push(b'\n');
                let chunk = LogChunk {
                    stream,
                    ts: Utc::now(),
                    bytes: Bytes::from(bytes),
                };
                if tx.send(chunk).await.is_err() {
                    break;
                }
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }
    total
}

/// Capture `getrusage(RUSAGE_CHILDREN)` peak RSS + CPU times.
///
/// Returns `(peak_rss_bytes, user_cpu_secs, sys_cpu_secs)` on unix; all
/// `None` on Windows. NOTE: RUSAGE_CHILDREN reports the cumulative usage of
/// **every** child that has been reaped — when the executor runs multiple
/// jobs concurrently in one process, the numbers attributed to a single run
/// may include other reaped children. Accurate enough for trend analysis,
/// not for billing.
fn capture_rusage_children() -> (Option<u64>, Option<f64>, Option<f64>) {
    #[cfg(unix)]
    {
        let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::getrusage(libc::RUSAGE_CHILDREN, &mut ru) };
        if rc != 0 {
            return (None, None, None);
        }
        // ru_maxrss is kilobytes on Linux, bytes on macOS.
        let rss_kib = ru.ru_maxrss as i64;
        let rss_bytes = if cfg!(target_os = "macos") {
            rss_kib as u64
        } else {
            (rss_kib as u64).saturating_mul(1024)
        };
        let user = ru.ru_utime.tv_sec as f64 + ru.ru_utime.tv_usec as f64 / 1_000_000.0;
        let sys = ru.ru_stime.tv_sec as f64 + ru.ru_stime.tv_usec as f64 / 1_000_000.0;
        (Some(rss_bytes), Some(user), Some(sys))
    }
    #[cfg(not(unix))]
    {
        (None, None, None)
    }
}

/// Send a unix signal to a child PID. No-op on Windows (logs a warning).
fn send_unix_signal(pid: Option<u32>, sig: i32, run_id: RunId) {
    #[cfg(unix)]
    {
        if let Some(pid) = pid {
            // SAFETY: pid is an OS-issued PID; kill(2) is safe for any pid.
            let rc = unsafe { libc::kill(pid as i32, sig) };
            if rc != 0 {
                warn!(%run_id, %sig, errno = std::io::Error::last_os_error().raw_os_error(), "kill(2) failed");
            } else {
                debug!(%run_id, %sig, "signal sent");
            }
        } else {
            warn!(%run_id, "cannot signal: pid unavailable");
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        let _ = sig;
        warn!(%run_id, "SEND_SIGNAL is not supported on this platform");
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
    // Plugin shell needs an open stdin to receive the JSON payload.
    if matches!(job.shell, Shell::Plugin) {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }
    // On Linux, exec returns ETXTBSY ("Text file busy") if the target executable
    // still has any open writer in the system — including a sibling thread that
    // just finished writing the file. We briefly retry to absorb this race.
    let mut last_err = None;
    for _ in 0..10 {
        match cmd.spawn() {
            Ok(c) => return Ok(c),
            Err(e) if e.raw_os_error() == Some(26) => {
                last_err = Some(e);
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(e.into()),
        }
    }
    Err(last_err.unwrap().into())
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
        Shell::None | Shell::Auto | Shell::Plugin => {
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

    // ---- Plugin shell tests (v0.7.4 Cronicle-compatible plugin host) ----

    #[test]
    fn plugin_shell_builds_command_directly() {
        // Shell::Plugin must spawn `cmd` directly without any shell wrapping.
        let mut job = JobBuilder::new("p1", "/usr/bin/plugin", cron_trigger())
            .build()
            .unwrap();
        job.shell = Shell::Plugin;
        job.args = vec!["--flag".into()];
        let c = build_command(&job);
        let prog = c.as_std().get_program().to_string_lossy().to_string();
        assert_eq!(prog, "/usr/bin/plugin");
        let args: Vec<String> = c
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(args, vec!["--flag"]);
    }

    #[test]
    fn plugin_stdin_payload_has_id_and_params() {
        let mut job = JobBuilder::new("p2", "/bin/true", cron_trigger())
            .build()
            .unwrap();
        job.shell = Shell::Plugin;
        job.env.insert("FOO".into(), "bar".into());
        let run_id = RunId::new();
        let s = build_plugin_stdin(run_id, &job);
        assert!(s.ends_with('\n'));
        let v: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
        assert_eq!(v["id"], run_id.to_string());
        assert_eq!(v["params"]["FOO"], "bar");
    }

    #[test]
    fn is_plugin_event_recognizes_all_four_keys() {
        let progress: serde_json::Value = serde_json::from_str(r#"{"progress":0.5}"#).unwrap();
        let perf: serde_json::Value = serde_json::from_str(r#"{"perf":"db=1.2"}"#).unwrap();
        let complete: serde_json::Value =
            serde_json::from_str(r#"{"complete":1,"code":0}"#).unwrap();
        let desc: serde_json::Value = serde_json::from_str(r#"{"description":"hi"}"#).unwrap();
        assert!(is_plugin_event(&progress));
        assert!(is_plugin_event(&perf));
        assert!(is_plugin_event(&complete));
        assert!(is_plugin_event(&desc));

        let plain: serde_json::Value = serde_json::from_str(r#"{"foo":"bar"}"#).unwrap();
        assert!(!is_plugin_event(&plain));
    }

    #[cfg(unix)]
    fn write_plugin_script(name: &str, body: &str) -> std::path::PathBuf {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        // Per-thread + nanos suffix avoids any collision between parallel tests
        // sharing tmp dir. On Linux, exec() returns ETXTBSY ("Text file busy")
        // if any process still holds a writable fd to the path — sync_all + drop
        // before set_permissions guarantees the write fd is fully closed.
        let tid = format!("{:?}", std::thread::current().id());
        let path = std::env::temp_dir().join(format!(
            "rsched-{name}-{}-{}-{}.sh",
            std::process::id(),
            tid.replace([' ', '(', ')'], "_"),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&path)
                .unwrap();
            f.write_all(format!("#!/bin/sh\nread _ignored\n{body}\n").as_bytes())
                .unwrap();
            f.sync_all().unwrap();
        }
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        path
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn plugin_run_streams_events_and_uses_complete_code() {
        // Emits a progress line + complete with code=3. Exit code MUST be 3.
        let exe = LocalExecutor::new();
        let path = write_plugin_script(
            "plugin-cc",
            r#"echo '{"progress":0.5,"description":"halfway"}'
echo '{"complete":1,"code":3,"description":"done"}'"#,
        );

        let mut job = JobBuilder::new("p_run", path.to_string_lossy().to_string(), cron_trigger())
            .build()
            .unwrap();
        job.shell = Shell::Plugin;
        let mut handle = exe.dispatch(RunId::new(), job).await.unwrap();
        let mut plugin_seen = 0usize;
        let mut stdout_seen = 0usize;
        while let Some(chunk) = handle.logs.next().await {
            match chunk.stream {
                Stream::Plugin => plugin_seen += 1,
                Stream::Stdout => stdout_seen += 1,
                Stream::Stderr => {}
            }
        }
        let outcome = handle.outcome.await.unwrap().unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(outcome.exit_code, Some(3), "plugin code should override");
        assert!(plugin_seen >= 2, "got plugin lines: {plugin_seen}");
        assert_eq!(stdout_seen, 0, "no non-JSON lines expected");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn plugin_non_json_falls_through_to_stdout() {
        let exe = LocalExecutor::new();
        let path = write_plugin_script(
            "plugin-pass",
            "echo 'hello world'\necho '{\"complete\":1,\"code\":0}'",
        );

        let mut job = JobBuilder::new("p_pass", path.to_string_lossy().to_string(), cron_trigger())
            .build()
            .unwrap();
        job.shell = Shell::Plugin;
        let mut handle = exe.dispatch(RunId::new(), job).await.unwrap();
        let mut plugin_seen = 0usize;
        let mut stdout_seen = 0usize;
        while let Some(chunk) = handle.logs.next().await {
            match chunk.stream {
                Stream::Plugin => plugin_seen += 1,
                Stream::Stdout => stdout_seen += 1,
                Stream::Stderr => {}
            }
        }
        let outcome = handle.outcome.await.unwrap().unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(outcome.exit_code, Some(0));
        assert!(plugin_seen >= 1);
        assert!(stdout_seen >= 1, "non-JSON line must fall through");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn plugin_without_complete_event_is_failure() {
        // Plugin exits 0 but never sends `complete` — must be marked failure.
        let exe = LocalExecutor::new();
        let path = write_plugin_script("plugin-nocomp", "echo '{\"progress\":0.5}'\nexit 0");

        let mut job = JobBuilder::new(
            "p_nocomp",
            path.to_string_lossy().to_string(),
            cron_trigger(),
        )
        .build()
        .unwrap();
        job.shell = Shell::Plugin;
        let mut handle = exe.dispatch(RunId::new(), job).await.unwrap();
        while handle.logs.next().await.is_some() {}
        let outcome = handle.outcome.await.unwrap().unwrap();
        let _ = std::fs::remove_file(&path);
        assert_ne!(
            outcome.exit_code,
            Some(0),
            "missing complete must NOT be success"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn plugin_complete_without_code_defaults_to_zero() {
        let exe = LocalExecutor::new();
        let path = write_plugin_script(
            "plugin-nocode",
            "echo '{\"complete\":1,\"description\":\"ok\"}'",
        );

        let mut job = JobBuilder::new(
            "p_nocode",
            path.to_string_lossy().to_string(),
            cron_trigger(),
        )
        .build()
        .unwrap();
        job.shell = Shell::Plugin;
        let mut handle = exe.dispatch(RunId::new(), job).await.unwrap();
        while handle.logs.next().await.is_some() {}
        let outcome = handle.outcome.await.unwrap().unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(outcome.exit_code, Some(0));
    }
}
