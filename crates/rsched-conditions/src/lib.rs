//! rsched-conditions — Autosys-style condition expression parser and evaluator.
//!
//! Parses condition strings like `success(jobA) and (failure(jobB) or done(jobC))`
//! and evaluates them against upstream job state.

#![warn(missing_docs)]

mod eval;
mod expr;
mod parse;

pub use eval::{evaluate, UpstreamState};
pub use expr::{CmpOp, Expr};
pub use parse::{parse, ParseError};
