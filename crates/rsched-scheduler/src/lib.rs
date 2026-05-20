//! rsched-scheduler — cron evaluation, DAG resolution, dispatch decisions.
//!
//! No IO of its own; consumes a [`rsched_store::Store`] and emits dispatch
//! intents on a channel.

#![warn(missing_docs)]

mod condition_ctx;
mod cron;
mod dag;
mod dispatch;
mod error;
mod handle_registry;
mod tick;

pub use cron::next_fire;
pub use dag::{deps_satisfied, has_cycle};
pub use dispatch::{should_retry, DispatchIntent, Dispatcher};
pub use error::SchedulerError;
pub use handle_registry::HandleRegistry;
pub use tick::{tick_once, SchedulerConfig};
