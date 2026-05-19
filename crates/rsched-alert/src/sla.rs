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
