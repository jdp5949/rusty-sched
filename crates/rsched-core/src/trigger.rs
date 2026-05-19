//! Trigger types — what causes a job to run.

use crate::{CoreError, JobId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Kind of trigger (used for fast SQL filtering before deser).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    /// Cron expression.
    Cron,
    /// Fixed interval between fires.
    Interval,
    /// Single fire at a wall-clock timestamp.
    OneShot,
    /// Fires when an upstream dependency completes.
    Dep,
    /// Fires on filesystem event.
    File,
    /// Fires when an HTTP webhook is POSTed.
    Webhook,
    /// Only triggered manually via API/CLI/UI.
    Manual,
}

/// Full trigger payload. Tagged enum — store as JSON alongside `kind` column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trigger {
    /// Cron expression in 5- or 6-field form. Timezone string e.g. "America/New_York".
    Cron {
        /// The cron expression.
        expr: String,
        /// IANA timezone name; UTC if absent.
        timezone: Option<String>,
    },
    /// Fixed interval.
    Interval {
        /// Period between fires.
        #[serde(with = "duration_secs")]
        every: Duration,
        /// Optional first-fire time (else: now + every).
        start_at: Option<DateTime<Utc>>,
    },
    /// One-shot at a specific time.
    OneShot {
        /// When to fire.
        at: DateTime<Utc>,
    },
    /// Fires when upstream deps satisfied (resolved by scheduler).
    Dep {
        /// Upstream job IDs (any/all logic per `Job::dependencies`).
        on: Vec<JobId>,
    },
    /// Filesystem watcher.
    File {
        /// Path to watch.
        path: String,
        /// Event mask: "create"|"modify"|"delete"|"any".
        event: String,
    },
    /// HTTP webhook receiver.
    Webhook {
        /// Random opaque path segment.
        slug: String,
        /// HMAC secret (sent as `X-Sig` header).
        secret: String,
    },
    /// Manual-only trigger.
    Manual,
}

impl Trigger {
    /// Return the discriminant kind.
    pub fn kind(&self) -> TriggerKind {
        match self {
            Trigger::Cron { .. } => TriggerKind::Cron,
            Trigger::Interval { .. } => TriggerKind::Interval,
            Trigger::OneShot { .. } => TriggerKind::OneShot,
            Trigger::Dep { .. } => TriggerKind::Dep,
            Trigger::File { .. } => TriggerKind::File,
            Trigger::Webhook { .. } => TriggerKind::Webhook,
            Trigger::Manual => TriggerKind::Manual,
        }
    }

    /// Validate shape of trigger.
    pub fn validate(&self) -> Result<(), CoreError> {
        match self {
            Trigger::Cron { expr, timezone } => {
                if expr.trim().is_empty() {
                    return Err(CoreError::InvalidCron(
                        expr.clone(),
                        "empty expression".into(),
                    ));
                }
                if let Some(tz) = timezone {
                    tz.parse::<chrono_tz::Tz>()
                        .map_err(|_| CoreError::UnknownTimezone(tz.clone()))?;
                }
                Ok(())
            }
            Trigger::Interval { every, .. } => {
                if every.is_zero() {
                    return Err(CoreError::InvalidRetry("interval cannot be zero"));
                }
                Ok(())
            }
            Trigger::File { path, event } => {
                if path.is_empty() {
                    return Err(CoreError::InvalidCalendar("file trigger needs path"));
                }
                if !matches!(event.as_str(), "create" | "modify" | "delete" | "any") {
                    return Err(CoreError::InvalidCalendar("unknown file event"));
                }
                Ok(())
            }
            Trigger::Webhook { slug, secret } => {
                if slug.len() < 8 || secret.len() < 16 {
                    return Err(CoreError::InvalidCalendar(
                        "webhook slug<8 or secret<16 chars",
                    ));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

mod duration_secs {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(d.as_secs())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let secs = u64::deserialize(d)?;
        Ok(Duration::from_secs(secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cron_kind_roundtrip() {
        let t = Trigger::Cron {
            expr: "*/5 * * * *".into(),
            timezone: Some("UTC".into()),
        };
        assert_eq!(t.kind(), TriggerKind::Cron);
        let json = serde_json::to_string(&t).unwrap();
        let back: Trigger = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn empty_cron_rejected() {
        let t = Trigger::Cron {
            expr: "".into(),
            timezone: None,
        };
        assert!(matches!(t.validate(), Err(CoreError::InvalidCron(_, _))));
    }

    #[test]
    fn zero_interval_rejected() {
        let t = Trigger::Interval {
            every: Duration::ZERO,
            start_at: None,
        };
        assert!(t.validate().is_err());
    }

    #[test]
    fn webhook_short_secret_rejected() {
        let t = Trigger::Webhook {
            slug: "abcdefgh".into(),
            secret: "short".into(),
        };
        assert!(t.validate().is_err());
    }

    #[test]
    fn unknown_tz_rejected() {
        let t = Trigger::Cron {
            expr: "* * * * *".into(),
            timezone: Some("Mars/Olympus".into()),
        };
        assert!(matches!(t.validate(), Err(CoreError::UnknownTimezone(_))));
    }
}
