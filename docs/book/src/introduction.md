# Introduction

**rusty-sched** is an open-source job scheduler written in Rust. It ships as a
single binary that runs on Linux, macOS, and Windows. The goal is straightforward:
**the operational ergonomics of CA AutoSys + the dashboard UX of Cronicle, with
none of the enterprise license tax or Node.js footprint.**

## What you get out of the box

- A scheduler tick loop that fires jobs on cron / interval / one-shot / file /
  webhook / manual / DAG-dependency / condition-expression triggers.
- A REST API, a React-based dashboard, a CLI client, and an Autosys-style
  `sendevent` / `autorep` parity layer — all served from the same binary.
- Boxes (job containers) with `box_success` / `box_failure` rollup rules and
  terminator semantics.
- Exit-code policies (`max_exit_success`, `fail_codes`, `condition_code`),
  exclude calendars, `must_start_times` / `must_complete_times`.
- A condition DSL with look-back (`success(jobA, 01.30)`) and run counts
  (`numrun`, `numsuc`, `numfail`).
- Virtual resources (named counters with fixed capacity) — jobs declare
  claims; scheduler acquires atomically before dispatch.
- Global variables (`value(name)` in conditions, `sendevent SET_GLOBAL` parity).
- bcrypt users + sessions + API keys + RBAC (admin / operator / viewer) +
  audit log on every mutation.
- Live log tail over WebSocket; per-run peak RSS + CPU time captured via
  `getrusage`.
- JIL parser (Autosys text DSL) for bulk job import.
- A cross-platform installer pipeline + GitHub release artifacts on every tag.

## Who it's for

You're running batch workloads on Linux / macOS / Windows and you've
outgrown plain `cron` but don't want to:

- Pay six figures for AutoSys / Control-M / Tidal.
- Run a Node.js stack just for Cronicle's UI.
- Build a custom scheduler around Airflow / Prefect / Temporal for what is
  essentially "cron with dependencies and a web UI."

## What rusty-sched is NOT

- **A workflow engine for data pipelines.** Use Airflow / Dagster / Prefect.
- **A general-purpose actor system.** Use Temporal / Cadence.
- **A container orchestrator.** Use Kubernetes / Nomad.

rusty-sched is a *job scheduler* in the operational, batch-processing sense:
"start this command at this time on this host, retry it, watch its exit code,
alert me when it breaks, give me a dashboard."

## License + credit

MIT. Inspired by Cronicle's UX (live log tail, dashboard tabs, plugin idea)
and by Autosys's JIL grammar + box / condition semantics. Both are referenced
throughout these docs.
