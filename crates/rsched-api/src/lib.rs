//! rsched-api — HTTP/JSON REST surface (axum).
//!
//! Single-node mode. Auth + RBAC landed in v0.4 (`auth` module).

#![warn(missing_docs)]

pub mod auth;
mod error;
mod routes;
mod state;

pub use auth::seed_admin_if_empty;
pub use error::ApiError;
pub use routes::router;
pub use state::AppState;
