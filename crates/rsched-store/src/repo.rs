//! High-level repositories. One `Store` wraps the pool and exposes the four
//! domain repos.

use crate::StoreError;
use chrono::{DateTime, Utc};
use rsched_core::{AgentId, Calendar, CalendarId, Job, JobId, Run, RunId, RunState, TriggerKind};
use sqlx::{Row, SqlitePool};

/// Wraps a pool + provides repo accessors.
#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Wrap an existing pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Run embedded migrations to head.
    pub async fn migrate(&self) -> Result<(), StoreError> {
        crate::MIGRATOR.run(&self.pool).await?;
        Ok(())
    }

    /// Borrow underlying pool (for transactions / advanced use).
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Job repository.
    pub fn jobs(&self) -> JobRepo<'_> {
        JobRepo { pool: &self.pool }
    }
    /// Run repository.
    pub fn runs(&self) -> RunRepo<'_> {
        RunRepo { pool: &self.pool }
    }
    /// Calendar repository.
    pub fn calendars(&self) -> CalendarRepo<'_> {
        CalendarRepo { pool: &self.pool }
    }
    /// Agent repository.
    pub fn agents(&self) -> AgentRepo<'_> {
        AgentRepo { pool: &self.pool }
    }
    /// Run-log repository.
    pub fn run_logs(&self) -> RunLogRepo<'_> {
        RunLogRepo { pool: &self.pool }
    }
}

fn trigger_kind_str(k: TriggerKind) -> &'static str {
    match k {
        TriggerKind::Cron => "cron",
        TriggerKind::Interval => "interval",
        TriggerKind::OneShot => "one_shot",
        TriggerKind::Dep => "dep",
        TriggerKind::File => "file",
        TriggerKind::Webhook => "webhook",
        TriggerKind::Manual => "manual",
    }
}

fn run_state_str(s: RunState) -> &'static str {
    match s {
        RunState::Queued => "queued",
        RunState::Running => "running",
        RunState::Success => "success",
        RunState::Failed => "failed",
        RunState::Killed => "killed",
        RunState::Skipped => "skipped",
        RunState::Lost => "lost",
    }
}

fn run_state_from(s: &str) -> RunState {
    match s {
        "queued" => RunState::Queued,
        "running" => RunState::Running,
        "success" => RunState::Success,
        "failed" => RunState::Failed,
        "killed" => RunState::Killed,
        "skipped" => RunState::Skipped,
        _ => RunState::Lost,
    }
}

/// Repository for [`Job`].
pub struct JobRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> JobRepo<'a> {
    /// Insert a job (validates first).
    pub async fn insert(&self, job: &Job) -> Result<(), StoreError> {
        job.validate()?;
        let id = job.id.to_string();
        let box_id = job.box_id.as_ref().map(|b| b.to_string());
        let trigger_kind = trigger_kind_str(job.trigger.kind());
        let trigger_json = serde_json::to_string(&job.trigger)?;
        let args_json = serde_json::to_string(&job.args)?;
        let env_json = serde_json::to_string(&job.env)?;
        let target_json = serde_json::to_string(&job.target)?;
        let retry_json = serde_json::to_string(&job.retry)?;
        let alert_json = serde_json::to_string(&job.alerts)?;
        let shell = serde_json::to_value(job.shell)?
            .as_str()
            .unwrap_or("auto")
            .to_string();
        let misfire = serde_json::to_value(job.misfire)?
            .as_str()
            .unwrap_or("fire_once")
            .to_string();
        let timeout = job.timeout_secs as i64;
        let sla = job.sla_secs as i64;
        let calendar_id = job.calendar_id.as_ref().map(|c| c.to_string());
        let paused = job.paused as i64;
        let created = job.created_at.to_rfc3339();
        let updated = job.updated_at.to_rfc3339();
        let next_fire = job.next_fire_at.map(|t| t.to_rfc3339());

        sqlx::query(
            r#"INSERT INTO jobs
            (id, name, box_id, trigger_kind, trigger_data_json, cmd, args_json, env_json,
             cwd, shell, target_json, retry_json, timeout_secs, sla_secs, calendar_id,
             misfire_policy, paused, alert_config_json, created_at, updated_at, next_fire_at)
            VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"#,
        )
        .bind(id)
        .bind(&job.name)
        .bind(box_id)
        .bind(trigger_kind)
        .bind(trigger_json)
        .bind(&job.cmd)
        .bind(args_json)
        .bind(env_json)
        .bind(&job.cwd)
        .bind(shell)
        .bind(target_json)
        .bind(retry_json)
        .bind(timeout)
        .bind(sla)
        .bind(calendar_id)
        .bind(misfire)
        .bind(paused)
        .bind(alert_json)
        .bind(created)
        .bind(updated)
        .bind(next_fire)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Fetch a job by id.
    pub async fn get(&self, id: JobId) -> Result<Job, StoreError> {
        let row = sqlx::query("SELECT * FROM jobs WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(self.pool)
            .await?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))?;
        row_to_job(&row)
    }

    /// Fetch a job by name.
    pub async fn get_by_name(&self, name: &str) -> Result<Job, StoreError> {
        let row = sqlx::query("SELECT * FROM jobs WHERE name = ?")
            .bind(name)
            .fetch_optional(self.pool)
            .await?
            .ok_or_else(|| StoreError::NotFound(name.into()))?;
        row_to_job(&row)
    }

    /// List all jobs.
    pub async fn list(&self) -> Result<Vec<Job>, StoreError> {
        let rows = sqlx::query("SELECT * FROM jobs ORDER BY name")
            .fetch_all(self.pool)
            .await?;
        rows.iter().map(row_to_job).collect()
    }

    /// Jobs whose `next_fire_at <= now` and not paused (uses partial index).
    pub async fn due(&self, now: DateTime<Utc>) -> Result<Vec<Job>, StoreError> {
        let rows = sqlx::query(
            "SELECT * FROM jobs
             WHERE paused = 0 AND next_fire_at IS NOT NULL AND next_fire_at <= ?
             ORDER BY next_fire_at",
        )
        .bind(now.to_rfc3339())
        .fetch_all(self.pool)
        .await?;
        rows.iter().map(row_to_job).collect()
    }

    /// Update `next_fire_at` for a job.
    pub async fn set_next_fire(
        &self,
        id: JobId,
        next: Option<DateTime<Utc>>,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE jobs SET next_fire_at = ?, updated_at = ? WHERE id = ?")
            .bind(next.map(|t| t.to_rfc3339()))
            .bind(Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(self.pool)
            .await?;
        Ok(())
    }

    /// Pause or resume.
    pub async fn set_paused(&self, id: JobId, paused: bool) -> Result<(), StoreError> {
        sqlx::query("UPDATE jobs SET paused = ?, updated_at = ? WHERE id = ?")
            .bind(paused as i64)
            .bind(Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(self.pool)
            .await?;
        Ok(())
    }

    /// Delete a job.
    pub async fn delete(&self, id: JobId) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM jobs WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await?;
        Ok(())
    }

    /// Replace a job's mutable fields (everything except id + created_at).
    pub async fn update(&self, job: &Job) -> Result<(), StoreError> {
        job.validate()?;
        let trigger_kind = trigger_kind_str(job.trigger.kind());
        let trigger_json = serde_json::to_string(&job.trigger)?;
        let args_json = serde_json::to_string(&job.args)?;
        let env_json = serde_json::to_string(&job.env)?;
        let target_json = serde_json::to_string(&job.target)?;
        let retry_json = serde_json::to_string(&job.retry)?;
        let alert_json = serde_json::to_string(&job.alerts)?;
        let shell = serde_json::to_value(job.shell)?
            .as_str()
            .unwrap_or("auto")
            .to_string();
        let misfire = serde_json::to_value(job.misfire)?
            .as_str()
            .unwrap_or("fire_once")
            .to_string();
        let box_id = job.box_id.as_ref().map(|b| b.to_string());
        let calendar_id = job.calendar_id.as_ref().map(|c| c.to_string());
        let timeout = job.timeout_secs as i64;
        let sla = job.sla_secs as i64;
        let paused = job.paused as i64;
        let updated = Utc::now().to_rfc3339();
        let next_fire = job.next_fire_at.map(|t| t.to_rfc3339());

        sqlx::query(
            r#"UPDATE jobs SET
                name = ?, box_id = ?, trigger_kind = ?, trigger_data_json = ?,
                cmd = ?, args_json = ?, env_json = ?, cwd = ?, shell = ?,
                target_json = ?, retry_json = ?, timeout_secs = ?, sla_secs = ?,
                calendar_id = ?, misfire_policy = ?, paused = ?,
                alert_config_json = ?, updated_at = ?, next_fire_at = ?
              WHERE id = ?"#,
        )
        .bind(&job.name)
        .bind(box_id)
        .bind(trigger_kind)
        .bind(trigger_json)
        .bind(&job.cmd)
        .bind(args_json)
        .bind(env_json)
        .bind(&job.cwd)
        .bind(shell)
        .bind(target_json)
        .bind(retry_json)
        .bind(timeout)
        .bind(sla)
        .bind(calendar_id)
        .bind(misfire)
        .bind(paused)
        .bind(alert_json)
        .bind(updated)
        .bind(next_fire)
        .bind(job.id.to_string())
        .execute(self.pool)
        .await?;
        Ok(())
    }
}

fn row_to_job(row: &sqlx::sqlite::SqliteRow) -> Result<Job, StoreError> {
    use std::collections::HashMap;
    let id: String = row.try_get("id")?;
    let id: JobId = id
        .parse()
        .map_err(|e: ulid::DecodeError| StoreError::NotFound(format!("bad job id: {e}")))?;
    let name: String = row.try_get("name")?;
    let box_id_str: Option<String> = row.try_get("box_id")?;
    let box_id = box_id_str
        .as_deref()
        .map(|s| s.parse())
        .transpose()
        .map_err(|e: ulid::DecodeError| StoreError::NotFound(format!("bad box id: {e}")))?;
    let trigger_json: String = row.try_get("trigger_data_json")?;
    let trigger = serde_json::from_str(&trigger_json)?;
    let cmd: String = row.try_get("cmd")?;
    let args_json: String = row.try_get("args_json")?;
    let args: Vec<String> = serde_json::from_str(&args_json)?;
    let env_json: String = row.try_get("env_json")?;
    let env: HashMap<String, String> = serde_json::from_str(&env_json)?;
    let cwd: Option<String> = row.try_get("cwd")?;
    let shell_str: String = row.try_get("shell")?;
    let shell = serde_json::from_value(serde_json::Value::String(shell_str))?;
    let target_json: String = row.try_get("target_json")?;
    let target = serde_json::from_str(&target_json)?;
    let retry_json: String = row.try_get("retry_json")?;
    let retry = serde_json::from_str(&retry_json)?;
    let timeout_secs: i64 = row.try_get("timeout_secs")?;
    let sla_secs: i64 = row.try_get("sla_secs")?;
    let calendar_id_str: Option<String> = row.try_get("calendar_id")?;
    let calendar_id = calendar_id_str
        .as_deref()
        .map(|s| s.parse())
        .transpose()
        .map_err(|e: ulid::DecodeError| StoreError::NotFound(format!("bad cal id: {e}")))?;
    let misfire_str: String = row.try_get("misfire_policy")?;
    let misfire = serde_json::from_value(serde_json::Value::String(misfire_str))?;
    let paused: i64 = row.try_get("paused")?;
    let alerts_json: String = row.try_get("alert_config_json")?;
    let alerts = serde_json::from_str(&alerts_json)?;
    let created_at: String = row.try_get("created_at")?;
    let updated_at: String = row.try_get("updated_at")?;
    let next_fire_at_str: Option<String> = row.try_get("next_fire_at")?;

    let created_at = DateTime::parse_from_rfc3339(&created_at)
        .map_err(|e| StoreError::NotFound(format!("bad ts: {e}")))?
        .with_timezone(&Utc);
    let updated_at = DateTime::parse_from_rfc3339(&updated_at)
        .map_err(|e| StoreError::NotFound(format!("bad ts: {e}")))?
        .with_timezone(&Utc);
    let next_fire_at = next_fire_at_str
        .as_deref()
        .map(|s| DateTime::parse_from_rfc3339(s).map(|t| t.with_timezone(&Utc)))
        .transpose()
        .map_err(|e| StoreError::NotFound(format!("bad ts: {e}")))?;

    Ok(Job {
        id,
        name,
        box_id,
        trigger,
        cmd,
        args,
        env,
        cwd,
        shell,
        target,
        retry,
        timeout_secs: timeout_secs as u64,
        sla_secs: sla_secs as u64,
        calendar_id,
        misfire,
        dependencies: Vec::new(), // loaded separately via dep table when needed
        paused: paused != 0,
        alerts,
        created_at,
        updated_at,
        next_fire_at,
    })
}

/// Repository for [`Run`].
pub struct RunRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> RunRepo<'a> {
    /// Insert a new run.
    pub async fn insert(&self, run: &Run) -> Result<(), StoreError> {
        sqlx::query(
            r#"INSERT INTO runs
            (id, job_id, agent_id, state, attempt, queued_at, started_at, finished_at,
             exit_code, parent_run_ids_json, log_truncated, log_bytes)
            VALUES (?,?,?,?,?,?,?,?,?,?,?,?)"#,
        )
        .bind(run.id.to_string())
        .bind(run.job_id.to_string())
        .bind(run.agent_id.as_ref().map(|a| a.to_string()))
        .bind(run_state_str(run.state))
        .bind(run.attempt as i64)
        .bind(run.queued_at.to_rfc3339())
        .bind(run.started_at.map(|t| t.to_rfc3339()))
        .bind(run.finished_at.map(|t| t.to_rfc3339()))
        .bind(run.exit_code.map(|c| c as i64))
        .bind(serde_json::to_string(&run.parent_run_ids)?)
        .bind(run.log_truncated as i64)
        .bind(run.log_bytes as i64)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Update a run's mutable fields.
    pub async fn update(&self, run: &Run) -> Result<(), StoreError> {
        sqlx::query(
            r#"UPDATE runs SET
              agent_id = ?, state = ?, started_at = ?, finished_at = ?,
              exit_code = ?, log_truncated = ?, log_bytes = ?
              WHERE id = ?"#,
        )
        .bind(run.agent_id.as_ref().map(|a| a.to_string()))
        .bind(run_state_str(run.state))
        .bind(run.started_at.map(|t| t.to_rfc3339()))
        .bind(run.finished_at.map(|t| t.to_rfc3339()))
        .bind(run.exit_code.map(|c| c as i64))
        .bind(run.log_truncated as i64)
        .bind(run.log_bytes as i64)
        .bind(run.id.to_string())
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Fetch run by id.
    pub async fn get(&self, id: RunId) -> Result<Run, StoreError> {
        let row = sqlx::query("SELECT * FROM runs WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(self.pool)
            .await?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))?;
        row_to_run(&row)
    }

    /// List most recent runs for a job (newest first).
    pub async fn list_for_job(&self, job_id: JobId, limit: i64) -> Result<Vec<Run>, StoreError> {
        let rows =
            sqlx::query("SELECT * FROM runs WHERE job_id = ? ORDER BY queued_at DESC LIMIT ?")
                .bind(job_id.to_string())
                .bind(limit)
                .fetch_all(self.pool)
                .await?;
        rows.iter().map(row_to_run).collect()
    }

    /// List runs currently in non-terminal states.
    pub async fn list_active(&self) -> Result<Vec<Run>, StoreError> {
        let rows = sqlx::query("SELECT * FROM runs WHERE state IN ('queued','running')")
            .fetch_all(self.pool)
            .await?;
        rows.iter().map(row_to_run).collect()
    }
}

fn row_to_run(row: &sqlx::sqlite::SqliteRow) -> Result<Run, StoreError> {
    let id: String = row.try_get("id")?;
    let id: RunId = id
        .parse()
        .map_err(|e: ulid::DecodeError| StoreError::NotFound(format!("bad run id: {e}")))?;
    let job_id: String = row.try_get("job_id")?;
    let job_id: JobId = job_id
        .parse()
        .map_err(|e: ulid::DecodeError| StoreError::NotFound(format!("bad job id: {e}")))?;
    let agent_id_str: Option<String> = row.try_get("agent_id")?;
    let agent_id = agent_id_str
        .as_deref()
        .map(|s| s.parse())
        .transpose()
        .map_err(|e: ulid::DecodeError| StoreError::NotFound(format!("bad agent id: {e}")))?;
    let state_str: String = row.try_get("state")?;
    let attempt: i64 = row.try_get("attempt")?;
    let queued_at: String = row.try_get("queued_at")?;
    let started_at: Option<String> = row.try_get("started_at")?;
    let finished_at: Option<String> = row.try_get("finished_at")?;
    let exit_code: Option<i64> = row.try_get("exit_code")?;
    let parent_json: String = row.try_get("parent_run_ids_json")?;
    let parent_run_ids: Vec<RunId> = serde_json::from_str(&parent_json)?;
    let log_truncated: i64 = row.try_get("log_truncated")?;
    let log_bytes: i64 = row.try_get("log_bytes")?;

    let parse_ts = |s: &str| -> Result<DateTime<Utc>, StoreError> {
        DateTime::parse_from_rfc3339(s)
            .map(|t| t.with_timezone(&Utc))
            .map_err(|e| StoreError::NotFound(format!("bad ts: {e}")))
    };

    Ok(Run {
        id,
        job_id,
        agent_id,
        state: run_state_from(&state_str),
        attempt: attempt as u32,
        queued_at: parse_ts(&queued_at)?,
        started_at: started_at.as_deref().map(parse_ts).transpose()?,
        finished_at: finished_at.as_deref().map(parse_ts).transpose()?,
        exit_code: exit_code.map(|c| c as i32),
        parent_run_ids,
        log_truncated: log_truncated != 0,
        log_bytes: log_bytes as u64,
    })
}

/// Repository for [`Calendar`].
pub struct CalendarRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> CalendarRepo<'a> {
    /// Insert calendar.
    pub async fn insert(&self, cal: &Calendar) -> Result<(), StoreError> {
        cal.validate()?;
        sqlx::query("INSERT INTO calendars (id, name, definition_json) VALUES (?,?,?)")
            .bind(cal.id.to_string())
            .bind(&cal.name)
            .bind(serde_json::to_string(&cal.rules)?)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    /// Fetch by id.
    pub async fn get(&self, id: CalendarId) -> Result<Calendar, StoreError> {
        let row = sqlx::query("SELECT id, name, definition_json FROM calendars WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(self.pool)
            .await?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))?;
        let id: String = row.try_get("id")?;
        let id: CalendarId = id
            .parse()
            .map_err(|e: ulid::DecodeError| StoreError::NotFound(format!("bad cal id: {e}")))?;
        let name: String = row.try_get("name")?;
        let def_json: String = row.try_get("definition_json")?;
        let rules = serde_json::from_str(&def_json)?;
        Ok(Calendar { id, name, rules })
    }
}

/// Repository for agents.
pub struct AgentRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> AgentRepo<'a> {
    /// Register or update an agent (upsert on cert fingerprint).
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert(
        &self,
        id: AgentId,
        hostname: &str,
        cert_fingerprint: &str,
        tags: &[String],
        version: Option<&str>,
        os: Option<&str>,
        arch: Option<&str>,
    ) -> Result<(), StoreError> {
        sqlx::query(
            r#"INSERT INTO agents (id, hostname, cert_fingerprint, tags_json, last_seen,
                                   state, version, os, arch)
               VALUES (?, ?, ?, ?, ?, 'online', ?, ?, ?)
               ON CONFLICT(cert_fingerprint) DO UPDATE SET
                 hostname = excluded.hostname,
                 tags_json = excluded.tags_json,
                 last_seen = excluded.last_seen,
                 state = 'online',
                 version = excluded.version,
                 os = excluded.os,
                 arch = excluded.arch"#,
        )
        .bind(id.to_string())
        .bind(hostname)
        .bind(cert_fingerprint)
        .bind(serde_json::to_string(tags)?)
        .bind(Utc::now().to_rfc3339())
        .bind(version)
        .bind(os)
        .bind(arch)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Count registered agents.
    pub async fn count(&self) -> Result<i64, StoreError> {
        let row = sqlx::query("SELECT COUNT(*) AS n FROM agents")
            .fetch_one(self.pool)
            .await?;
        Ok(row.try_get("n")?)
    }
}

/// A single persisted log chunk from a run.
#[derive(Debug)]
pub struct LogRow {
    /// Monotone sequence number within the run (0-based).
    pub seq: i64,
    /// RFC-3339 timestamp when the chunk was captured.
    pub ts: String,
    /// `"stdout"` or `"stderr"`.
    pub stream: String,
    /// Raw bytes from the process output.
    pub chunk: Vec<u8>,
}

/// Repository for run-log chunks stored in `run_logs`.
pub struct RunLogRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> RunLogRepo<'a> {
    /// Append one log chunk. Silently ignores duplicate `(run_id, seq)` pairs.
    pub async fn append(
        &self,
        run_id: &str,
        seq: i64,
        ts: &str,
        stream: &str,
        chunk: &[u8],
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT OR IGNORE INTO run_logs (run_id, seq, ts, stream, chunk) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(run_id)
        .bind(seq)
        .bind(ts)
        .bind(stream)
        .bind(chunk)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Fetch log chunks ordered by `seq`, optionally starting from `from_seq`.
    pub async fn fetch(
        &self,
        run_id: &str,
        from_seq: Option<i64>,
        limit: i64,
    ) -> Result<Vec<LogRow>, StoreError> {
        let rows = if let Some(start) = from_seq {
            sqlx::query(
                "SELECT seq, ts, stream, chunk FROM run_logs \
                 WHERE run_id = ? AND seq >= ? ORDER BY seq LIMIT ?",
            )
            .bind(run_id)
            .bind(start)
            .bind(limit)
            .fetch_all(self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT seq, ts, stream, chunk FROM run_logs \
                 WHERE run_id = ? ORDER BY seq LIMIT ?",
            )
            .bind(run_id)
            .bind(limit)
            .fetch_all(self.pool)
            .await?
        };
        rows.into_iter()
            .map(|r| {
                Ok(LogRow {
                    seq: r.try_get("seq")?,
                    ts: r.try_get("ts")?,
                    stream: r.try_get("stream")?,
                    chunk: r.try_get("chunk")?,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsched_core::{JobBuilder, Trigger};

    async fn fresh_store() -> Store {
        let pool = crate::open_memory().await.unwrap();
        let store = Store::new(pool);
        store.migrate().await.unwrap();
        store
    }

    fn cron_trigger() -> Trigger {
        Trigger::Cron {
            expr: "*/5 * * * *".into(),
            timezone: None,
        }
    }

    #[tokio::test]
    async fn migrate_idempotent() {
        let store = fresh_store().await;
        // run twice
        store.migrate().await.unwrap();
    }

    #[tokio::test]
    async fn job_insert_and_get() {
        let store = fresh_store().await;
        let job = JobBuilder::new("my-job", "echo hi", cron_trigger())
            .timeout(60)
            .build()
            .unwrap();
        store.jobs().insert(&job).await.unwrap();
        let back = store.jobs().get(job.id).await.unwrap();
        assert_eq!(back.name, "my-job");
        assert_eq!(back.timeout_secs, 60);
    }

    #[tokio::test]
    async fn job_list() {
        let store = fresh_store().await;
        for n in ["a", "b", "c"] {
            let job = JobBuilder::new(n, "echo", cron_trigger()).build().unwrap();
            store.jobs().insert(&job).await.unwrap();
        }
        let jobs = store.jobs().list().await.unwrap();
        assert_eq!(jobs.len(), 3);
    }

    #[tokio::test]
    async fn job_due_uses_next_fire() {
        let store = fresh_store().await;
        let job = JobBuilder::new("nightly", "echo", cron_trigger())
            .build()
            .unwrap();
        store.jobs().insert(&job).await.unwrap();
        // initially no next_fire -> not due
        assert!(store.jobs().due(Utc::now()).await.unwrap().is_empty());
        // set past time
        store
            .jobs()
            .set_next_fire(job.id, Some(Utc::now() - chrono::Duration::seconds(1)))
            .await
            .unwrap();
        let due = store.jobs().due(Utc::now()).await.unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].name, "nightly");
    }

    #[tokio::test]
    async fn pause_excludes_from_due() {
        let store = fresh_store().await;
        let job = JobBuilder::new("nightly", "echo", cron_trigger())
            .build()
            .unwrap();
        store.jobs().insert(&job).await.unwrap();
        store
            .jobs()
            .set_next_fire(job.id, Some(Utc::now() - chrono::Duration::seconds(1)))
            .await
            .unwrap();
        store.jobs().set_paused(job.id, true).await.unwrap();
        assert!(store.jobs().due(Utc::now()).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn run_insert_update_get() {
        let store = fresh_store().await;
        let job = JobBuilder::new("j", "echo", cron_trigger())
            .build()
            .unwrap();
        store.jobs().insert(&job).await.unwrap();
        let mut run = Run::new(job.id, 1);
        store.runs().insert(&run).await.unwrap();
        run.state = RunState::Running;
        run.started_at = Some(Utc::now());
        store.runs().update(&run).await.unwrap();
        let back = store.runs().get(run.id).await.unwrap();
        assert_eq!(back.state, RunState::Running);
        assert!(back.started_at.is_some());
    }

    #[tokio::test]
    async fn active_runs_only() {
        let store = fresh_store().await;
        let job = JobBuilder::new("j", "echo", cron_trigger())
            .build()
            .unwrap();
        store.jobs().insert(&job).await.unwrap();
        let r1 = Run::new(job.id, 1);
        let mut r2 = Run::new(job.id, 2);
        r2.state = RunState::Success;
        store.runs().insert(&r1).await.unwrap();
        store.runs().insert(&r2).await.unwrap();
        let active = store.runs().list_active().await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, r1.id);
    }

    #[tokio::test]
    async fn calendar_roundtrip() {
        use rsched_core::CalendarRule;
        let store = fresh_store().await;
        let cal = Calendar {
            id: CalendarId::new(),
            name: "biz".into(),
            rules: vec![CalendarRule::Weekdays {
                days: vec![1, 2, 3, 4, 5],
            }],
        };
        store.calendars().insert(&cal).await.unwrap();
        let back = store.calendars().get(cal.id).await.unwrap();
        assert_eq!(back.name, "biz");
    }

    #[tokio::test]
    async fn agent_upsert_idempotent() {
        let store = fresh_store().await;
        let id = AgentId::new();
        store
            .agents()
            .upsert(
                id,
                "host-a",
                "FP123",
                &["etl".into()],
                Some("0.1.0"),
                Some("linux"),
                Some("x86_64"),
            )
            .await
            .unwrap();
        store
            .agents()
            .upsert(
                id,
                "host-a",
                "FP123",
                &["etl".into()],
                Some("0.1.0"),
                Some("linux"),
                Some("x86_64"),
            )
            .await
            .unwrap();
        assert_eq!(store.agents().count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn run_log_append_and_fetch() {
        let store = fresh_store().await;
        let job = JobBuilder::new("lj", "echo", cron_trigger())
            .build()
            .unwrap();
        store.jobs().insert(&job).await.unwrap();
        let run = rsched_core::Run::new(job.id, 1);
        store.runs().insert(&run).await.unwrap();
        let id = run.id.to_string();
        store
            .run_logs()
            .append(&id, 0, "2026-01-01T00:00:00Z", "stdout", b"hello")
            .await
            .unwrap();
        store
            .run_logs()
            .append(&id, 1, "2026-01-01T00:00:01Z", "stderr", b"err")
            .await
            .unwrap();
        let rows = store.run_logs().fetch(&id, None, 100).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].chunk, b"hello");
        assert_eq!(rows[1].stream, "stderr");
    }

    #[tokio::test]
    async fn run_log_from_seq_filter() {
        let store = fresh_store().await;
        let job = JobBuilder::new("lj2", "echo", cron_trigger())
            .build()
            .unwrap();
        store.jobs().insert(&job).await.unwrap();
        let run = rsched_core::Run::new(job.id, 1);
        store.runs().insert(&run).await.unwrap();
        let id = run.id.to_string();
        for i in 0i64..5 {
            store
                .run_logs()
                .append(&id, i, "2026-01-01T00:00:00Z", "stdout", b"x")
                .await
                .unwrap();
        }
        let rows = store.run_logs().fetch(&id, Some(3), 100).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].seq, 3);
        assert_eq!(rows[1].seq, 4);
    }

    #[tokio::test]
    async fn run_log_limit() {
        let store = fresh_store().await;
        let job = JobBuilder::new("lj3", "echo", cron_trigger())
            .build()
            .unwrap();
        store.jobs().insert(&job).await.unwrap();
        let run = rsched_core::Run::new(job.id, 1);
        store.runs().insert(&run).await.unwrap();
        let id = run.id.to_string();
        for i in 0i64..10 {
            store
                .run_logs()
                .append(&id, i, "2026-01-01T00:00:00Z", "stdout", b"y")
                .await
                .unwrap();
        }
        let rows = store.run_logs().fetch(&id, None, 3).await.unwrap();
        assert_eq!(rows.len(), 3);
    }
}
