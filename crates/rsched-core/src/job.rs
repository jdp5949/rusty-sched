//! Job — the unit a user schedules.

use crate::{
    AlertConfig, BoxId, CalendarId, CoreError, ExitCodePolicy, JobId, MisfirePolicy, ResourceClaim,
    RetryPolicy, Shell, Target, Trigger,
};
use chrono::{DateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// How upstream dependencies combine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DepCondition {
    /// All upstream jobs must succeed.
    #[default]
    AllSucceed,
    /// Any upstream job must succeed.
    AnySucceed,
    /// Any upstream job must finish (success or failure).
    AnyFinish,
}

/// Reference to an upstream job in a dependency edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepEdge {
    /// Upstream job id.
    pub upstream: JobId,
    /// Combination logic.
    pub condition: DepCondition,
}

/// A scheduled job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    /// Identifier.
    pub id: JobId,
    /// Unique name (used in CLI, alerts, deps).
    pub name: String,
    /// Optional box membership.
    pub box_id: Option<BoxId>,
    /// What triggers this job.
    pub trigger: Trigger,
    /// Command to run.
    pub cmd: String,
    /// Optional argv (otherwise cmd parsed by shell).
    pub args: Vec<String>,
    /// Env vars.
    pub env: HashMap<String, String>,
    /// Working directory.
    pub cwd: Option<String>,
    /// Shell wrapper.
    pub shell: Shell,
    /// Where to run.
    pub target: Target,
    /// Retry policy.
    pub retry: RetryPolicy,
    /// Hard timeout in seconds (0 = none).
    pub timeout_secs: u64,
    /// Soft SLA in seconds (0 = none).
    pub sla_secs: u64,
    /// Optional include-calendar (job allowed only when this allows).
    pub calendar_id: Option<CalendarId>,
    /// Optional exclude-calendar (job blocked when this allows). Autosys `exclude_calendar`.
    #[serde(default)]
    pub exclude_calendar_id: Option<CalendarId>,
    /// Must-start-by times of day (UTC). Used for `OnLateStart` alerts. Autosys `must_start_times`.
    #[serde(default)]
    pub must_start_times: Vec<NaiveTime>,
    /// Must-complete-by times of day (UTC). Used for `OnSlaMiss`. Autosys `must_complete_times`.
    #[serde(default)]
    pub must_complete_times: Vec<NaiveTime>,
    /// Exit-code policy (Autosys `max_exit_success`, `fail_codes`, `condition_code`).
    #[serde(default)]
    pub exit_policy: ExitCodePolicy,
    /// Virtual resource claims (Autosys `resources` attribute). Scheduler
    /// acquires every claim before dispatch; if any exceeds remaining
    /// capacity the job is left queued for the next tick.
    #[serde(default)]
    pub resource_claims: Vec<ResourceClaim>,
    /// Misfire policy.
    pub misfire: MisfirePolicy,
    /// Upstream deps (besides Dep trigger, used for ordering inside boxes).
    pub dependencies: Vec<DepEdge>,
    /// Whether job is paused.
    pub paused: bool,
    /// Alert config.
    pub alerts: AlertConfig,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
    /// Last edited timestamp.
    pub updated_at: DateTime<Utc>,
    /// Next computed fire time (None for non-time triggers).
    pub next_fire_at: Option<DateTime<Utc>>,
}

impl Job {
    /// Validate all fields.
    pub fn validate(&self) -> Result<(), CoreError> {
        validate_name(&self.name)?;
        if self.cmd.trim().is_empty() {
            return Err(CoreError::InvalidName(
                self.cmd.clone(),
                "cmd cannot be empty",
            ));
        }
        self.trigger.validate()?;
        self.retry.validate()?;
        self.exit_policy.validate()?;
        for c in &self.resource_claims {
            c.validate()?;
        }
        if self.timeout_secs != 0 && self.sla_secs != 0 && self.sla_secs > self.timeout_secs {
            return Err(CoreError::InvalidRetry("sla_secs > timeout_secs"));
        }
        if let (Some(inc), Some(exc)) = (&self.calendar_id, &self.exclude_calendar_id) {
            if inc == exc {
                return Err(CoreError::InvalidCalendar(
                    "calendar_id and exclude_calendar_id must differ",
                ));
            }
        }
        Ok(())
    }
}

fn validate_name(name: &str) -> Result<(), CoreError> {
    if name.is_empty() || name.len() > 200 {
        return Err(CoreError::InvalidName(name.into(), "len must be 1..=200"));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err(CoreError::InvalidName(
            name.into(),
            "only [A-Za-z0-9_.-] allowed",
        ));
    }
    Ok(())
}

/// Builder for ergonomic construction in tests + CLI apply.
pub struct JobBuilder {
    job: Job,
}

impl JobBuilder {
    /// New job with required fields. Other fields default.
    pub fn new(name: impl Into<String>, cmd: impl Into<String>, trigger: Trigger) -> Self {
        let now = Utc::now();
        Self {
            job: Job {
                id: JobId::new(),
                name: name.into(),
                box_id: None,
                trigger,
                cmd: cmd.into(),
                args: Vec::new(),
                env: HashMap::new(),
                cwd: None,
                shell: Shell::default(),
                target: Target::Any,
                retry: RetryPolicy {
                    max_attempts: 1,
                    backoff: crate::BackoffKind::None,
                },
                timeout_secs: 0,
                sla_secs: 0,
                calendar_id: None,
                exclude_calendar_id: None,
                must_start_times: Vec::new(),
                must_complete_times: Vec::new(),
                exit_policy: ExitCodePolicy::default(),
                resource_claims: Vec::new(),
                misfire: MisfirePolicy::default(),
                dependencies: Vec::new(),
                paused: false,
                alerts: AlertConfig::default(),
                created_at: now,
                updated_at: now,
                next_fire_at: None,
            },
        }
    }

    /// Set target.
    pub fn target(mut self, t: Target) -> Self {
        self.job.target = t;
        self
    }
    /// Set timeout in seconds.
    pub fn timeout(mut self, secs: u64) -> Self {
        self.job.timeout_secs = secs;
        self
    }
    /// Set retry policy.
    pub fn retry(mut self, r: RetryPolicy) -> Self {
        self.job.retry = r;
        self
    }
    /// Set box.
    pub fn in_box(mut self, b: BoxId) -> Self {
        self.job.box_id = Some(b);
        self
    }
    /// Build, validating.
    pub fn build(self) -> Result<Job, CoreError> {
        self.job.validate()?;
        Ok(self.job)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BackoffKind;

    fn cron_trigger() -> Trigger {
        Trigger::Cron {
            expr: "*/5 * * * *".into(),
            timezone: None,
        }
    }

    #[test]
    fn build_ok() {
        let j = JobBuilder::new("my-job", "echo hi", cron_trigger())
            .timeout(60)
            .build()
            .unwrap();
        assert_eq!(j.name, "my-job");
        assert_eq!(j.timeout_secs, 60);
    }

    #[test]
    fn empty_cmd_rejected() {
        let r = JobBuilder::new("ok", "", cron_trigger()).build();
        assert!(r.is_err());
    }

    #[test]
    fn bad_name_rejected() {
        let r = JobBuilder::new("bad name!", "echo", cron_trigger()).build();
        assert!(r.is_err());
    }

    #[test]
    fn sla_gt_timeout_rejected() {
        let mut j = JobBuilder::new("x", "echo", cron_trigger())
            .build()
            .unwrap();
        j.timeout_secs = 10;
        j.sla_secs = 20;
        assert!(j.validate().is_err());
    }

    #[test]
    fn json_roundtrip() {
        let j = JobBuilder::new("x", "echo", cron_trigger())
            .retry(RetryPolicy {
                max_attempts: 3,
                backoff: BackoffKind::Exponential {
                    base_secs: 2,
                    max_secs: 60,
                },
            })
            .build()
            .unwrap();
        let json = serde_json::to_string(&j).unwrap();
        let back: Job = serde_json::from_str(&json).unwrap();
        assert_eq!(j, back);
    }
}
