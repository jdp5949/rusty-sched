//! rsched-api — HTTP/JSON REST surface (axum).
//!
//! Single-node mode. RBAC + session auth land in M5.1.

#![warn(missing_docs)]

mod error;
mod routes;
mod state;

pub use error::ApiError;
pub use routes::router;
pub use state::AppState;
