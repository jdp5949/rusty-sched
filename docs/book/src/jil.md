# JIL parser

rusty-sched ships an Autosys-compatible JIL (Job Information Language)
parser so you can bulk-import existing job definitions.

```bash
rusty-sched cli jil -f my_jobs.jil
```

The parser strips `/* ... */` comments, splits on `insert_job` /
`update_job` / `delete_job` verbs, and translates each block into a
[Job](./jobs.md). Warnings are emitted (but don't fail the apply) for:

- Unknown attributes
- Attributes that need a separate apply step (e.g. `exclude_calendar` name → CalendarId resolution)
- Box-only attributes on a non-box job

## Supported attributes

| Attribute | Maps to |
|---|---|
| `job_type: c\|box\|fw` | `Job` / `Box` / file-watcher |
| `command` | `cmd` |
| `machine` | `Target::Tag` |
| `owner` | spec-level metadata |
| `days_of_week` + `start_times` | Cron expression |
| `condition` | Stored raw → `Trigger::Condition` |
| `alarm_if_fail: y\|n` | `AlertConfig { events: [OnFailure] }` |
| `n_retrys` / `n_retries` | `retry.max_attempts = n + 1` |
| `term_run_time` (minutes) | `timeout_secs = N * 60` |
| `description` / `std_out_file` / `std_err_file` | metadata |
| `box_name` | `box_id` (resolved at apply) |
| `exclude_calendar` | `exclude_calendar_id` (name resolved at apply) |
| `must_start_times` / `must_complete_times` | `Vec<NaiveTime>` (parses HH:MM or HH:MM:SS) |
| `fail_codes` | `exit_policy.fail_codes` (CSV of ints) |
| `max_exit_success` | `exit_policy.max_exit_success` |
| `condition_code` | `exit_policy.condition_code` |
| `box_success` | `box.box_success_expr` |
| `box_failure` | `box.box_failure_expr` |
| `box_terminator: y\|n` | `box.box_terminator` |
| `job_terminator: y\|n` | `box.job_terminator` |
| `auto_hold: y\|n` | `box.auto_hold` |
| `resources` | `Vec<ResourceClaim>` — see syntax below |

## `resources:` syntax

Comma-separated entries. Each entry is `name` (defaults to 1 unit) or
`name(N)`:

```jil
resources: "db.connections(3), cpu, gpu(2)"
```

→ three claims: `db.connections × 3`, `cpu × 1`, `gpu × 2`.

## Sample

```jil
/* ------------ Nightly ETL ------------ */

insert_job: nightly_etl   job_type: c
command: /opt/etl/run.sh
machine: etl-prod-01
owner: etl-team@example.com
days_of_week: mo,tu,we,th,fr
start_times: "02:00"
exclude_calendar: us_holidays
must_start_times: "02:05"
must_complete_times: "04:00,06:00"
fail_codes: "100,101,102"
max_exit_success: 2
condition_code: 7
alarm_if_fail: y
n_retrys: 3
term_run_time: 60
resources: "db.connections(1), warehouse_slot"
condition: success(prep, 01.30) and notrunning(other) and value(IS_RUN_DAY)

/* ------------ Box around it ------------ */

insert_job: nightly_etl_box   job_type: box
description: "Container"
box_success: success(prep) and success(nightly_etl)
box_failure: failure(prep) or failure(nightly_etl)
box_terminator: y
auto_hold: y

update_job: nightly_etl
alarm_if_fail: n

delete_job: old_job
```
