# Compatibility with Autosys + Cronicle

## Autosys coverage

### JIL attributes

| Attribute | Status | Notes |
|---|---|---|
| `job_type` (c/box/fw) | ✅ | |
| `command` / `machine` / `owner` | ✅ | machine → `Target::Tag` |
| `days_of_week` / `start_times` | ✅ | composed into cron |
| `condition` | ✅ | DSL evaluator wired |
| `alarm_if_fail` / `n_retrys` / `term_run_time` / `description` / `std_out_file` / `std_err_file` / `box_name` | ✅ | |
| `exclude_calendar` | ✅ | name resolved at apply step |
| `must_start_times` / `must_complete_times` | ✅ data + helper; ⏳ runtime alert dispatch | |
| `fail_codes` / `max_exit_success` / `condition_code` | ✅ | mapped to `ExitCodePolicy` |
| `box_success` / `box_failure` | ✅ data + evaluator; ⏳ runtime kill-on-failure | |
| `box_terminator` / `job_terminator` / `auto_hold` | ✅ data; ⏳ runtime | |
| `resources` | ✅ | atomic acquire/release |
| `date_conditions` / `timezone` / `run_calendar` / `max_run_alarm` / `watch_file*` / `profile` / `application` / `group` / `permission` | ⏳ recognized + warning, not yet mapped | |

### Condition DSL

| Function | Status |
|---|---|
| `success/failure/done(job)` | ✅ |
| `running/notrunning(job)` | ✅ |
| `exitcode(j) <op> N` | ✅ |
| `success/failure/done(j, HH.MM)` look-back | ✅ |
| `numrun/numsuc/numfail(j [, HH.MM]) <op> N` | ✅ |
| `value(name)` | ✅ resolves to globals |
| `and / or / not` + parens | ✅ |

### sendevent verbs

| Verb | Status |
|---|---|
| `STARTJOB` / `FORCE_STARTJOB` | ✅ |
| `KILLJOB` | ✅ |
| `ON_HOLD` / `OFF_HOLD` | ✅ |
| `CHANGE_STATUS=X` | ✅ |
| `SEND_SIGNAL=X` | ✅ (unix only) |
| `SET_GLOBAL` | ✅ via `cli global set` |
| `JOB_ON_ICE` / `JOB_OFF_ICE` | ✅ aliased to ON_HOLD/OFF_HOLD |
| `CHANGE_PRIORITY` | ⏳ not yet |
| `COMMENT` / `RUN_REPORT` | ⏳ not yet |

### autorep

- `autorep -J <name>` ✅
- `autorep -A` ✅
- `autorep -M` (machines/agents) ⏳ pending M4-full
- `autorep -B` (boxes report) ⏳

## Cronicle coverage

| Feature | Status |
|---|---|
| Web dashboard | ✅ |
| Real-time live log tail | ✅ via WebSocket |
| Dark / light theme | ✅ |
| Multi-page UI (Jobs / Resources / Globals / API keys / Users / Audit) | ✅ |
| Job filter / search | ✅ |
| Job stats (success rate + p50/p99) | ✅ from M21 |
| API keys | ✅ |
| User auth + RBAC | ✅ |
| Plugins (JSON-over-stdio) | ⏳ planned v0.7.2 |
| Multi-server failover + auto-discovery | ⏳ planned v0.8 (Raft) |
| Performance graphs (CPU/mem time series) | ⏳ end-of-run snapshot in v0.7.1; chart coming |
| Email + Slack + Webhook notifications | ✅ |

## What rusty-sched offers beyond either

- **Single Rust binary** — no Java runtime, no Node.js install, no
  Python venv.
- **Full Postgres backend** via sqlx::Any — drop-in for HA / shared state.
- **JIL → REST translation** — keep existing Autosys job definitions,
  manage them via a modern API.
- **RBAC + API keys built-in** — Cronicle's API key UX, AutoSys's
  permission model, neither requires a plugin.
- **Audit log on every mutation**, queryable via REST + UI.
- **MIT license, OSS.**
