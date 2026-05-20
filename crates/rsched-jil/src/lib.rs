//! rsched-jil — Autosys JIL (Job Information Language) parser.
//!
//! Converts JIL text blocks into [`JilBlock`] values that can be applied to
//! a rusty-sched cluster via the REST API or directly translated to
//! [`rsched_core::Job`] structs.

#![warn(missing_docs)]

pub mod error;
pub mod parse;
pub mod spec;
pub mod translate;

pub use error::JilError;
pub use parse::parse;
pub use spec::{JobSpec, PartialJobSpec};
pub use translate::ParseOutput;

#[cfg(test)]
mod tests;

/// A parsed JIL block.
#[derive(Debug, Clone, PartialEq)]
pub enum JilBlock {
    /// `insert_job: <name>  job_type: <type>` + attributes.
    Insert(JobSpec),
    /// `update_job: <name>` + partial attributes.
    Update(String, PartialJobSpec),
    /// `delete_job: <name>`.
    Delete(String),
}
