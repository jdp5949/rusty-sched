//! DAG resolution: cycle detection + "are deps satisfied" check.

use rsched_core::{DepCondition, DepEdge, JobId, RunState};
use std::collections::{HashMap, HashSet};

/// Detect a cycle in a dependency adjacency map (job → upstream job ids).
/// Returns the offending job id if a cycle exists.
pub fn has_cycle(adj: &HashMap<JobId, Vec<JobId>>) -> Option<JobId> {
    let mut visiting: HashSet<JobId> = HashSet::new();
    let mut visited: HashSet<JobId> = HashSet::new();
    for &node in adj.keys() {
        if visited.contains(&node) {
            continue;
        }
        if let Some(cyc) = dfs(node, adj, &mut visiting, &mut visited) {
            return Some(cyc);
        }
    }
    None
}

fn dfs(
    node: JobId,
    adj: &HashMap<JobId, Vec<JobId>>,
    visiting: &mut HashSet<JobId>,
    visited: &mut HashSet<JobId>,
) -> Option<JobId> {
    if visiting.contains(&node) {
        return Some(node);
    }
    if visited.contains(&node) {
        return None;
    }
    visiting.insert(node);
    if let Some(deps) = adj.get(&node) {
        for &d in deps {
            if let Some(c) = dfs(d, adj, visiting, visited) {
                return Some(c);
            }
        }
    }
    visiting.remove(&node);
    visited.insert(node);
    None
}

/// Check whether all dependency edges are satisfied given the latest known
/// states of the upstream jobs.
pub fn deps_satisfied(deps: &[DepEdge], upstream_state: &HashMap<JobId, RunState>) -> bool {
    if deps.is_empty() {
        return true;
    }
    // Group by condition.
    let mut all_succeed: Vec<JobId> = Vec::new();
    let mut any_succeed: Vec<JobId> = Vec::new();
    let mut any_finish: Vec<JobId> = Vec::new();
    for e in deps {
        match e.condition {
            DepCondition::AllSucceed => all_succeed.push(e.upstream),
            DepCondition::AnySucceed => any_succeed.push(e.upstream),
            DepCondition::AnyFinish => any_finish.push(e.upstream),
        }
    }
    // AllSucceed: every listed upstream must be Success.
    for j in &all_succeed {
        match upstream_state.get(j) {
            Some(RunState::Success) => {}
            _ => return false,
        }
    }
    // AnySucceed: at least one in this group must be Success.
    if !any_succeed.is_empty()
        && !any_succeed
            .iter()
            .any(|j| matches!(upstream_state.get(j), Some(RunState::Success)))
    {
        return false;
    }
    // AnyFinish: at least one in this group must be in a terminal state.
    if !any_finish.is_empty()
        && !any_finish.iter().any(|j| {
            upstream_state
                .get(j)
                .map(|s| s.is_terminal())
                .unwrap_or(false)
        })
    {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_simple_cycle() {
        let a = JobId::new();
        let b = JobId::new();
        let mut adj = HashMap::new();
        adj.insert(a, vec![b]);
        adj.insert(b, vec![a]);
        assert!(has_cycle(&adj).is_some());
    }

    #[test]
    fn no_cycle_in_chain() {
        let a = JobId::new();
        let b = JobId::new();
        let c = JobId::new();
        let mut adj = HashMap::new();
        adj.insert(a, vec![b]);
        adj.insert(b, vec![c]);
        adj.insert(c, vec![]);
        assert!(has_cycle(&adj).is_none());
    }

    #[test]
    fn empty_deps_satisfied() {
        assert!(deps_satisfied(&[], &HashMap::new()));
    }

    #[test]
    fn all_succeed_requires_all() {
        let a = JobId::new();
        let b = JobId::new();
        let deps = vec![
            DepEdge {
                upstream: a,
                condition: DepCondition::AllSucceed,
            },
            DepEdge {
                upstream: b,
                condition: DepCondition::AllSucceed,
            },
        ];
        let mut state = HashMap::new();
        state.insert(a, RunState::Success);
        assert!(!deps_satisfied(&deps, &state));
        state.insert(b, RunState::Success);
        assert!(deps_satisfied(&deps, &state));
        state.insert(b, RunState::Failed);
        assert!(!deps_satisfied(&deps, &state));
    }

    #[test]
    fn any_succeed_one_enough() {
        let a = JobId::new();
        let b = JobId::new();
        let deps = vec![
            DepEdge {
                upstream: a,
                condition: DepCondition::AnySucceed,
            },
            DepEdge {
                upstream: b,
                condition: DepCondition::AnySucceed,
            },
        ];
        let mut state = HashMap::new();
        state.insert(a, RunState::Failed);
        state.insert(b, RunState::Success);
        assert!(deps_satisfied(&deps, &state));
    }

    #[test]
    fn any_finish_includes_failed() {
        let a = JobId::new();
        let deps = vec![DepEdge {
            upstream: a,
            condition: DepCondition::AnyFinish,
        }];
        let mut state = HashMap::new();
        state.insert(a, RunState::Failed);
        assert!(deps_satisfied(&deps, &state));
    }
}
