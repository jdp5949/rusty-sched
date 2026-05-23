//! SLA evaluation for running jobs.

use chrono::{DateTime, Utc};
use rsched_core::AlertEvent;

/// Result of evaluating a running run against its SLA + timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlaBreach {
    /// All good.
    None,
    /// Run has exceeded its soft SLA but is still under hard timeout.
    SlaMiss,
    /// Run has not yet started by its scheduled time.
    LateStart,
}

impl SlaBreach {
    /// Convert to an alert event (or None for `Self::None`).
    pub fn to_event(self) -> Option<AlertEvent> {
        match self {
            SlaBreach::None => None,
            SlaBreach::SlaMiss => Some(AlertEvent::OnSlaMiss),
            SlaBreach::LateStart => Some(AlertEvent::OnLateStart),
        }
    }
}

/// Compute the SLA state of a run.
///
/// `started_at`: when the run actually started (None = not yet started).
/// `scheduled_for`: when the run was scheduled (used for late-start).
/// `sla_secs`: soft SLA; 0 disables.
/// `late_start_grace_secs`: how long after `scheduled_for` we tolerate before
/// firing a late-start alert.
pub fn evaluate_sla(
    now: DateTime<Utc>,
    scheduled_for: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    sla_secs: u64,
    late_start_grace_secs: u64,
) -> SlaBreach {
    if let Some(started) = started_at {
        if sla_secs == 0 {
            return SlaBreach::None;
        }
        let elapsed = (now - started).num_seconds().max(0) as u64;
        if elapsed > sla_secs {
            return SlaBreach::SlaMiss;
        }
        SlaBreach::None
    } else {
        let waited = (now - scheduled_for).num_seconds().max(0) as u64;
        if waited > late_start_grace_secs {
            SlaBreach::LateStart
        } else {
            SlaBreach::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn no_breach_when_under_sla() {
        let now = Utc::now();
        let started = now - Duration::seconds(30);
        assert_eq!(
            evaluate_sla(now, now - Duration::seconds(120), Some(started), 60, 30),
            SlaBreach::None
        );
    }

    #[test]
    fn sla_miss_when_exceeded() {
        let now = Utc::now();
        let started = now - Duration::seconds(120);
        assert_eq!(
            evaluate_sla(now, now - Duration::seconds(200), Some(started), 60, 30),
            SlaBreach::SlaMiss
        );
    }

    #[test]
    fn late_start_when_not_started_in_time() {
        let now = Utc::now();
        let scheduled = now - Duration::seconds(120);
        assert_eq!(
            evaluate_sla(now, scheduled, None, 0, 30),
            SlaBreach::LateStart
        );
    }

    #[test]
    fn sla_zero_disables_check() {
        let now = Utc::now();
        let started = now - Duration::seconds(3600);
        assert_eq!(
            evaluate_sla(now, now - Duration::seconds(3700), Some(started), 0, 30),
            SlaBreach::None
        );
    }
}

/// Evaluate a run against Autosys-style `must_start_times` /
/// `must_complete_times` lists. Both lists are wall-clock UTC times of day;
/// today's date is taken from `now`.
///
/// Rules:
/// - If `started_at` is None and `now` has passed every `must_start_times`
///   entry for today → [`SlaBreach::LateStart`].
/// - If `started_at` is Some, run is still running, and `now` has passed any
///   `must_complete_times` entry that fell after the run started →
///   [`SlaBreach::SlaMiss`].
/// - Otherwise [`SlaBreach::None`].
pub fn evaluate_must_times(
    now: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    must_start_times: &[chrono::NaiveTime],
    must_complete_times: &[chrono::NaiveTime],
) -> SlaBreach {
    use chrono::TimeZone;
    let today = now.date_naive();
    let to_utc =
        |t: chrono::NaiveTime| -> DateTime<Utc> { Utc.from_utc_datetime(&today.and_time(t)) };

    if started_at.is_none() && !must_start_times.is_empty() {
        let all_passed = must_start_times.iter().all(|t| now > to_utc(*t));
        if all_passed {
            return SlaBreach::LateStart;
        }
        return SlaBreach::None;
    }

    if let Some(started) = started_at {
        for t in must_complete_times {
            let deadline = to_utc(*t);
            if deadline > started && now > deadline {
                return SlaBreach::SlaMiss;
            }
        }
    }
    SlaBreach::None
}

#[cfg(test)]
mod must_times_tests {
    use super::*;
    use chrono::{Duration, NaiveTime, TimeZone};

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        let today = Utc::now().date_naive();
        Utc.from_utc_datetime(&today.and_time(NaiveTime::from_hms_opt(h, m, 0).unwrap()))
    }

    #[test]
    fn no_breach_before_must_start_window() {
        let now = at(1, 30);
        let must_start = vec![NaiveTime::from_hms_opt(2, 0, 0).unwrap()];
        assert_eq!(
            evaluate_must_times(now, None, &must_start, &[]),
            SlaBreach::None
        );
    }

    #[test]
    fn late_start_when_past_all_must_start_times() {
        let now = at(3, 0);
        let must_start = vec![
            NaiveTime::from_hms_opt(2, 0, 0).unwrap(),
            NaiveTime::from_hms_opt(2, 30, 0).unwrap(),
        ];
        assert_eq!(
            evaluate_must_times(now, None, &must_start, &[]),
            SlaBreach::LateStart
        );
    }

    #[test]
    fn no_late_start_when_one_slot_still_pending() {
        let now = at(2, 15);
        let must_start = vec![
            NaiveTime::from_hms_opt(2, 0, 0).unwrap(),
            NaiveTime::from_hms_opt(2, 30, 0).unwrap(),
        ];
        // 2:00 passed but 2:30 hasn't — still some hope of starting on time.
        assert_eq!(
            evaluate_must_times(now, None, &must_start, &[]),
            SlaBreach::None
        );
    }

    #[test]
    fn sla_miss_when_past_must_complete() {
        let now = at(4, 30);
        let started = Some(at(2, 0));
        let must_complete = vec![NaiveTime::from_hms_opt(4, 0, 0).unwrap()];
        assert_eq!(
            evaluate_must_times(now, started, &[], &must_complete),
            SlaBreach::SlaMiss
        );
    }

    #[test]
    fn no_sla_miss_before_must_complete() {
        let now = at(3, 30);
        let started = Some(at(2, 0));
        let must_complete = vec![NaiveTime::from_hms_opt(4, 0, 0).unwrap()];
        assert_eq!(
            evaluate_must_times(now, started, &[], &must_complete),
            SlaBreach::None
        );
    }

    #[test]
    fn must_complete_before_start_ignored() {
        let now = at(4, 30);
        let started = Some(at(4, 0));
        // 3:00 must_complete is BEFORE the run started — irrelevant.
        let must_complete = vec![NaiveTime::from_hms_opt(3, 0, 0).unwrap()];
        assert_eq!(
            evaluate_must_times(now, started, &[], &must_complete),
            SlaBreach::None
        );
    }

    #[test]
    fn empty_must_times_lists_never_fire() {
        let now = at(12, 0);
        assert_eq!(evaluate_must_times(now, None, &[], &[]), SlaBreach::None);
        assert_eq!(
            evaluate_must_times(now, Some(at(10, 0)), &[], &[]),
            SlaBreach::None
        );
        let _ = Duration::seconds(0); // silence unused import warning
    }
}
