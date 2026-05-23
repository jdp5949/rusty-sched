//! rsched-core — domain types shared across the workspace.
//!
//! Pure data + small invariants. No IO, no async, no DB.

#![warn(missing_docs)]

mod r#box;
mod calendar;
mod error;
mod ids;
mod job;
mod policy;
mod run;
mod target;
mod trigger;

pub use calendar::{Calendar, CalendarRule};
pub use error::CoreError;
pub use ids::{AgentId, BoxId, CalendarId, JobId, RunId, UserId};
pub use job::{DepCondition, DepEdge, Job, JobBuilder};
pub use policy::{
    AlertChannel, AlertConfig, AlertEvent, BackoffKind, ExitCodePolicy, MisfirePolicy, RetryPolicy,
    RunOutcome, Shell,
};
pub use r#box::{Box as JobBox, BoxState};
pub use run::{Run, RunState};
pub use target::{Target, TargetKind};
pub use trigger::{Trigger, TriggerKind};
