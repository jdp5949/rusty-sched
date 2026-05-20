//! Evaluator for condition expressions.

use crate::expr::{CmpOp, Expr};
use rsched_core::RunState;

/// Provides upstream job state for condition evaluation.
pub trait UpstreamState {
    /// Returns the last run state for a job, or None if no runs exist.
    fn last_run_state(&self, job_name: &str) -> Option<RunState>;
    /// Returns the last run's exit code, or None if unavailable.
    fn last_exit_code(&self, job_name: &str) -> Option<i32>;
    /// Returns true if the job is currently running.
    fn is_running(&self, job_name: &str) -> bool;
}

/// Evaluate an expression against upstream state.
///
/// Returns `Some(true)` / `Some(false)` if all referenced jobs are known,
/// `None` if any referenced job has no history (unknown state).
pub fn evaluate(expr: &Expr, ctx: &dyn UpstreamState) -> Option<bool> {
    match expr {
        Expr::Success(j) => Some(ctx.last_run_state(j)? == RunState::Success),
        Expr::Failure(j) => Some(ctx.last_run_state(j)? == RunState::Failed),
        Expr::Done(j) => Some(ctx.last_run_state(j)?.is_terminal()),
        Expr::Running(j) => Some(ctx.is_running(j)),
        Expr::NotRunning(j) => Some(!ctx.is_running(j)),
        Expr::ExitCode(j, op, expected) => {
            let code = ctx.last_exit_code(j)?;
            Some(apply_op(op, code, *expected))
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
    }

    fn ctx(state: RunState, code: i32) -> FakeCtx {
        FakeCtx {
            state: Some(state),
            exit_code: Some(code),
            running: state == RunState::Running,
        }
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
        let c = FakeCtx {
            state: Some(RunState::Running),
            exit_code: None,
            running: true,
        };
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
        let c = FakeCtx {
            state: None,
            exit_code: None,
            running: false,
        };
        assert_eq!(evaluate(&e, &c), None);
    }

    #[test]
    fn value_returns_none() {
        let e = parse("value(global.x)").unwrap();
        let c = FakeCtx {
            state: None,
            exit_code: None,
            running: false,
        };
        assert_eq!(evaluate(&e, &c), None);
    }
}
