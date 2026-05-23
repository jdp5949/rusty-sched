//! Derive a [`BoxState`] from a box's children + their latest run states.
//!
//! Autosys boxes carry optional `box_success` and `box_failure` condition
//! expressions evaluated against children. When neither is present we fall
//! back to the default rule: **all children Success → Success**, **any child
//! Failed → Failed**, **any child Running/Queued → Running**, else **Pending**.

use rsched_conditions::{evaluate, parse, UpstreamState};
use rsched_core::{BoxState, Job, JobBox, RunState};
use std::collections::HashMap;
use std::time::Duration;

/// Pure function that maps `(box, children, child_states)` → [`BoxState`].
pub fn evaluate_box_state(
    box_def: &JobBox,
    children: &[Job],
    child_states: &HashMap<String, RunState>,
) -> BoxState {
    if box_def.paused {
        return BoxState::Paused;
    }

    // Custom failure rule wins over success rule.
    if let Some(expr_str) = &box_def.box_failure_expr {
        if let Ok(expr) = parse(expr_str) {
            let ctx = ChildStatesCtx::new(child_states);
            if evaluate(&expr, &ctx) == Some(true) {
                return BoxState::Failed;
            }
        }
    }
    if let Some(expr_str) = &box_def.box_success_expr {
        if let Ok(expr) = parse(expr_str) {
            let ctx = ChildStatesCtx::new(child_states);
            if evaluate(&expr, &ctx) == Some(true) {
                return BoxState::Success;
            }
        }
    }

    // Default rules.
    let mut any_running = false;
    let mut any_failed = false;
    let mut any_pending = false;
    let mut all_succeed = !children.is_empty();
    for c in children {
        match child_states.get(&c.name).copied() {
            Some(RunState::Success) => {}
            Some(RunState::Failed | RunState::Killed | RunState::Lost) => {
                any_failed = true;
                all_succeed = false;
            }
            Some(RunState::Running | RunState::Queued) => {
                any_running = true;
                all_succeed = false;
            }
            Some(RunState::Skipped) | None => {
                any_pending = true;
                all_succeed = false;
            }
        }
    }
    if any_failed {
        return BoxState::Failed;
    }
    if any_running {
        return BoxState::Running;
    }
    if all_succeed {
        return BoxState::Success;
    }
    if any_pending {
        return BoxState::Pending;
    }
    BoxState::Pending
}

/// `UpstreamState` impl backed by a name → latest-state map. Used internally
/// so the box-rollup expressions reuse the same evaluator as Condition triggers.
struct ChildStatesCtx<'a> {
    states: &'a HashMap<String, RunState>,
}

impl<'a> ChildStatesCtx<'a> {
    fn new(states: &'a HashMap<String, RunState>) -> Self {
        Self { states }
    }
}

impl<'a> UpstreamState for ChildStatesCtx<'a> {
    fn last_run_state(&self, job_name: &str) -> Option<RunState> {
        self.states.get(job_name).copied()
    }
    fn last_exit_code(&self, _job_name: &str) -> Option<i32> {
        None
    }
    fn is_running(&self, job_name: &str) -> bool {
        matches!(
            self.states.get(job_name).copied(),
            Some(RunState::Running | RunState::Queued)
        )
    }
    fn success_within(&self, job_name: &str, _within: Duration) -> Option<bool> {
        Some(self.states.get(job_name).copied()? == RunState::Success)
    }
    fn failure_within(&self, job_name: &str, _within: Duration) -> Option<bool> {
        Some(self.states.get(job_name).copied()? == RunState::Failed)
    }
    fn done_within(&self, job_name: &str, _within: Duration) -> Option<bool> {
        Some(self.states.get(job_name).copied()?.is_terminal())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsched_core::{BoxId, JobBuilder, Trigger};

    fn make_box(success_expr: Option<&str>, failure_expr: Option<&str>) -> JobBox {
        JobBox {
            id: BoxId::new(),
            name: "b".into(),
            parent: None,
            paused: false,
            box_success_expr: success_expr.map(String::from),
            box_failure_expr: failure_expr.map(String::from),
            box_terminator: false,
            job_terminator: false,
            auto_hold: false,
        }
    }

    fn job(name: &str) -> Job {
        JobBuilder::new(name, "echo", Trigger::Manual)
            .build()
            .unwrap()
    }

    #[test]
    fn paused_box_is_paused() {
        let mut b = make_box(None, None);
        b.paused = true;
        assert_eq!(
            evaluate_box_state(&b, &[job("a")], &HashMap::new()),
            BoxState::Paused
        );
    }

    #[test]
    fn empty_box_is_pending() {
        let b = make_box(None, None);
        assert_eq!(
            evaluate_box_state(&b, &[], &HashMap::new()),
            BoxState::Pending
        );
    }

    #[test]
    fn default_rule_all_success() {
        let b = make_box(None, None);
        let kids = vec![job("a"), job("b")];
        let mut states = HashMap::new();
        states.insert("a".into(), RunState::Success);
        states.insert("b".into(), RunState::Success);
        assert_eq!(evaluate_box_state(&b, &kids, &states), BoxState::Success);
    }

    #[test]
    fn default_rule_any_failed() {
        let b = make_box(None, None);
        let kids = vec![job("a"), job("b")];
        let mut states = HashMap::new();
        states.insert("a".into(), RunState::Success);
        states.insert("b".into(), RunState::Failed);
        assert_eq!(evaluate_box_state(&b, &kids, &states), BoxState::Failed);
    }

    #[test]
    fn default_rule_any_running() {
        let b = make_box(None, None);
        let kids = vec![job("a"), job("b")];
        let mut states = HashMap::new();
        states.insert("a".into(), RunState::Success);
        states.insert("b".into(), RunState::Running);
        assert_eq!(evaluate_box_state(&b, &kids, &states), BoxState::Running);
    }

    #[test]
    fn default_rule_pending_when_unknown() {
        let b = make_box(None, None);
        let kids = vec![job("a"), job("b")];
        let mut states = HashMap::new();
        states.insert("a".into(), RunState::Success);
        // b has no state yet.
        assert_eq!(evaluate_box_state(&b, &kids, &states), BoxState::Pending);
    }

    #[test]
    fn custom_success_expr_wins_over_default() {
        // Default would say Failed (one child failed), but the box has an
        // explicit success expr "success(a)" — only A matters.
        let b = make_box(Some("success(a)"), None);
        let kids = vec![job("a"), job("b")];
        let mut states = HashMap::new();
        states.insert("a".into(), RunState::Success);
        states.insert("b".into(), RunState::Failed);
        assert_eq!(evaluate_box_state(&b, &kids, &states), BoxState::Success);
    }

    #[test]
    fn custom_failure_expr_wins_over_success_expr() {
        // Failure expr is evaluated first.
        let b = make_box(Some("success(a)"), Some("failure(b)"));
        let kids = vec![job("a"), job("b")];
        let mut states = HashMap::new();
        states.insert("a".into(), RunState::Success);
        states.insert("b".into(), RunState::Failed);
        assert_eq!(evaluate_box_state(&b, &kids, &states), BoxState::Failed);
    }

    #[test]
    fn complex_expr_and_or() {
        let b = make_box(Some("success(a) and (success(b) or success(c))"), None);
        let kids = vec![job("a"), job("b"), job("c")];
        let mut states = HashMap::new();
        states.insert("a".into(), RunState::Success);
        states.insert("b".into(), RunState::Failed);
        states.insert("c".into(), RunState::Success);
        assert_eq!(evaluate_box_state(&b, &kids, &states), BoxState::Success);
    }
}
