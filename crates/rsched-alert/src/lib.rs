//! rsched-alert — alert delivery + SLA evaluation.
//!
//! Channels: Slack (webhook), generic Webhook (POST JSON), Email (SMTP via
//! lettre + rustls). All delivered concurrently per [`deliver_all`].

#![warn(missing_docs)]

mod channel;
mod error;
mod payload;
mod sla;
mod smtp;

pub use channel::{deliver_all, Channel, SlackChannel, WebhookChannel};
pub use error::AlertError;
pub use payload::AlertPayload;
pub use sla::{evaluate_sla, SlaBreach};
pub use smtp::{EmailChannel, SmtpConfig};
