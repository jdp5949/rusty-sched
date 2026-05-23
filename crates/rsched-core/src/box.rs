//! Job boxes — Autosys-style grouping.

use crate::{BoxId, CoreError};
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
    /// Optional condition expression evaluated against children — when true the box succeeds.
    /// If unset, default rule is "all children succeed". Autosys `box_success`.
    #[serde(default)]
    pub box_success_expr: Option<String>,
    /// Optional condition expression — when true the box fails. Autosys `box_failure`.
    #[serde(default)]
    pub box_failure_expr: Option<String>,
    /// If true, kill all running children when the box transitions to Failed. Autosys `box_terminator`.
    #[serde(default)]
    pub box_terminator: bool,
    /// If true, kill children when the *containing* box fails. Per-job behavior in Autosys; mirrored here
    /// at box level for the default-for-children. Autosys `job_terminator`.
    #[serde(default)]
    pub job_terminator: bool,
    /// If true, children are auto-held when the box transitions to Running and released as it advances.
    /// Autosys `auto_hold`.
    #[serde(default)]
    pub auto_hold: bool,
}

impl Box {
    /// Validate.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.name.trim().is_empty() || self.name.len() > 200 {
            return Err(CoreError::InvalidName(self.name.clone(), "len 1..=200"));
        }
        if !self
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        {
            return Err(CoreError::InvalidName(
                self.name.clone(),
                "only [A-Za-z0-9_.-] allowed",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_box(name: &str) -> Box {
        Box {
            id: BoxId::new(),
            name: name.into(),
            parent: None,
            paused: false,
            box_success_expr: None,
            box_failure_expr: None,
            box_terminator: false,
            job_terminator: false,
            auto_hold: false,
        }
    }

    #[test]
    fn validate_ok() {
        assert!(make_box("my-box").validate().is_ok());
    }

    #[test]
    fn validate_bad_name() {
        assert!(make_box("bad box!").validate().is_err());
        assert!(make_box("").validate().is_err());
    }

    #[test]
    fn defaults_disabled() {
        let b = make_box("x");
        assert!(!b.box_terminator);
        assert!(!b.job_terminator);
        assert!(!b.auto_hold);
        assert!(b.box_success_expr.is_none());
        assert!(b.box_failure_expr.is_none());
    }

    #[test]
    fn serde_roundtrip() {
        let mut b = make_box("daily-batch");
        b.box_success_expr = Some("success(child_a) and success(child_b)".into());
        b.box_failure_expr = Some("failure(child_a)".into());
        b.box_terminator = true;
        b.auto_hold = true;
        let s = serde_json::to_string(&b).unwrap();
        let back: Box = serde_json::from_str(&s).unwrap();
        assert_eq!(b, back);
    }

    #[test]
    fn old_json_backward_compat() {
        let id = BoxId::new();
        let old = serde_json::json!({
            "id": id,
            "name": "x",
            "parent": null,
            "paused": false,
        });
        let b: Box = serde_json::from_value(old).unwrap();
        assert_eq!(b.name, "x");
        assert!(!b.box_terminator);
        assert!(b.box_success_expr.is_none());
    }
}
