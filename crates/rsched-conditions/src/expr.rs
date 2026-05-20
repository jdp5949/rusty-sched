//! AST node types for condition expressions.

use serde::{Deserialize, Serialize};
use std::fmt;

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

/// A condition expression node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Expr {
    /// Last run of job succeeded.
    Success(String),
    /// Last run of job failed.
    Failure(String),
    /// Last run of job is in any terminal state.
    Done(String),
    /// Job is currently running.
    Running(String),
    /// Job is not currently running.
    NotRunning(String),
    /// Last run exit code matches comparison.
    ExitCode(String, CmpOp, i32),
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
            Expr::Success(j) => write!(f, "success({j})"),
            Expr::Failure(j) => write!(f, "failure({j})"),
            Expr::Done(j) => write!(f, "done({j})"),
            Expr::Running(j) => write!(f, "running({j})"),
            Expr::NotRunning(j) => write!(f, "notrunning({j})"),
            Expr::ExitCode(j, op, n) => write!(f, "exitcode({j}) {op} {n}"),
            Expr::Value(name) => write!(f, "value({name})"),
            Expr::And(a, b) => write!(f, "({a} and {b})"),
            Expr::Or(a, b) => write!(f, "({a} or {b})"),
            Expr::Not(e) => write!(f, "not({e})"),
        }
    }
}
