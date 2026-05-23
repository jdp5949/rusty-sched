//! Translate [`JobSpec`] into [`rsched_core::Job`].

use crate::spec::{JilJobType, JobSpec};
use chrono::NaiveTime;
use rsched_core::{
    AlertConfig, AlertEvent, BackoffKind, ExitCodePolicy, Job, JobBuilder, ResourceClaim,
    RetryPolicy, Target, Trigger,
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

        let exit_policy = build_exit_policy(&self, &mut warnings);
        let must_start_times =
            parse_times_csv(&self.must_start_times, &mut warnings, "must_start_times");
        let must_complete_times = parse_times_csv(
            &self.must_complete_times,
            &mut warnings,
            "must_complete_times",
        );

        if let Some(name) = &self.exclude_calendar {
            warnings.push(format!(
                "exclude_calendar {name:?} kept as name; apply step must resolve to CalendarId"
            ));
        }
        if self.box_success.is_some()
            || self.box_failure.is_some()
            || self.box_terminator == Some(true)
            || self.job_terminator == Some(true)
            || self.auto_hold == Some(true)
        {
            if self.job_type == JilJobType::Box {
                warnings.push(
                    "box_success/box_failure/box_terminator/job_terminator/auto_hold attrs are stored on the JobSpec; apply step must propagate to the Box record".to_string(),
                );
            } else {
                warnings.push(
                    "box-only attrs (box_success/box_failure/box_terminator/auto_hold) ignored on non-box job"
                        .to_string(),
                );
            }
        }

        let builder = JobBuilder::new(&self.name, effective_cmd, trigger)
            .target(target)
            .retry(retry)
            .timeout(timeout_secs);

        let resource_claims = parse_resource_claims(&self.resources, &mut warnings);

        // Set alerts + extras by mutating after build to avoid touching builder API surface.
        let job = {
            let mut j = builder.build()?;
            j.alerts = alerts;
            j.exit_policy = exit_policy;
            j.must_start_times = must_start_times;
            j.must_complete_times = must_complete_times;
            j.resource_claims = resource_claims;
            j
        };

        Ok(ParseOutput { job, warnings })
    }
}

fn build_exit_policy(spec: &JobSpec, warnings: &mut Vec<String>) -> ExitCodePolicy {
    let mut p = ExitCodePolicy::default();
    if let Some(max) = spec.max_exit_success {
        p.max_exit_success = max;
    }
    if let Some(csv) = &spec.fail_codes {
        let codes: Vec<i32> = csv
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        p.fail_codes = codes;
    }
    if let Some(cc) = spec.condition_code {
        p.condition_code = Some(cc);
    }
    if p.validate().is_err() {
        warnings.push("exit-code policy invalid; reverting to defaults".to_string());
        return ExitCodePolicy::default();
    }
    p
}

fn parse_times_csv(raw: &Option<String>, warnings: &mut Vec<String>, attr: &str) -> Vec<NaiveTime> {
    let Some(s) = raw else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for tok in s.split(',') {
        let t = tok.trim().trim_matches('"');
        // Accept HH:MM or HH:MM:SS.
        if let Ok(parsed) = NaiveTime::parse_from_str(t, "%H:%M") {
            out.push(parsed);
        } else if let Ok(parsed) = NaiveTime::parse_from_str(t, "%H:%M:%S") {
            out.push(parsed);
        } else {
            warnings.push(format!("{attr}: failed to parse time {tok:?}"));
        }
    }
    out
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

/// Parse Autosys-style `resources` attribute into `Vec<ResourceClaim>`.
///
/// Accepts comma-separated entries where each entry is either `name` (1 unit)
/// or `name(units)`. Whitespace around tokens is ignored. Malformed entries
/// emit a warning and are skipped.
fn parse_resource_claims(raw: &Option<String>, warnings: &mut Vec<String>) -> Vec<ResourceClaim> {
    let Some(s) = raw else { return Vec::new() };
    let mut out = Vec::new();
    for entry in s.split(',') {
        let entry = entry.trim().trim_matches('"');
        if entry.is_empty() {
            continue;
        }
        let (name, units) = if let Some((n, rest)) = entry.split_once('(') {
            let n = n.trim();
            let rest = rest.trim_end_matches(')').trim();
            match rest.parse::<u32>() {
                Ok(u) if u > 0 => (n.to_string(), u),
                _ => {
                    warnings.push(format!("resources: bad units in {entry:?}"));
                    continue;
                }
            }
        } else {
            (entry.to_string(), 1u32)
        };
        if name.is_empty() {
            warnings.push(format!("resources: empty name in {entry:?}"));
            continue;
        }
        out.push(ResourceClaim {
            resource_name: name,
            units,
        });
    }
    out
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
