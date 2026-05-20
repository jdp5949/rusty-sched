//! Translate [`JobSpec`] into [`rsched_core::Job`].

use crate::spec::{JilJobType, JobSpec};
use rsched_core::{
    AlertConfig, AlertEvent, BackoffKind, Job, JobBuilder, RetryPolicy, Target, Trigger,
};

/// Result of translating a [`JobSpec`] into a [`Job`].
pub struct ParseOutput {
    /// The constructed job.
    pub job: Job,
    /// Warnings to display to the user.
    pub warnings: Vec<String>,
}

impl JobSpec {
    /// Translate this spec into a [`Job`].
    ///
    /// The `condition` field (raw Autosys dependency string) is stored only in
    /// the spec; M19 will resolve it into proper [`rsched_core::DepEdge`]s.
    /// A warning is emitted when a condition is present.
    pub fn into_job(self) -> Result<ParseOutput, rsched_core::CoreError> {
        let mut warnings = self.warnings.clone();

        let trigger = build_trigger(&self, &mut warnings);
        let cmd = self.command.clone().unwrap_or_default();

        // Box jobs don't have a real command.
        let effective_cmd = if self.job_type == JilJobType::Box {
            "__box__".to_string()
        } else {
            cmd
        };

        let target = match self.machine.as_deref() {
            Some(m) if !m.is_empty() => Target::Tag { tag: m.to_string() },
            _ => Target::Any,
        };

        let retry = build_retry(self.n_retrys);
        let timeout_secs = self.term_run_time.unwrap_or(0) * 60;

        let alerts = if self.alarm_if_fail {
            AlertConfig {
                events: vec![AlertEvent::OnFailure],
                channels: vec![],
            }
        } else {
            AlertConfig::default()
        };

        if let Some(cond) = &self.condition {
            warnings.push(format!(
                "condition {cond:?} is stored as raw string; M19 will resolve dependencies"
            ));
        }

        let builder = JobBuilder::new(&self.name, effective_cmd, trigger)
            .target(target)
            .retry(retry)
            .timeout(timeout_secs);

        // Set alerts by mutating after build to avoid missing builder method.
        let job = {
            let mut j = builder.build()?;
            j.alerts = alerts;
            j
        };

        Ok(ParseOutput { job, warnings })
    }
}

fn build_trigger(spec: &JobSpec, warnings: &mut Vec<String>) -> Trigger {
    // Try to compose a cron expression from days_of_week x start_times.
    if let Some(cron) = try_build_cron(spec) {
        return cron;
    }
    // File watcher.
    if spec.job_type == JilJobType::FileWatcher {
        if let Some(cmd) = &spec.command {
            return Trigger::File {
                path: cmd.clone(),
                event: "create".to_string(),
            };
        }
    }
    warnings.push("no schedule found; defaulting to Manual trigger".to_string());
    Trigger::Manual
}

fn try_build_cron(spec: &JobSpec) -> Option<Trigger> {
    let days = spec.days_of_week.as_deref()?;
    let times = spec.start_times.as_deref()?;

    // Take the first start time.
    let first_time = times.split(',').next()?.trim();
    let first_time = first_time.trim_matches('"');
    let (hour_str, min_str) = first_time.split_once(':')?;
    let hour: u8 = hour_str.trim().parse().ok()?;
    let min: u8 = min_str.trim().parse().ok()?;

    let dow = translate_days(days);
    let expr = format!("{min} {hour} * * {dow}");

    Some(Trigger::Cron {
        expr,
        timezone: None,
    })
}

/// Convert Autosys day abbreviations to cron DOW numbers.
fn translate_days(days: &str) -> String {
    let mut dow: Vec<&str> = Vec::new();
    for d in days.split(',') {
        match d.trim().to_ascii_lowercase().as_str() {
            "su" | "sun" => dow.push("0"),
            "mo" | "mon" => dow.push("1"),
            "tu" | "tue" => dow.push("2"),
            "we" | "wed" => dow.push("3"),
            "th" | "thu" => dow.push("4"),
            "fr" | "fri" => dow.push("5"),
            "sa" | "sat" => dow.push("6"),
            "all" => return "*".to_string(),
            _ => {}
        }
    }
    if dow.is_empty() {
        "*".to_string()
    } else {
        dow.join(",")
    }
}

fn build_retry(n_retrys: u32) -> RetryPolicy {
    RetryPolicy {
        max_attempts: n_retrys + 1,
        backoff: if n_retrys > 0 {
            BackoffKind::Fixed { delay_secs: 60 }
        } else {
            BackoffKind::None
        },
    }
}
