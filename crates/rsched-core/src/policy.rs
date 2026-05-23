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

/// Outcome derived from an exit code via [`ExitCodePolicy`].
///
/// Distinct from [`crate::RunState`] which also covers `Killed`/`Lost`/`Skipped`
/// — outcomes only model what an exit code says about a finished run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    /// Run is considered successful.
    Success,
    /// Run is considered a failure.
    Failure,
    /// Run finished with the conditional-success code (Autosys `condition_code`).
    Conditional,
}

/// Maps a process exit code to a [`RunOutcome`].
///
/// Defaults: exit 0 is the only success; everything else is a failure.
/// Matches Autosys-style `max_exit_success`, `fail_codes`, `condition_code`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExitCodePolicy {
    /// Inclusive upper bound for `Success`. Exits in `0..=max_exit_success` succeed
    /// unless overridden by `fail_codes` or `condition_code`. Default 0.
    pub max_exit_success: i32,
    /// Exit codes that always map to `Failure` (overrides `max_exit_success`).
    pub fail_codes: Vec<i32>,
    /// Exit code mapped to `Conditional` (overrides everything else).
    pub condition_code: Option<i32>,
}

impl ExitCodePolicy {
    /// Resolve an exit code into an outcome.
    ///
    /// Precedence: `condition_code` > `fail_codes` > `<= max_exit_success`.
    pub fn evaluate(&self, exit: i32) -> RunOutcome {
        if Some(exit) == self.condition_code {
            return RunOutcome::Conditional;
        }
        if self.fail_codes.contains(&exit) {
            return RunOutcome::Failure;
        }
        if exit >= 0 && exit <= self.max_exit_success {
            return RunOutcome::Success;
        }
        RunOutcome::Failure
    }

    /// Validate.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.max_exit_success < 0 {
            return Err(CoreError::InvalidRetry("max_exit_success must be >= 0"));
        }
        if let Some(cc) = self.condition_code {
            if self.fail_codes.contains(&cc) {
                return Err(CoreError::InvalidRetry(
                    "condition_code cannot also appear in fail_codes",
                ));
            }
        }
        Ok(())
    }
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

    #[test]
    fn exit_policy_default_zero_is_success() {
        let p = ExitCodePolicy::default();
        assert_eq!(p.evaluate(0), RunOutcome::Success);
        assert_eq!(p.evaluate(1), RunOutcome::Failure);
        assert_eq!(p.evaluate(-1), RunOutcome::Failure);
    }

    #[test]
    fn exit_policy_max_exit_success_window() {
        let p = ExitCodePolicy {
            max_exit_success: 2,
            ..Default::default()
        };
        assert_eq!(p.evaluate(0), RunOutcome::Success);
        assert_eq!(p.evaluate(2), RunOutcome::Success);
        assert_eq!(p.evaluate(3), RunOutcome::Failure);
    }

    #[test]
    fn exit_policy_fail_codes_override() {
        let p = ExitCodePolicy {
            max_exit_success: 5,
            fail_codes: vec![2, 3],
            ..Default::default()
        };
        assert_eq!(p.evaluate(1), RunOutcome::Success);
        assert_eq!(p.evaluate(2), RunOutcome::Failure);
        assert_eq!(p.evaluate(3), RunOutcome::Failure);
        assert_eq!(p.evaluate(4), RunOutcome::Success);
    }

    #[test]
    fn exit_policy_condition_code_wins() {
        let p = ExitCodePolicy {
            max_exit_success: 0,
            fail_codes: vec![],
            condition_code: Some(7),
        };
        assert_eq!(p.evaluate(7), RunOutcome::Conditional);
        assert_eq!(p.evaluate(0), RunOutcome::Success);
        assert_eq!(p.evaluate(1), RunOutcome::Failure);
    }

    #[test]
    fn exit_policy_condition_and_fail_collision_rejected() {
        let p = ExitCodePolicy {
            max_exit_success: 0,
            fail_codes: vec![7],
            condition_code: Some(7),
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn exit_policy_negative_max_rejected() {
        let p = ExitCodePolicy {
            max_exit_success: -1,
            ..Default::default()
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn exit_policy_serde_roundtrip() {
        let p = ExitCodePolicy {
            max_exit_success: 4,
            fail_codes: vec![100, 101],
            condition_code: Some(2),
        };
        let s = serde_json::to_string(&p).unwrap();
        let back: ExitCodePolicy = serde_json::from_str(&s).unwrap();
        assert_eq!(p, back);
    }
}
