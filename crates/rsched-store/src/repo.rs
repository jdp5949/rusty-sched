//! High-level repositories. One `Store` wraps the pool and exposes the four
//! domain repos.

use crate::StoreError;
use chrono::{DateTime, Utc};
use rsched_core::{AgentId, Calendar, CalendarId, Job, JobId, Run, RunId, RunState, TriggerKind};
use sqlx::any::AnyRow;
use sqlx::{AnyPool, Row};

/// Wraps a pool + provides repo accessors.
#[derive(Clone)]
pub struct Store {
    pool: AnyPool,
    url: String,
}

impl Store {
    /// Wrap an existing pool.
    pub fn new(pool: AnyPool) -> Self {
        Self {
            pool,
            url: String::new(),
        }
    }

    /// Wrap a pool opened from a known URL (enables correct migrator selection).
    pub fn with_url(pool: AnyPool, url: impl Into<String>) -> Self {
        Self {
            pool,
            url: url.into(),
        }
    }

    /// Run embedded migrations to head.
    pub async fn migrate(&self) -> Result<(), StoreError> {
        crate::migrator_for_url(&self.url).run(&self.pool).await?;
        Ok(())
    }

    /// Borrow underlying pool (for transactions / advanced use).
    pub fn pool(&self) -> &AnyPool {
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
    /// User repository.
    pub fn users(&self) -> UserRepo<'_> {
        UserRepo { pool: &self.pool }
    }
    /// Session repository.
    pub fn sessions(&self) -> SessionRepo<'_> {
        SessionRepo { pool: &self.pool }
    }
    /// API key repository.
    pub fn api_keys(&self) -> ApiKeyRepo<'_> {
        ApiKeyRepo { pool: &self.pool }
    }
    /// Audit log repository.
    pub fn audit(&self) -> AuditRepo<'_> {
        AuditRepo { pool: &self.pool }
    }
    /// Virtual resource repository.
    pub fn resources(&self) -> ResourceRepo<'_> {
        ResourceRepo { pool: &self.pool }
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
        TriggerKind::Condition => "condition",
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
    pool: &'a AnyPool,
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
        let exclude_calendar_id = job.exclude_calendar_id.as_ref().map(|c| c.to_string());
        let must_start_times_json = serde_json::to_string(&job.must_start_times)?;
        let must_complete_times_json = serde_json::to_string(&job.must_complete_times)?;
        let exit_policy_json = serde_json::to_string(&job.exit_policy)?;
        let resource_claims_json = serde_json::to_string(&job.resource_claims)?;
        let paused = job.paused as i64;
        let created = job.created_at.to_rfc3339();
        let updated = job.updated_at.to_rfc3339();
        let next_fire = job.next_fire_at.map(|t| t.to_rfc3339());

        sqlx::query(
            r#"INSERT INTO jobs
            (id, name, box_id, trigger_kind, trigger_data_json, cmd, args_json, env_json,
             cwd, shell, target_json, retry_json, timeout_secs, sla_secs, calendar_id,
             misfire_policy, paused, alert_config_json, created_at, updated_at, next_fire_at,
             exclude_calendar_id, must_start_times_json, must_complete_times_json,
             exit_policy_json, resource_claims_json)
            VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"#,
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
        .bind(exclude_calendar_id)
        .bind(must_start_times_json)
        .bind(must_complete_times_json)
        .bind(exit_policy_json)
        .bind(resource_claims_json)
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
        let exclude_calendar_id = job.exclude_calendar_id.as_ref().map(|c| c.to_string());
        let must_start_times_json = serde_json::to_string(&job.must_start_times)?;
        let must_complete_times_json = serde_json::to_string(&job.must_complete_times)?;
        let exit_policy_json = serde_json::to_string(&job.exit_policy)?;
        let resource_claims_json = serde_json::to_string(&job.resource_claims)?;
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
                alert_config_json = ?, updated_at = ?, next_fire_at = ?,
                exclude_calendar_id = ?, must_start_times_json = ?,
                must_complete_times_json = ?, exit_policy_json = ?,
                resource_claims_json = ?
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
        .bind(exclude_calendar_id)
        .bind(must_start_times_json)
        .bind(must_complete_times_json)
        .bind(exit_policy_json)
        .bind(resource_claims_json)
        .bind(job.id.to_string())
        .execute(self.pool)
        .await?;
        Ok(())
    }
}

fn row_to_job(row: &sqlx::any::AnyRow) -> Result<Job, StoreError> {
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

    let exclude_calendar_id: Option<String> = row.try_get("exclude_calendar_id").ok().flatten();
    let exclude_calendar_id = exclude_calendar_id
        .as_deref()
        .map(|s| s.parse())
        .transpose()
        .map_err(|e: ulid::DecodeError| StoreError::NotFound(format!("bad excl cal id: {e}")))?;
    let must_start_times_json: Option<String> = row.try_get("must_start_times_json").ok().flatten();
    let must_start_times = match must_start_times_json {
        Some(s) if !s.is_empty() => serde_json::from_str(&s)?,
        _ => Vec::new(),
    };
    let must_complete_times_json: Option<String> =
        row.try_get("must_complete_times_json").ok().flatten();
    let must_complete_times = match must_complete_times_json {
        Some(s) if !s.is_empty() => serde_json::from_str(&s)?,
        _ => Vec::new(),
    };
    let exit_policy_json: Option<String> = row.try_get("exit_policy_json").ok().flatten();
    let exit_policy = match exit_policy_json {
        Some(s) if !s.is_empty() => serde_json::from_str(&s)?,
        _ => rsched_core::ExitCodePolicy::default(),
    };
    let resource_claims_json: Option<String> = row.try_get("resource_claims_json").ok().flatten();
    let resource_claims = match resource_claims_json {
        Some(s) if !s.is_empty() => serde_json::from_str(&s)?,
        _ => Vec::new(),
    };

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
        exclude_calendar_id,
        must_start_times,
        must_complete_times,
        exit_policy,
        resource_claims,
        misfire,
        dependencies: Vec::new(), // loaded separately via dep table when needed
        paused: paused != 0,
        alerts,
        created_at,
        updated_at,
        next_fire_at,
    })
}

/// Aggregated stats for a job over the last 24 hours.
#[derive(Debug, serde::Serialize)]
pub struct JobStats {
    /// Total runs in the last 24h.
    pub total_24h: i64,
    /// Successful runs in the last 24h.
    pub success_24h: i64,
    /// Success rate as a fraction [0.0, 1.0].
    pub success_rate_24h: f64,
    /// Median (p50) run duration in seconds.
    pub p50_duration_secs: f64,
    /// 99th-percentile run duration in seconds.
    pub p99_duration_secs: f64,
    /// RFC-3339 timestamp of the most recent failure, if any.
    pub last_failure_at: Option<String>,
    /// Last 20 run outcomes: "success" / "failure" / "running" / "unknown".
    pub recent_outcomes: Vec<String>,
}

/// Repository for [`Run`].
pub struct RunRepo<'a> {
    pool: &'a AnyPool,
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

    /// True if the job has any run in a non-terminal state.
    pub async fn has_active_for_job(&self, job_id: JobId) -> Result<bool, StoreError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM runs WHERE job_id = ? AND state IN ('queued','running')",
        )
        .bind(job_id.to_string())
        .fetch_one(self.pool)
        .await?;
        Ok(count > 0)
    }

    /// Set the state of a run.
    pub async fn set_state(&self, run_id: RunId, state: RunState) -> Result<(), StoreError> {
        sqlx::query("UPDATE runs SET state = ? WHERE id = ?")
            .bind(run_state_str(state))
            .bind(run_id.to_string())
            .execute(self.pool)
            .await?;
        Ok(())
    }

    /// List runs currently in non-terminal states.
    pub async fn list_active(&self) -> Result<Vec<Run>, StoreError> {
        let rows = sqlx::query("SELECT * FROM runs WHERE state IN ('queued','running')")
            .fetch_all(self.pool)
            .await?;
        rows.iter().map(row_to_run).collect()
    }

    /// Compute stats for a job over the last 24 hours.
    pub async fn job_stats(&self, job_id: &str) -> sqlx::Result<JobStats> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339();
        let rows = sqlx::query(
            "SELECT state, started_at, finished_at FROM runs \
             WHERE job_id = ? AND queued_at > ? \
             ORDER BY queued_at DESC LIMIT 500",
        )
        .bind(job_id)
        .bind(&cutoff)
        .fetch_all(self.pool)
        .await?;

        let total_24h = rows.len() as i64;
        let mut success_24h: i64 = 0;
        let mut durations: Vec<f64> = Vec::new();
        let mut last_failure_at: Option<String> = None;
        let mut recent_outcomes: Vec<String> = Vec::new();

        for row in &rows {
            let state: String = row.try_get("state").unwrap_or_default();
            let started: Option<String> = row.try_get("started_at").unwrap_or(None);
            let finished: Option<String> = row.try_get("finished_at").unwrap_or(None);

            let outcome = map_outcome(&state);
            if recent_outcomes.len() < 20 {
                recent_outcomes.push(outcome.to_string());
            }

            if state == "success" {
                success_24h += 1;
            } else if (state == "failed" || state == "killed") && last_failure_at.is_none() {
                last_failure_at = finished.clone().or_else(|| started.clone());
            }

            if let (Some(s), Some(f)) = (started, finished) {
                if let (Ok(st), Ok(ft)) = (
                    chrono::DateTime::parse_from_rfc3339(&s),
                    chrono::DateTime::parse_from_rfc3339(&f),
                ) {
                    let secs = (ft - st).num_milliseconds() as f64 / 1000.0;
                    if secs >= 0.0 {
                        durations.push(secs);
                    }
                }
            }
        }

        let success_rate_24h = if total_24h > 0 {
            success_24h as f64 / total_24h as f64
        } else {
            0.0
        };

        durations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let p50 = percentile(&durations, 50.0);
        let p99 = percentile(&durations, 99.0);

        Ok(JobStats {
            total_24h,
            success_24h,
            success_rate_24h,
            p50_duration_secs: p50,
            p99_duration_secs: p99,
            last_failure_at,
            recent_outcomes,
        })
    }
}

fn map_outcome(state: &str) -> &'static str {
    match state {
        "success" => "success",
        "failed" | "killed" | "lost" => "failure",
        "running" | "queued" => "running",
        _ => "unknown",
    }
}

fn percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((pct / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn row_to_run(row: &sqlx::any::AnyRow) -> Result<Run, StoreError> {
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
    pool: &'a AnyPool,
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
        Self::row_to_calendar(&row)
    }

    /// Fetch by name.
    pub async fn get_by_name(&self, name: &str) -> Result<Calendar, StoreError> {
        let row = sqlx::query("SELECT id, name, definition_json FROM calendars WHERE name = ?")
            .bind(name)
            .fetch_optional(self.pool)
            .await?
            .ok_or_else(|| StoreError::NotFound(name.into()))?;
        Self::row_to_calendar(&row)
    }

    /// List all calendars.
    pub async fn list(&self) -> Result<Vec<Calendar>, StoreError> {
        let rows = sqlx::query("SELECT id, name, definition_json FROM calendars ORDER BY name")
            .fetch_all(self.pool)
            .await?;
        rows.iter().map(Self::row_to_calendar).collect()
    }

    fn row_to_calendar(row: &AnyRow) -> Result<Calendar, StoreError> {
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
    pool: &'a AnyPool,
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
    pool: &'a AnyPool,
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
            "INSERT INTO run_logs (run_id, seq, ts, stream, chunk) \
             VALUES (?, ?, ?, ?, ?) ON CONFLICT (run_id, seq) DO NOTHING",
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
        crate::pool::init_drivers();
        let pool = crate::open_pool("sqlite::memory:").await.unwrap();
        let store = Store::with_url(pool, "sqlite::memory:");
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
    async fn job_autosys_fields_roundtrip() {
        use chrono::NaiveTime;
        use rsched_core::ExitCodePolicy;
        let store = fresh_store().await;
        let mut job = JobBuilder::new("audit", "echo hi", cron_trigger())
            .build()
            .unwrap();
        job.exit_policy = ExitCodePolicy {
            max_exit_success: 2,
            fail_codes: vec![100, 101],
            condition_code: Some(7),
        };
        job.must_start_times = vec![NaiveTime::from_hms_opt(2, 0, 0).unwrap()];
        job.must_complete_times = vec![NaiveTime::from_hms_opt(4, 30, 0).unwrap()];
        store.jobs().insert(&job).await.unwrap();
        let back = store.jobs().get(job.id).await.unwrap();
        assert_eq!(back.exit_policy.max_exit_success, 2);
        assert_eq!(back.exit_policy.fail_codes, vec![100, 101]);
        assert_eq!(back.exit_policy.condition_code, Some(7));
        assert_eq!(back.must_start_times.len(), 1);
        assert_eq!(back.must_complete_times.len(), 1);
    }

    #[tokio::test]
    async fn job_autosys_fields_update_persists() {
        use rsched_core::ExitCodePolicy;
        let store = fresh_store().await;
        let mut job = JobBuilder::new("audit2", "echo hi", cron_trigger())
            .build()
            .unwrap();
        store.jobs().insert(&job).await.unwrap();
        job.exit_policy = ExitCodePolicy {
            max_exit_success: 4,
            ..Default::default()
        };
        store.jobs().update(&job).await.unwrap();
        let back = store.jobs().get(job.id).await.unwrap();
        assert_eq!(back.exit_policy.max_exit_success, 4);
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
    async fn run_log_append_and_fetch() {
        let store = fresh_store().await;
        let job = JobBuilder::new("j", "echo", cron_trigger())
            .build()
            .unwrap();
        store.jobs().insert(&job).await.unwrap();
        let run = Run::new(job.id, 1);
        store.runs().insert(&run).await.unwrap();
        let run_id = run.id.to_string();

        store
            .run_logs()
            .append(&run_id, 0, "2026-01-01T00:00:00Z", "stdout", b"hello")
            .await
            .unwrap();
        store
            .run_logs()
            .append(&run_id, 1, "2026-01-01T00:00:01Z", "stderr", b"err")
            .await
            .unwrap();
        store
            .run_logs()
            .append(&run_id, 2, "2026-01-01T00:00:02Z", "stdout", b"world")
            .await
            .unwrap();

        let rows = store.run_logs().fetch(&run_id, None, 100).await.unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].seq, 0);
        assert_eq!(rows[0].chunk, b"hello");
        assert_eq!(rows[1].stream, "stderr");
        assert_eq!(rows[2].seq, 2);
    }

    #[tokio::test]
    async fn run_log_from_seq_filter() {
        let store = fresh_store().await;
        let job = JobBuilder::new("j2", "echo", cron_trigger())
            .build()
            .unwrap();
        store.jobs().insert(&job).await.unwrap();
        let run = Run::new(job.id, 1);
        store.runs().insert(&run).await.unwrap();
        let run_id = run.id.to_string();

        for i in 0i64..5 {
            store
                .run_logs()
                .append(&run_id, i, "2026-01-01T00:00:00Z", "stdout", b"x")
                .await
                .unwrap();
        }

        let rows = store.run_logs().fetch(&run_id, Some(2), 100).await.unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].seq, 2);
    }

    #[tokio::test]
    async fn run_log_limit() {
        let store = fresh_store().await;
        let job = JobBuilder::new("j3", "echo", cron_trigger())
            .build()
            .unwrap();
        store.jobs().insert(&job).await.unwrap();
        let run = Run::new(job.id, 1);
        store.runs().insert(&run).await.unwrap();
        let run_id = run.id.to_string();

        for i in 0i64..10 {
            store
                .run_logs()
                .append(&run_id, i, "2026-01-01T00:00:00Z", "stdout", b"y")
                .await
                .unwrap();
        }

        let rows = store.run_logs().fetch(&run_id, None, 3).await.unwrap();
        assert_eq!(rows.len(), 3);
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

    // ---------- opt-in Postgres tests ----------
    // To run: RSCHED_PG_TEST_URL=postgres://postgres:test@localhost/rsched_test \
    //   cargo test -p rsched-store -- pg_ --ignored
    #[tokio::test]
    #[ignore]
    async fn pg_insert_get_list_due() {
        let url = match std::env::var("RSCHED_PG_TEST_URL") {
            Ok(u) => u,
            Err(_) => return,
        };
        crate::pool::init_drivers();
        let pool = crate::open_pool(&url).await.expect("pg connect");
        let store = Store::with_url(pool, &url);
        store.migrate().await.expect("pg migrate");

        let job = JobBuilder::new("pg-job", "echo pg", cron_trigger())
            .timeout(30)
            .build()
            .unwrap();
        store.jobs().insert(&job).await.unwrap();
        let back = store.jobs().get(job.id).await.unwrap();
        assert_eq!(back.name, "pg-job");

        let list = store.jobs().list().await.unwrap();
        assert!(!list.is_empty());

        store
            .jobs()
            .set_next_fire(job.id, Some(Utc::now() - chrono::Duration::seconds(1)))
            .await
            .unwrap();
        let due = store.jobs().due(Utc::now()).await.unwrap();
        assert!(!due.is_empty());

        store.jobs().delete(job.id).await.unwrap();
    }
}

// ----- Users / Sessions / API keys / Audit ---------------------------------

use rsched_core::{ApiKey, ApiKeyId, Role, User, UserId};

fn parse_ts(s: &str) -> Result<DateTime<Utc>, StoreError> {
    Ok(DateTime::parse_from_rfc3339(s)
        .map_err(|e| StoreError::NotFound(format!("bad ts: {e}")))?
        .with_timezone(&Utc))
}

/// User repository.
pub struct UserRepo<'a> {
    pool: &'a AnyPool,
}

impl<'a> UserRepo<'a> {
    /// Insert a new user. `password_hash` is the bcrypt hash.
    pub async fn insert(
        &self,
        id: UserId,
        username: &str,
        password_hash: &str,
        role: Role,
    ) -> Result<(), StoreError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role, disabled, created_at) \
             VALUES (?, ?, ?, ?, 0, ?)",
        )
        .bind(id.to_string())
        .bind(username)
        .bind(password_hash)
        .bind(role.as_str())
        .bind(now)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Look up by username, returning user + password hash.
    pub async fn get_by_username(
        &self,
        username: &str,
    ) -> Result<Option<(User, String)>, StoreError> {
        let row = sqlx::query(
            "SELECT id, username, password_hash, role, disabled, created_at \
             FROM users WHERE username = ?",
        )
        .bind(username)
        .fetch_optional(self.pool)
        .await?;
        match row {
            Some(r) => Ok(Some(Self::row_to_user_with_hash(&r)?)),
            None => Ok(None),
        }
    }

    /// Look up by id.
    pub async fn get(&self, id: UserId) -> Result<User, StoreError> {
        let row = sqlx::query(
            "SELECT id, username, password_hash, role, disabled, created_at \
             FROM users WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::NotFound(id.to_string()))?;
        let (u, _) = Self::row_to_user_with_hash(&row)?;
        Ok(u)
    }

    /// List all users.
    pub async fn list(&self) -> Result<Vec<User>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, username, password_hash, role, disabled, created_at \
             FROM users ORDER BY username",
        )
        .fetch_all(self.pool)
        .await?;
        rows.iter()
            .map(|r| Self::row_to_user_with_hash(r).map(|(u, _)| u))
            .collect()
    }

    /// Set a new password hash.
    pub async fn set_password_hash(
        &self,
        id: UserId,
        password_hash: &str,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
            .bind(password_hash)
            .bind(id.to_string())
            .execute(self.pool)
            .await?;
        Ok(())
    }

    /// Set a user's enabled/disabled flag.
    pub async fn set_disabled(&self, id: UserId, disabled: bool) -> Result<(), StoreError> {
        sqlx::query("UPDATE users SET disabled = ? WHERE id = ?")
            .bind(disabled as i64)
            .bind(id.to_string())
            .execute(self.pool)
            .await?;
        Ok(())
    }

    /// Count users — used by `/readyz` and to decide whether to seed an admin.
    pub async fn count(&self) -> Result<i64, StoreError> {
        let row = sqlx::query("SELECT COUNT(*) AS n FROM users")
            .fetch_one(self.pool)
            .await?;
        Ok(row.try_get("n")?)
    }

    fn row_to_user_with_hash(row: &AnyRow) -> Result<(User, String), StoreError> {
        let id_str: String = row.try_get("id")?;
        let id: UserId = id_str
            .parse()
            .map_err(|e: ulid::DecodeError| StoreError::NotFound(format!("bad user id: {e}")))?;
        let username: String = row.try_get("username")?;
        let pw: String = row.try_get("password_hash")?;
        let role_str: String = row.try_get("role")?;
        let role = Role::parse(&role_str)
            .ok_or_else(|| StoreError::NotFound(format!("bad role: {role_str}")))?;
        let disabled: i64 = row.try_get("disabled")?;
        let created_at_str: String = row.try_get("created_at")?;
        let created_at = parse_ts(&created_at_str)?;
        Ok((
            User {
                id,
                username,
                role,
                disabled: disabled != 0,
                created_at,
            },
            pw,
        ))
    }
}

/// Session repository.
pub struct SessionRepo<'a> {
    pool: &'a AnyPool,
}

impl<'a> SessionRepo<'a> {
    /// Insert a session. The token should be a cryptographically random string.
    pub async fn insert(
        &self,
        token: &str,
        user_id: UserId,
        expires_at: DateTime<Utc>,
        ip: Option<&str>,
    ) -> Result<(), StoreError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO sessions (token, user_id, expires_at, created_at, ip) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(token)
        .bind(user_id.to_string())
        .bind(expires_at.to_rfc3339())
        .bind(now)
        .bind(ip)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Look up session by token, returning the (user_id, expires_at) if alive.
    pub async fn get_valid(
        &self,
        token: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<(UserId, DateTime<Utc>)>, StoreError> {
        let row = sqlx::query("SELECT user_id, expires_at FROM sessions WHERE token = ?")
            .bind(token)
            .fetch_optional(self.pool)
            .await?;
        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };
        let uid_str: String = row.try_get("user_id")?;
        let uid: UserId = uid_str
            .parse()
            .map_err(|e: ulid::DecodeError| StoreError::NotFound(format!("bad user id: {e}")))?;
        let exp_str: String = row.try_get("expires_at")?;
        let exp = parse_ts(&exp_str)?;
        if exp <= now {
            return Ok(None);
        }
        Ok(Some((uid, exp)))
    }

    /// Delete a session (logout).
    pub async fn delete(&self, token: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM sessions WHERE token = ?")
            .bind(token)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    /// Prune expired sessions.
    pub async fn prune_expired(&self, now: DateTime<Utc>) -> Result<u64, StoreError> {
        let res = sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
            .bind(now.to_rfc3339())
            .execute(self.pool)
            .await?;
        Ok(res.rows_affected())
    }
}

/// API key repository.
pub struct ApiKeyRepo<'a> {
    pool: &'a AnyPool,
}

impl<'a> ApiKeyRepo<'a> {
    /// Insert an API key (bcrypt-hashed token).
    pub async fn insert(
        &self,
        id: ApiKeyId,
        user_id: UserId,
        name: &str,
        key_hash: &str,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<(), StoreError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO api_keys (id, user_id, name, key_hash, created_at, last_used_at, expires_at, disabled) \
             VALUES (?, ?, ?, ?, ?, NULL, ?, 0)",
        )
        .bind(id.to_string())
        .bind(user_id.to_string())
        .bind(name)
        .bind(key_hash)
        .bind(now)
        .bind(expires_at.map(|t| t.to_rfc3339()))
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// List all keys (metadata only).
    pub async fn list_for_user(&self, user_id: UserId) -> Result<Vec<ApiKey>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, user_id, name, created_at, last_used_at, expires_at, disabled \
             FROM api_keys WHERE user_id = ? ORDER BY created_at DESC",
        )
        .bind(user_id.to_string())
        .fetch_all(self.pool)
        .await?;
        rows.iter().map(Self::row_to_api_key).collect()
    }

    /// Fetch every key's `(id, user_id, hash)` — auth-time lookup.
    pub async fn all_active(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<(ApiKeyId, UserId, String)>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, user_id, key_hash, expires_at, disabled FROM api_keys \
             WHERE disabled = 0",
        )
        .fetch_all(self.pool)
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let exp_str: Option<String> = r.try_get("expires_at")?;
            if let Some(s) = exp_str {
                if parse_ts(&s)? <= now {
                    continue;
                }
            }
            let id_str: String = r.try_get("id")?;
            let id: ApiKeyId = id_str
                .parse()
                .map_err(|e: ulid::DecodeError| StoreError::NotFound(format!("bad key id: {e}")))?;
            let uid_str: String = r.try_get("user_id")?;
            let uid: UserId = uid_str.parse().map_err(|e: ulid::DecodeError| {
                StoreError::NotFound(format!("bad user id: {e}"))
            })?;
            let hash: String = r.try_get("key_hash")?;
            out.push((id, uid, hash));
        }
        Ok(out)
    }

    /// Update `last_used_at`.
    pub async fn touch(&self, id: ApiKeyId) -> Result<(), StoreError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE api_keys SET last_used_at = ? WHERE id = ?")
            .bind(now)
            .bind(id.to_string())
            .execute(self.pool)
            .await?;
        Ok(())
    }

    /// Delete a key.
    pub async fn delete(&self, id: ApiKeyId) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM api_keys WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await?;
        Ok(())
    }

    fn row_to_api_key(row: &AnyRow) -> Result<ApiKey, StoreError> {
        let id_str: String = row.try_get("id")?;
        let id: ApiKeyId = id_str
            .parse()
            .map_err(|e: ulid::DecodeError| StoreError::NotFound(format!("bad key id: {e}")))?;
        let uid_str: String = row.try_get("user_id")?;
        let user_id: UserId = uid_str
            .parse()
            .map_err(|e: ulid::DecodeError| StoreError::NotFound(format!("bad user id: {e}")))?;
        let name: String = row.try_get("name")?;
        let created_at_str: String = row.try_get("created_at")?;
        let last_used_str: Option<String> = row.try_get("last_used_at")?;
        let expires_str: Option<String> = row.try_get("expires_at")?;
        let disabled: i64 = row.try_get("disabled")?;
        Ok(ApiKey {
            id,
            user_id,
            name,
            created_at: parse_ts(&created_at_str)?,
            last_used_at: last_used_str.as_deref().map(parse_ts).transpose()?,
            expires_at: expires_str.as_deref().map(parse_ts).transpose()?,
            disabled: disabled != 0,
        })
    }
}

/// One row in the audit log.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuditEntry {
    /// Row id.
    pub id: String,
    /// User who performed the action (None for system).
    pub user_id: Option<String>,
    /// Action verb, e.g. "job.create", "job.delete".
    pub action: String,
    /// Target resource type ("job", "run", "user", …).
    pub target_type: String,
    /// Target resource id (ULID string).
    pub target_id: Option<String>,
    /// Optional JSON payload with details.
    pub payload_json: Option<String>,
    /// Timestamp.
    pub ts: String,
}

/// Audit log repository.
pub struct AuditRepo<'a> {
    pool: &'a AnyPool,
}

impl<'a> AuditRepo<'a> {
    /// Insert an audit row.
    pub async fn record(
        &self,
        user_id: Option<&str>,
        action: &str,
        target_type: &str,
        target_id: Option<&str>,
        payload_json: Option<&str>,
    ) -> Result<(), StoreError> {
        let id = ulid::Ulid::new().to_string();
        let ts = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO audit_log (id, user_id, action, target_type, target_id, payload_json, ts) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(user_id)
        .bind(action)
        .bind(target_type)
        .bind(target_id)
        .bind(payload_json)
        .bind(ts)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Fetch recent entries (newest first).
    pub async fn recent(&self, limit: i64) -> Result<Vec<AuditEntry>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, user_id, action, target_type, target_id, payload_json, ts \
             FROM audit_log ORDER BY ts DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                Ok(AuditEntry {
                    id: r.try_get("id")?,
                    user_id: r.try_get("user_id")?,
                    action: r.try_get("action")?,
                    target_type: r.try_get("target_type")?,
                    target_id: r.try_get("target_id")?,
                    payload_json: r.try_get("payload_json")?,
                    ts: r.try_get("ts")?,
                })
            })
            .collect()
    }
}

// ----- Virtual resources -----------------------------------------------------

use rsched_core::{Resource, ResourceClaim, ResourceId};

/// Virtual resource repository — counters with fixed capacity, claimed by runs.
pub struct ResourceRepo<'a> {
    pool: &'a AnyPool,
}

impl<'a> ResourceRepo<'a> {
    /// Insert a new resource.
    pub async fn insert(&self, r: &Resource) -> Result<(), StoreError> {
        r.validate()?;
        sqlx::query(
            "INSERT INTO resources (id, name, capacity, description, created_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(r.id.to_string())
        .bind(&r.name)
        .bind(r.capacity as i64)
        .bind(&r.description)
        .bind(r.created_at.to_rfc3339())
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// List all resources.
    pub async fn list(&self) -> Result<Vec<Resource>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, name, capacity, description, created_at FROM resources ORDER BY name",
        )
        .fetch_all(self.pool)
        .await?;
        rows.iter().map(Self::row_to_resource).collect()
    }

    /// Get a resource by name.
    pub async fn get_by_name(&self, name: &str) -> Result<Option<Resource>, StoreError> {
        let row = sqlx::query(
            "SELECT id, name, capacity, description, created_at FROM resources WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(self.pool)
        .await?;
        row.map(|r| Self::row_to_resource(&r)).transpose()
    }

    /// Delete by id.
    pub async fn delete(&self, id: ResourceId) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM resources WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await?;
        Ok(())
    }

    /// Currently available units for one resource.
    pub async fn available_units(&self, id: ResourceId) -> Result<u32, StoreError> {
        let row = sqlx::query(
            "SELECT (SELECT capacity FROM resources WHERE id = ?1) - \
                    COALESCE((SELECT SUM(units) FROM resource_holds WHERE resource_id = ?1), 0) \
             AS available",
        )
        .bind(id.to_string())
        .fetch_one(self.pool)
        .await?;
        let n: i64 = row.try_get("available")?;
        Ok(n.max(0) as u32)
    }

    /// Atomic acquire. Resolves each claim by name → id, checks capacity,
    /// inserts holds. Returns `Ok(false)` if any claim doesn't fit or the
    /// resource name is unknown; partial holds are rolled back.
    pub async fn try_acquire(
        &self,
        run_id: rsched_core::RunId,
        claims: &[ResourceClaim],
    ) -> Result<bool, StoreError> {
        if claims.is_empty() {
            return Ok(true);
        }
        let mut tx = self.pool.begin().await?;
        for claim in claims {
            let row = sqlx::query("SELECT id, capacity FROM resources WHERE name = ?")
                .bind(&claim.resource_name)
                .fetch_optional(&mut *tx)
                .await?;
            let Some(row) = row else {
                tx.rollback().await?;
                return Ok(false);
            };
            let res_id: String = row.try_get("id")?;
            let capacity: i64 = row.try_get("capacity")?;
            let used: i64 = sqlx::query(
                "SELECT COALESCE(SUM(units), 0) AS used FROM resource_holds WHERE resource_id = ?",
            )
            .bind(&res_id)
            .fetch_one(&mut *tx)
            .await?
            .try_get("used")?;
            if used + claim.units as i64 > capacity {
                tx.rollback().await?;
                return Ok(false);
            }
            sqlx::query(
                "INSERT INTO resource_holds (run_id, resource_id, units, acquired_at) \
                 VALUES (?, ?, ?, ?)",
            )
            .bind(run_id.to_string())
            .bind(res_id)
            .bind(claim.units as i64)
            .bind(Utc::now().to_rfc3339())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    /// Release every hold belonging to a run.
    pub async fn release(&self, run_id: rsched_core::RunId) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM resource_holds WHERE run_id = ?")
            .bind(run_id.to_string())
            .execute(self.pool)
            .await?;
        Ok(())
    }

    fn row_to_resource(row: &AnyRow) -> Result<Resource, StoreError> {
        let id_str: String = row.try_get("id")?;
        let id: ResourceId = id_str.parse().map_err(|e: ulid::DecodeError| {
            StoreError::NotFound(format!("bad resource id: {e}"))
        })?;
        let name: String = row.try_get("name")?;
        let capacity: i64 = row.try_get("capacity")?;
        let description: Option<String> = row.try_get("description")?;
        let created_at_str: String = row.try_get("created_at")?;
        Ok(Resource {
            id,
            name,
            capacity: capacity as u32,
            description,
            created_at: parse_ts(&created_at_str)?,
        })
    }
}

#[cfg(test)]
mod auth_tests {
    use super::*;
    use rsched_core::Role;

    async fn fresh_store() -> Store {
        crate::pool::init_drivers();
        let pool = crate::open_pool("sqlite::memory:").await.unwrap();
        let s = Store::with_url(pool, "sqlite::memory:");
        s.migrate().await.unwrap();
        s
    }

    #[tokio::test]
    async fn user_insert_lookup_roundtrip() {
        let store = fresh_store().await;
        let id = UserId::new();
        store
            .users()
            .insert(id, "alice", "$2y$dummy", Role::Admin)
            .await
            .unwrap();
        let (u, h) = store
            .users()
            .get_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(u.username, "alice");
        assert_eq!(u.role, Role::Admin);
        assert_eq!(h, "$2y$dummy");
        let by_id = store.users().get(id).await.unwrap();
        assert_eq!(by_id.username, "alice");
    }

    #[tokio::test]
    async fn user_count_and_list() {
        let store = fresh_store().await;
        assert_eq!(store.users().count().await.unwrap(), 0);
        store
            .users()
            .insert(UserId::new(), "u1", "x", Role::Viewer)
            .await
            .unwrap();
        store
            .users()
            .insert(UserId::new(), "u2", "x", Role::Operator)
            .await
            .unwrap();
        assert_eq!(store.users().count().await.unwrap(), 2);
        assert_eq!(store.users().list().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn session_lifecycle() {
        let store = fresh_store().await;
        let uid = UserId::new();
        store
            .users()
            .insert(uid, "alice", "x", Role::Admin)
            .await
            .unwrap();
        let exp = Utc::now() + chrono::Duration::hours(1);
        store
            .sessions()
            .insert("tok-abc", uid, exp, None)
            .await
            .unwrap();
        let got = store
            .sessions()
            .get_valid("tok-abc", Utc::now())
            .await
            .unwrap();
        assert!(got.is_some());
        // Expired session: insert past expiry.
        let past = Utc::now() - chrono::Duration::hours(1);
        store
            .sessions()
            .insert("tok-old", uid, past, None)
            .await
            .unwrap();
        assert!(store
            .sessions()
            .get_valid("tok-old", Utc::now())
            .await
            .unwrap()
            .is_none());
        // Delete + prune.
        store.sessions().delete("tok-abc").await.unwrap();
        let pruned = store.sessions().prune_expired(Utc::now()).await.unwrap();
        assert_eq!(pruned, 1);
    }

    #[tokio::test]
    async fn api_key_crud() {
        let store = fresh_store().await;
        let uid = UserId::new();
        store
            .users()
            .insert(uid, "alice", "x", Role::Admin)
            .await
            .unwrap();
        let kid = ApiKeyId::new();
        store
            .api_keys()
            .insert(kid, uid, "ci", "$2y$hash", None)
            .await
            .unwrap();
        let keys = store.api_keys().list_for_user(uid).await.unwrap();
        assert_eq!(keys.len(), 1);
        let active = store.api_keys().all_active(Utc::now()).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].2, "$2y$hash");
        store.api_keys().touch(kid).await.unwrap();
        store.api_keys().delete(kid).await.unwrap();
        assert!(store
            .api_keys()
            .list_for_user(uid)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn resource_acquire_release_lifecycle() {
        use rsched_core::{Resource, ResourceClaim, ResourceId, RunId};
        let store = fresh_store().await;
        let rid = ResourceId::new();
        store
            .resources()
            .insert(&Resource {
                id: rid,
                name: "db".into(),
                capacity: 5,
                description: None,
                created_at: Utc::now(),
            })
            .await
            .unwrap();
        // Available = capacity initially.
        assert_eq!(store.resources().available_units(rid).await.unwrap(), 5);

        // Acquire 3 for run1.
        let run1 = RunId::new();
        let claims = vec![ResourceClaim {
            resource_name: "db".into(),
            units: 3,
        }];
        assert!(store.resources().try_acquire(run1, &claims).await.unwrap());
        assert_eq!(store.resources().available_units(rid).await.unwrap(), 2);

        // Acquire 3 more for run2 → fails (only 2 left).
        let run2 = RunId::new();
        let big_claims = vec![ResourceClaim {
            resource_name: "db".into(),
            units: 3,
        }];
        assert!(!store
            .resources()
            .try_acquire(run2, &big_claims)
            .await
            .unwrap());
        assert_eq!(store.resources().available_units(rid).await.unwrap(), 2);

        // Smaller acquire for run2 succeeds.
        let small_claims = vec![ResourceClaim {
            resource_name: "db".into(),
            units: 2,
        }];
        assert!(store
            .resources()
            .try_acquire(run2, &small_claims)
            .await
            .unwrap());
        assert_eq!(store.resources().available_units(rid).await.unwrap(), 0);

        // Release run1 → 3 back.
        store.resources().release(run1).await.unwrap();
        assert_eq!(store.resources().available_units(rid).await.unwrap(), 3);
    }

    #[tokio::test]
    async fn resource_acquire_unknown_name_fails() {
        use rsched_core::{ResourceClaim, RunId};
        let store = fresh_store().await;
        let claims = vec![ResourceClaim {
            resource_name: "ghost".into(),
            units: 1,
        }];
        let ok = store
            .resources()
            .try_acquire(RunId::new(), &claims)
            .await
            .unwrap();
        assert!(!ok);
    }

    #[tokio::test]
    async fn audit_record_and_list() {
        let store = fresh_store().await;
        store
            .audit()
            .record(Some("user-1"), "job.create", "job", Some("job-1"), None)
            .await
            .unwrap();
        store
            .audit()
            .record(None, "system.start", "system", None, None)
            .await
            .unwrap();
        let entries = store.audit().recent(10).await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].action, "system.start");
    }
}
