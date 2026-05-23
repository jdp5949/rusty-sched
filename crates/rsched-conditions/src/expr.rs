//! AST node types for condition expressions.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

/// Comparison operator for exitcode predicates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CmpOp {
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
    /// Less than.
    Lt,
    /// Less than or equal.
    Le,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Ge,
}

impl fmt::Display for CmpOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CmpOp::Eq => write!(f, "="),
            CmpOp::Ne => write!(f, "!="),
            CmpOp::Lt => write!(f, "<"),
            CmpOp::Le => write!(f, "<="),
            CmpOp::Gt => write!(f, ">"),
            CmpOp::Ge => write!(f, ">="),
        }
    }
}

/// Render an `Option<Duration>` look-back operand back into Autosys `, HH.MM` form.
fn lb_str(lb: &Option<Duration>) -> String {
    match lb {
        None => String::new(),
        Some(d) => {
            let total = d.as_secs() / 60;
            let h = total / 60;
            let m = total % 60;
            format!(", {h:02}.{m:02}")
        }
    }
}

/// A condition expression node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Expr {
    /// `success(job [, HH.MM])` — last run in window succeeded.
    Success(String, #[serde(default)] Option<Duration>),
    /// `failure(job [, HH.MM])`.
    Failure(String, #[serde(default)] Option<Duration>),
    /// `done(job [, HH.MM])`.
    Done(String, #[serde(default)] Option<Duration>),
    /// Job is currently running.
    Running(String),
    /// Job is not currently running.
    NotRunning(String),
    /// Last run exit code matches comparison.
    ExitCode(String, CmpOp, i32),
    /// `numrun(job [, HH.MM]) <op> N` — count of runs in window.
    NumRun(String, CmpOp, i32, #[serde(default)] Option<Duration>),
    /// `numsuc(job [, HH.MM]) <op> N` — count of successes in window.
    NumSuc(String, CmpOp, i32, #[serde(default)] Option<Duration>),
    /// `numfail(job [, HH.MM]) <op> N` — count of failures in window.
    NumFail(String, CmpOp, i32, #[serde(default)] Option<Duration>),
    /// Global variable lookup (deferred evaluation).
    Value(String),
    /// Logical AND of two sub-expressions.
    And(Box<Expr>, Box<Expr>),
    /// Logical OR of two sub-expressions.
    Or(Box<Expr>, Box<Expr>),
    /// Logical NOT of a sub-expression.
    Not(Box<Expr>),
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Success(j, lb) => write!(f, "success({j}{})", lb_str(lb)),
            Expr::Failure(j, lb) => write!(f, "failure({j}{})", lb_str(lb)),
            Expr::Done(j, lb) => write!(f, "done({j}{})", lb_str(lb)),
            Expr::Running(j) => write!(f, "running({j})"),
            Expr::NotRunning(j) => write!(f, "notrunning({j})"),
            Expr::ExitCode(j, op, n) => write!(f, "exitcode({j}) {op} {n}"),
            Expr::NumRun(j, op, n, lb) => write!(f, "numrun({j}{}) {op} {n}", lb_str(lb)),
            Expr::NumSuc(j, op, n, lb) => write!(f, "numsuc({j}{}) {op} {n}", lb_str(lb)),
            Expr::NumFail(j, op, n, lb) => write!(f, "numfail({j}{}) {op} {n}", lb_str(lb)),
            Expr::Value(name) => write!(f, "value({name})"),
            Expr::And(a, b) => write!(f, "({a} and {b})"),
            Expr::Or(a, b) => write!(f, "({a} or {b})"),
            Expr::Not(e) => write!(f, "not({e})"),
        }
    }
}

/// Helper: parse Autosys look-back format `HH.MM` (hours, minutes) into a Duration.
pub(crate) fn parse_lookback(s: &str) -> Option<Duration> {
    let (h, m) = s.split_once('.')?;
    let h: u64 = h.parse().ok()?;
    let m: u64 = m.parse().ok()?;
    if m >= 60 {
        return None;
    }
    Some(Duration::from_secs(h * 3600 + m * 60))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookback_parse() {
        assert_eq!(parse_lookback("00.30"), Some(Duration::from_secs(30 * 60)));
        assert_eq!(parse_lookback("01.30"), Some(Duration::from_secs(5400)));
        assert_eq!(parse_lookback("24.00"), Some(Duration::from_secs(86400)));
        assert_eq!(parse_lookback("0.5"), Some(Duration::from_secs(300)));
        assert!(parse_lookback("1:30").is_none());
        assert!(parse_lookback("01.60").is_none());
        assert!(parse_lookback("xx.yy").is_none());
    }

    #[test]
    fn display_with_lookback() {
        let e = Expr::Success("jobA".into(), Some(Duration::from_secs(5400)));
        assert_eq!(e.to_string(), "success(jobA, 01.30)");
    }

    #[test]
    fn display_without_lookback() {
        let e = Expr::Success("jobA".into(), None);
        assert_eq!(e.to_string(), "success(jobA)");
    }
}
