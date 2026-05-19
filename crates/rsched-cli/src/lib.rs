//! rsched-cli — command-line client for the REST API.

#![warn(missing_docs)]

mod client;
mod cmd;

pub use client::ApiClient;
pub use cmd::{run_cli, Cli, Cmd};
