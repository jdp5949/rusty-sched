//! rsched-alert — alert delivery + SLA evaluation.
//!
//! Implements Slack + generic Webhook channels via reqwest+rustls. SMTP is
//! stubbed for now (M6.1) — its delivery path is identical and the trait
//! `Channel` keeps the route open.

#![warn(missing_docs)]

mod channel;
mod error;
mod payload;
mod sla;

pub use channel::{deliver_all, Channel, SlackChannel, WebhookChannel};
pub use error::AlertError;
pub use payload::AlertPayload;
pub use sla::{evaluate_sla, SlaBreach};
