//! Job boxes — Autosys-style grouping.

use crate::BoxId;
use serde::{Deserialize, Serialize};

/// Aggregate state of a box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoxState {
    /// All children succeeded.
    Success,
    /// Any child running.
    Running,
    /// Any child failed.
    Failed,
    /// All children pending / queued.
    Pending,
    /// Box paused.
    Paused,
}

/// Box = unit of grouped jobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Box {
    /// Identifier.
    pub id: BoxId,
    /// Display name (unique).
    pub name: String,
    /// Optional parent box for nesting.
    pub parent: Option<BoxId>,
    /// Whether box (and all children) is paused.
    pub paused: bool,
}
