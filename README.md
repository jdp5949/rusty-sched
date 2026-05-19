# rusty-sched

Autosys-class job scheduler. Single Rust binary. Zero external deps. HA cluster. Cross-platform.

> **Status:** pre-alpha / scaffold. See [v1 design spec](docs/specs/2026-05-18-rusty-sched-design.md) and [implementation plan](docs/plans/2026-05-18-rusty-sched-plan.md).

## Why

- Cron: no monitoring, no deps, no HA, no UI.
- Airflow: too complex for "just run things on a schedule."
- Autosys / Control-M: $$$, closed source, heavy ops.
- rusty-sched: one binary, web UI, agents on every host, 3-node Raft HA, runs anything (shell / ETL / app), with retries, SLAs, dependencies, calendars, alerts, audit log.

## Quickstart (target — once M10 ships)

```bash
# install
curl -fsSL https://github.com/jdp5949/rusty-sched/releases/latest/download/install.sh | sh

# 3-node cluster
rusty-sched server --peers node1:7000,node2:7000,node3:7000

# agent on every worker host
rusty-sched agent --servers node1:7000,node2:7000,node3:7000

# open http://localhost:8080
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
