//! Retry, alert, misfire, shell policies.

use crate::CoreError;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// What to do when scheduled fires were missed (server down or paused).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MisfirePolicy {
    /// Skip all missed fires.
    Skip,
    /// Fire exactly once on resume (default).
    #[default]
    FireOnce,
    /// Fire every missed schedule (rarely what you want).
    FireAllMissed,
}

/// Backoff strategy for retries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BackoffKind {
    /// No retry.
    #[default]
    None,
    /// Fixed delay between attempts.
    Fixed {
        /// Delay seconds.
        delay_secs: u64,
    },
    /// Exponential: delay = base * 2^(attempt-1), capped at max.
    Exponential {
        /// Base seconds.
        base_secs: u64,
        /// Cap seconds.
        max_secs: u64,
    },
}

impl BackoffKind {
    /// Compute the delay before attempt `n` (1-based).
    pub fn delay_for(&self, attempt: u32) -> Duration {
        match self {
            BackoffKind::None => Duration::ZERO,
            BackoffKind::Fixed { delay_secs } => Duration::from_secs(*delay_secs),
            BackoffKind::Exponential {
                base_secs,
                max_secs,
            } => {
                let exp = attempt.saturating_sub(1).min(30);
                let secs = base_secs.saturating_mul(1u64 << exp).min(*max_secs);
                Duration::from_secs(secs)
            }
        }
    }
}

/// Retry policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RetryPolicy {
    /// Max attempts including the first. 1 means no retry.
    pub max_attempts: u32,
    /// Backoff between attempts.
    pub backoff: BackoffKind,
}

impl RetryPolicy {
    /// Validate.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.max_attempts == 0 {
            return Err(CoreError::InvalidRetry("max_attempts must be >= 1"));
        }
        if self.max_attempts > 100 {
            return Err(CoreError::InvalidRetry("max_attempts cannot exceed 100"));
        }
        Ok(())
    }
}

/// Shell to wrap the command in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Shell {
    /// Pick per OS: `cmd /C` on Windows, `/bin/sh -c` on unix.
    #[default]
    Auto,
    /// Windows cmd.exe.
    Cmd,
    /// Windows powershell.
    Powershell,
    /// Posix sh.
    Sh,
    /// Bash.
    Bash,
    /// No shell — direct exec.
    None,
}

/// Events that fire alerts.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertEvent {
    /// Run failed.
    OnFailure,
    /// Run succeeded.
    OnSuccess,
    /// Run exceeded SLA seconds.
    OnSlaMiss,
    /// Run did not start by its expected time.
    OnLateStart,
    /// Run state lost (agent gone).
    OnLost,
}

/// Where to deliver alerts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AlertChannel {
    /// Email.
    Email {
        /// Recipient addresses.
        to: Vec<String>,
    },
    /// Slack incoming webhook URL.
    Slack {
        /// Webhook URL.
        webhook_url: String,
    },
    /// Generic webhook (POST JSON).
    Webhook {
        /// URL.
        url: String,
        /// Optional HMAC secret.
        secret: Option<String>,
    },
}

/// Alert config attached to a job or box.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AlertConfig {
    /// Subscribed events.
    pub events: Vec<AlertEvent>,
    /// Delivery channels.
    pub channels: Vec<AlertChannel>,
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest::proptest! {
        /// Exponential delay must never exceed max_secs, even for very large attempt numbers.
        #[test]
        fn exp_backoff_never_exceeds_max(attempt in 1u32..200, base in 1u64..100, max_s in 1u64..3600) {
            let b = BackoffKind::Exponential { base_secs: base, max_secs: max_s };
            proptest::prop_assert!(b.delay_for(attempt).as_secs() <= max_s);
        }

        /// Fixed backoff always returns exactly delay_secs regardless of attempt.
        #[test]
        fn fixed_backoff_constant(attempt in 1u32..100, delay in 0u64..3600) {
            let b = BackoffKind::Fixed { delay_secs: delay };
            proptest::prop_assert_eq!(b.delay_for(attempt).as_secs(), delay);
        }
    }

    #[test]
    fn exp_backoff_caps() {
        let b = BackoffKind::Exponential {
            base_secs: 2,
            max_secs: 60,
        };
        assert_eq!(b.delay_for(1).as_secs(), 2);
        assert_eq!(b.delay_for(2).as_secs(), 4);
        assert_eq!(b.delay_for(3).as_secs(), 8);
        assert_eq!(b.delay_for(10).as_secs(), 60); // capped
    }

    #[test]
    fn none_backoff_zero() {
        assert!(BackoffKind::None.delay_for(5).is_zero());
    }

    #[test]
    fn retry_zero_max_invalid() {
        let r = RetryPolicy {
            max_attempts: 0,
            backoff: BackoffKind::None,
        };
        assert!(r.validate().is_err());
    }
}
