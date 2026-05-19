//! rsched-scheduler — cron evaluation, DAG resolution, dispatch decisions.
//!
//! No IO of its own; consumes a [`rsched_store::Store`] and emits dispatch
//! intents on a channel.

#![warn(missing_docs)]

mod cron;
mod dag;
mod dispatch;
mod error;
mod tick;

pub use cron::next_fire;
pub use dag::{deps_satisfied, has_cycle};
pub use dispatch::{DispatchIntent, Dispatcher};
pub use error::SchedulerError;
pub use tick::{tick_once, SchedulerConfig};
