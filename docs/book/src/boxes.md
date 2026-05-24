# Boxes

A **Box** is an Autosys-style container of jobs. The box has a derived
state computed from its children:

| State | Rule |
|---|---|
| `Paused` | Box is paused (children skipped). |
| `Failed` | Custom `box_failure_expr` evaluates true, OR default: any child failed/killed/lost. |
| `Success` | Custom `box_success_expr` evaluates true, OR default: all children succeeded. |
| `Running` | Any child queued or running (default rule). |
| `Pending` | No children, or some without a recorded state. |

Eval order: paused → custom failure expr → custom success expr → default
rules. Failure wins over success when both are set.

## Fields

| Field | Purpose |
|---|---|
| `name` | Unique identifier. |
| `parent` | Optional parent box for nesting. |
| `paused` | If true, box (and all children) is paused. |
| `box_success_expr` | Condition expression evaluated against children. |
| `box_failure_expr` | Condition expression evaluated against children. |
| `box_terminator` | Kill running children on box failure. *(runtime wiring deferred.)* |
| `job_terminator` | Kill children when containing box fails. *(runtime wiring deferred.)* |
| `auto_hold` | Auto-hold children when box transitions to Running. *(runtime wiring deferred.)* |

## Expressions

The box's success/failure expressions are the same [Condition DSL](./conditions.md)
used by `Condition` triggers — but they're evaluated against **child job
states only**, not the whole job universe. Example:

```text
box_success: success(child_a) and success(child_b)
box_failure: failure(child_a) or failure(child_b)
```

## JIL

```jil
insert_job: prod_pipeline   job_type: box
description: "Main nightly batch"
box_success: success(child_a) and success(child_b)
box_failure: failure(child_a)
box_terminator: y
auto_hold: y

insert_job: child_a   job_type: c
box_name: prod_pipeline
command: /opt/etl/a.sh

insert_job: child_b   job_type: c
box_name: prod_pipeline
command: /opt/etl/b.sh
```

## Programmatic

```rust
use rsched_scheduler::evaluate_box_state;
use std::collections::HashMap;
use rsched_core::{RunState, BoxId, JobBox};

let box_def = JobBox { /* … box_success_expr, box_failure_expr … */ };
let children = vec![/* Job records */];
let mut states = HashMap::new();
states.insert("child_a".into(), RunState::Success);
states.insert("child_b".into(), RunState::Failed);
let state = evaluate_box_state(&box_def, &children, &states);
```
