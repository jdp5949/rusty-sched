# rusty-sched

Autosys-class job scheduler with a Cronicle-style web UI. Single Rust binary,
embedded SQLite, zero external deps. Cross-platform.

> **Status:** v2 (in progress) — Cronicle-model simplification. One server
> process runs the scheduler, REST API, web UI and SQLite. See the
> [v2 design doc](docs/superpowers/specs/2026-06-06-v2-cronicle-simplify-design.md).
>
> Raft HA, gRPC remote agents and Postgres were **removed in v2** to keep the
> project simple to run and maintain (Cronicle model). The `Executor` trait
> remains the seam if simple HTTP satellite workers are added later.

## Why

- Cron: no monitoring, no UI, no dependencies/retries/SLAs.
- Airflow: too heavy for "just run things on a schedule."
- Autosys / Control-M: $$$, closed source, heavy ops.
- rusty-sched: one binary, polished web UI (4 themes), runs anything
  (shell / ETL / app), with retries, SLAs, dependencies, calendars, virtual
  resources, JIL import, conditions DSL, alerts and an audit log.

## Quickstart

```bash
# install (cross-platform release binaries)
curl -fsSL https://github.com/jdp5949/rusty-sched/releases/latest/download/install.sh | sh

# run the server (SQLite auto-created in the OS data dir)
rusty-sched server

# open http://localhost:8080  (default user: admin)
```

## Build from source

```bash
git clone https://github.com/jdp5949/rusty-sched
cd rusty-sched
just ci      # fmt + clippy + test
just build
```

## License

Dual-licensed under MIT or Apache-2.0.
