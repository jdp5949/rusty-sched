# Jobs

A **Job** is the unit a user schedules. Every job has:

| Field | Purpose |
|---|---|
| `name` | Unique identifier visible in CLI, alerts, conditions, JIL. `[A-Za-z0-9_.-]{1..200}`. |
| `cmd` + `args` | The command line to execute. |
| `shell` | `auto` (default) / `sh` / `bash` / `cmd` / `powershell` / `none` (direct exec). |
| `cwd` + `env` | Working directory + environment variables. |
| `trigger` | What causes the job to run — see [Triggers](./triggers.md). |
| `target` | Where to run — currently `any` (local executor) only. mTLS gRPC agents are M4-full. |
| `timeout_secs` | Hard limit — process is killed via `start_kill()` on expiry. `0` disables. |
| `sla_secs` | Soft SLA — fires `OnSlaMiss` if the run exceeds it. `0` disables. |
| `retry` | `max_attempts` + `backoff` ({none / fixed / exponential}). |
| `misfire` | What to do for fires missed while the server was down: `skip` / `fire_once` (default) / `fire_all_missed`. |
| `paused` | Pause without delete. Tick loop skips paused jobs. |
| `alerts` | Subscribed events + delivery channels (Slack / webhook / SMTP). |
| `box_id` | Optional [Box](./boxes.md) membership. |
| `dependencies` | Upstream `DepEdge`s for ordering (in addition to `Dep` triggers). |
| `calendar_id` | Include calendar — job runs only when this allows. |
| `exclude_calendar_id` | Exclude calendar — job blocked when this allows. |
| `must_start_times` / `must_complete_times` | Wall-clock deadlines for SLA alerts. |
| `exit_policy` | `max_exit_success` + `fail_codes` + `condition_code`. |
| `resource_claims` | Virtual-resource counters to acquire before dispatch. |

## Lifecycle of a run

1. **Tick loop** finds due jobs every 1s.
2. **Calendar check** — include must allow `now`, exclude must NOT.
3. **Resource acquire** — atomic transaction over `resource_holds`.
4. **Run row inserted** (`state=Queued`).
5. **Dispatcher** receives the `DispatchIntent` and spawns the process.
6. **State → Running**, `started_at` set, logs streamed to `run_logs`.
7. **Process exits** → exit code mapped to `RunState` via `ExitCodePolicy`
   (`Success` / `Failed` / `Conditional`).
8. **Resources released**, retry attempted if policy allows.

## REST surface

| Method | Path | Notes |
|---|---|---|
| GET | `/api/v1/jobs` | List all (auth required) |
| POST | `/api/v1/jobs` | Create (admin/operator) |
| GET | `/api/v1/jobs/:id` | Fetch by id |
| GET | `/api/v1/jobs/by-name/:name` | Fetch by name |
| PUT | `/api/v1/jobs/:id` | Replace (admin/operator) |
| DELETE | `/api/v1/jobs/:id` | Delete (admin/operator) |
| POST | `/api/v1/jobs/:id/pause` | Set `paused=true` |
| POST | `/api/v1/jobs/:id/resume` | Clear `paused` |
| POST | `/api/v1/jobs/:id/trigger` | Set `next_fire_at=now` |
| GET | `/api/v1/jobs/:id/runs` | Last 100 runs |
| GET | `/api/v1/stats/jobs/:id` | success-rate + p50/p99 + sparkline outcomes |

Writes audit-log on every successful call: `job.create`, `job.update`,
`job.delete`, `job.pause`, `job.resume`, `job.trigger`.
