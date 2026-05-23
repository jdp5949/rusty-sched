//! Evaluator for condition expressions.

use crate::expr::{CmpOp, Expr};
use rsched_core::RunState;
use std::time::Duration;

/// Provides upstream job state for condition evaluation.
///
/// The "within" variants take a look-back window (Autosys `, HH.MM`); the
/// non-windowed variants ask about the most recent run.
///
/// Implementors only need to implement the basic accessors; the look-back
/// variants default to ignoring the window (returning the same answer as the
/// non-windowed accessor) which keeps existing tests / mocks working.
pub trait UpstreamState {
    /// Returns the last run state for a job, or None if no runs exist.
    fn last_run_state(&self, job_name: &str) -> Option<RunState>;
    /// Returns the last run's exit code, or None if unavailable.
    fn last_exit_code(&self, job_name: &str) -> Option<i32>;
    /// Returns true if the job is currently running.
    fn is_running(&self, job_name: &str) -> bool;

    /// Returns `Some(true)` if at least one run in the look-back window succeeded.
    /// Default impl falls back to `last_run_state`, ignoring the window.
    fn success_within(&self, job_name: &str, _within: Duration) -> Option<bool> {
        Some(self.last_run_state(job_name)? == RunState::Success)
    }
    /// Returns `Some(true)` if at least one run in the look-back window failed.
    fn failure_within(&self, job_name: &str, _within: Duration) -> Option<bool> {
        Some(self.last_run_state(job_name)? == RunState::Failed)
    }
    /// Returns `Some(true)` if at least one run in the look-back window is terminal.
    fn done_within(&self, job_name: &str, _within: Duration) -> Option<bool> {
        Some(self.last_run_state(job_name)?.is_terminal())
    }
    /// Count of all runs (any terminal state + running) within window. None = unknown job.
    fn count_runs_within(&self, job_name: &str, _within: Duration) -> Option<u32> {
        self.last_run_state(job_name).map(|_| 1)
    }
    /// Count of successful runs within window.
    fn count_successes_within(&self, job_name: &str, _within: Duration) -> Option<u32> {
        Some(u32::from(self.last_run_state(job_name)? == RunState::Success))
    }
    /// Count of failed runs within window.
    fn count_failures_within(&self, job_name: &str, _within: Duration) -> Option<u32> {
        Some(u32::from(self.last_run_state(job_name)? == RunState::Failed))
    }
}

/// Evaluate an expression against upstream state.
///
/// Returns `Some(true)` / `Some(false)` if all referenced jobs are known,
/// `None` if any referenced job has no history (unknown state).
pub fn evaluate(expr: &Expr, ctx: &dyn UpstreamState) -> Option<bool> {
    match expr {
        Expr::Success(j, lb) => match lb {
            Some(d) => ctx.success_within(j, *d),
            None => Some(ctx.last_run_state(j)? == RunState::Success),
        },
        Expr::Failure(j, lb) => match lb {
            Some(d) => ctx.failure_within(j, *d),
            None => Some(ctx.last_run_state(j)? == RunState::Failed),
        },
        Expr::Done(j, lb) => match lb {
            Some(d) => ctx.done_within(j, *d),
            None => Some(ctx.last_run_state(j)?.is_terminal()),
        },
        Expr::Running(j) => Some(ctx.is_running(j)),
        Expr::NotRunning(j) => Some(!ctx.is_running(j)),
        Expr::ExitCode(j, op, expected) => {
            let code = ctx.last_exit_code(j)?;
            Some(apply_op(op, code, *expected))
        }
        Expr::NumRun(j, op, expected, lb) => {
            let window = lb.unwrap_or(Duration::from_secs(u64::MAX / 2));
            let n = ctx.count_runs_within(j, window)? as i32;
            Some(apply_op(op, n, *expected))
        }
        Expr::NumSuc(j, op, expected, lb) => {
            let window = lb.unwrap_or(Duration::from_secs(u64::MAX / 2));
            let n = ctx.count_successes_within(j, window)? as i32;
            Some(apply_op(op, n, *expected))
        }
        Expr::NumFail(j, op, expected, lb) => {
            let window = lb.unwrap_or(Duration::from_secs(u64::MAX / 2));
            let n = ctx.count_failures_within(j, window)? as i32;
            Some(apply_op(op, n, *expected))
        }
        Expr::Value(_) => None, // deferred — global var lookup not yet implemented
        Expr::And(a, b) => {
            let lhs = evaluate(a, ctx)?;
            let rhs = evaluate(b, ctx)?;
            Some(lhs && rhs)
        }
        Expr::Or(a, b) => {
            let lhs = evaluate(a, ctx)?;
            let rhs = evaluate(b, ctx)?;
            Some(lhs || rhs)
        }
        Expr::Not(e) => Some(!evaluate(e, ctx)?),
    }
}

fn apply_op(op: &CmpOp, actual: i32, expected: i32) -> bool {
    match op {
        CmpOp::Eq => actual == expected,
        CmpOp::Ne => actual != expected,
        CmpOp::Lt => actual < expected,
        CmpOp::Le => actual <= expected,
        CmpOp::Gt => actual > expected,
        CmpOp::Ge => actual >= expected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    struct FakeCtx {
        state: Option<RunState>,
        exit_code: Option<i32>,
        running: bool,
        suc_count: u32,
        fail_count: u32,
    }

    impl FakeCtx {
        fn new(state: Option<RunState>, exit_code: Option<i32>, running: bool) -> Self {
            Self {
                state,
                exit_code,
                running,
                suc_count: 0,
                fail_count: 0,
            }
        }
    }

    impl UpstreamState for FakeCtx {
        fn last_run_state(&self, _job_name: &str) -> Option<RunState> {
            self.state
        }
        fn last_exit_code(&self, _job_name: &str) -> Option<i32> {
            self.exit_code
        }
        fn is_running(&self, _job_name: &str) -> bool {
            self.running
        }
        fn count_successes_within(&self, _job_name: &str, _within: Duration) -> Option<u32> {
            Some(self.suc_count)
        }
        fn count_failures_within(&self, _job_name: &str, _within: Duration) -> Option<u32> {
            Some(self.fail_count)
        }
        fn count_runs_within(&self, _job_name: &str, _within: Duration) -> Option<u32> {
            Some(self.suc_count + self.fail_count)
        }
    }

    fn ctx(state: RunState, code: i32) -> FakeCtx {
        FakeCtx::new(Some(state), Some(code), state == RunState::Running)
    }

    #[test]
    fn success_true() {
        let e = parse("success(j)").unwrap();
        assert_eq!(evaluate(&e, &ctx(RunState::Success, 0)), Some(true));
    }

    #[test]
    fn success_false_when_failed() {
        let e = parse("success(j)").unwrap();
        assert_eq!(evaluate(&e, &ctx(RunState::Failed, 1)), Some(false));
    }

    #[test]
    fn failure_true() {
        let e = parse("failure(j)").unwrap();
        assert_eq!(evaluate(&e, &ctx(RunState::Failed, 1)), Some(true));
    }

    #[test]
    fn done_when_success() {
        let e = parse("done(j)").unwrap();
        assert_eq!(evaluate(&e, &ctx(RunState::Success, 0)), Some(true));
    }

    #[test]
    fn done_false_when_running() {
        let e = parse("done(j)").unwrap();
        assert_eq!(evaluate(&e, &ctx(RunState::Running, 0)), Some(false));
    }

    #[test]
    fn running_true() {
        let e = parse("running(j)").unwrap();
        let c = FakeCtx::new(Some(RunState::Running), None, true);
        assert_eq!(evaluate(&e, &c), Some(true));
    }

    #[test]
    fn notrunning_true_when_success() {
        let e = parse("notrunning(j)").unwrap();
        assert_eq!(evaluate(&e, &ctx(RunState::Success, 0)), Some(true));
    }

    #[test]
    fn exitcode_eq() {
        let e = parse("exitcode(j) = 0").unwrap();
        assert_eq!(evaluate(&e, &ctx(RunState::Success, 0)), Some(true));
        assert_eq!(evaluate(&e, &ctx(RunState::Failed, 1)), Some(false));
    }

    #[test]
    fn exitcode_all_ops() {
        let c = ctx(RunState::Failed, 5);
        assert_eq!(
            evaluate(&parse("exitcode(j) != 4").unwrap(), &c),
            Some(true)
        );
        assert_eq!(evaluate(&parse("exitcode(j) > 4").unwrap(), &c), Some(true));
        assert_eq!(
            evaluate(&parse("exitcode(j) >= 5").unwrap(), &c),
            Some(true)
        );
        assert_eq!(evaluate(&parse("exitcode(j) < 6").unwrap(), &c), Some(true));
        assert_eq!(
            evaluate(&parse("exitcode(j) <= 5").unwrap(), &c),
            Some(true)
        );
    }

    #[test]
    fn lookback_success_falls_back_to_last_state() {
        // Default trait impl ignores window, so behavior should match unwindowed.
        let e = parse("success(j, 01.00)").unwrap();
        assert_eq!(evaluate(&e, &ctx(RunState::Success, 0)), Some(true));
        assert_eq!(evaluate(&e, &ctx(RunState::Failed, 1)), Some(false));
    }

    #[test]
    fn numsuc_compares_against_count() {
        let mut c = FakeCtx::new(Some(RunState::Success), Some(0), false);
        c.suc_count = 3;
        c.fail_count = 1;
        assert_eq!(
            evaluate(&parse("numsuc(j, 24.00) >= 3").unwrap(), &c),
            Some(true)
        );
        assert_eq!(
            evaluate(&parse("numsuc(j, 24.00) > 3").unwrap(), &c),
            Some(false)
        );
        assert_eq!(
            evaluate(&parse("numfail(j, 24.00) = 1").unwrap(), &c),
            Some(true)
        );
        assert_eq!(
            evaluate(&parse("numrun(j, 24.00) = 4").unwrap(), &c),
            Some(true)
        );
    }

    #[test]
    fn and_both_true() {
        let e = parse("success(j) and done(j)").unwrap();
        assert_eq!(evaluate(&e, &ctx(RunState::Success, 0)), Some(true));
    }

    #[test]
    fn or_one_true() {
        let e = parse("failure(j) or success(j)").unwrap();
        assert_eq!(evaluate(&e, &ctx(RunState::Success, 0)), Some(true));
    }

    #[test]
    fn not_inverts() {
        let e = parse("not success(j)").unwrap();
        assert_eq!(evaluate(&e, &ctx(RunState::Success, 0)), Some(false));
        assert_eq!(evaluate(&e, &ctx(RunState::Failed, 1)), Some(true));
    }

    #[test]
    fn unknown_job_returns_none() {
        let e = parse("success(j)").unwrap();
        let c = FakeCtx::new(None, None, false);
        assert_eq!(evaluate(&e, &c), None);
    }

    #[test]
    fn value_returns_none() {
        let e = parse("value(global.x)").unwrap();
        let c = FakeCtx::new(None, None, false);
        assert_eq!(evaluate(&e, &c), None);
    }
}
