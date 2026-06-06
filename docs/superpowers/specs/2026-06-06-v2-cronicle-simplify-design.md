# rusty-sched v2 — Cronicle-model simplification + UI rebuild

**Date:** 2026-06-06
**Branch:** `feat/v2-cronicle-simplify`
**Author:** Claude (delegated decision — user stepped away, granted full decision authority)
**Status:** decision doc — user to review on return

---

## Context

User asked for a "complete rewrite using Cronicle mechanism + AutoSys + themes +
cross-OS binaries + better web UI," citing the project as "too complex and hard
to handle." User stepped away and delegated all decisions.

### What the repo actually is (verified 2026-06-06)

A working, tested job scheduler. ~12k LOC, 13 crates, 50 commits, 80+ tests, CI
green on Linux/macOS/Windows. It **already** has:

- Cronicle-style React dashboard (Jobs, DAG, Resources, Globals, API keys, Users,
  Audit), dark/light theme, live WebSocket log tail, sparklines, JSON editor.
- AutoSys parity: JIL parser, boxes, calendars, conditions DSL, `sendevent`,
  `autorep`, virtual resources, exit-code policy.
- Triggers: cron / interval / one-shot / file / webhook. Retries, SLAs, alerts
  (Slack/webhook), RBAC, audit log.
- Cross-OS release pipeline + installers + brew/winget + mdBook docs.

### Where the actual complexity lives

Not in features — in **distributed-systems machinery**:

- `rsched-raft` — openraft HA cluster skeleton.
- `rsched-agent::grpc` + `rsched-proto` — mTLS gRPC remote agents.
- Dual storage — Postgres **and** SQLite migrations maintained in parallel.
- `chaos/` + `docker-compose.chaos.yml` — toxiproxy/netem chaos suite.
- Signed packaging matrix (codesign/notarytool, WiX MSI, deb/rpm).

This is what makes the project "hard to handle" for a solo maintainer.

## Decision

**Do NOT do a destructive from-scratch rewrite.** A rewrite throws away the
hard, valuable, tested parts (cron/JIL/conditions/boxes/calendars engine) and
re-implements them for months, while the thing the user actually wants
(handleable + great UI + binaries) is achieved *better* by removing complexity.

**Approach: simplify-in-place toward the true Cronicle model, rebuild the UI,
keep the cross-OS binary pipeline.** All work on `feat/v2-cronicle-simplify`;
`main` stays intact and recoverable.

### Cut / quarantine (source of "complexity")

| Component | Action | Rationale |
|-----------|--------|-----------|
| `rsched-raft` | Remove from default build; tag-archive | Raft HA is the #1 complexity driver; Cronicle has no Raft. |
| gRPC mTLS remote agent (`grpc.rs`, `rsched-proto`) | Quarantine behind feature flag, default off | Cronicle satellites use simpler HTTP; revisit later. |
| Postgres store | Drop dual-DB; **SQLite-only** | Cronicle is single-store. Halves migration/maintenance surface. |
| `chaos/`, chaos compose | Move to `extras/`, not part of core CI | Niche; not needed to ship. |
| Signed packaging | Keep unsigned cross-OS binaries; signing later | Notarization/WiX is heavy ops; plain binaries unblock users now. |

### Keep (the value)

`rsched-core`, `rsched-scheduler`, `rsched-conditions`, `rsched-jil`,
`rsched-store` (sqlite path), `rsched-api`, `rsched-agent` (local exec),
`rsched-alert`, `rsched-cli`, `rsched-bin`.

### Cronicle mechanism (target architecture)

- **Single primary server** runs the scheduler tick + web UI + API + SQLite.
- **Optional satellite workers** join over plain HTTP(S) + bearer token (simple,
  Cronicle-like) — replaces gRPC mTLS. Deferred to a later milestone; local
  execution is the v2.0 default.
- **State**: SQLite (WAL) for everything. JSON export/import for portability.
- Web UI is the front door, not an afterthought.

### New web UI

- Cleaner, better-looking than current single-file React-via-CDN.
- **Multiple themes** (not just dark/light): e.g. Light, Dark, Midnight,
  Solarized — switchable + persisted.
- Feature-rich: dashboard/home with cluster + schedule overview, jobs, run
  history with filters, live logs, DAG, calendars, resources, globals, admin.
- Keep it **simple to maintain**: stay buildless single-file React OR minimal
  bundler — decided in plan phase. Bias to buildless to match "easy to handle."

### AutoSys

Already implemented in the engine. Work = **polish + surface in the UI** (JIL
import/export screen, condition builder, box/DAG editing, sendevent/autorep
panels). No engine rewrite.

### Cross-OS binaries

Keep `.github/workflows/release.yml` producing Linux/macOS/Windows artifacts +
`install.sh`/`install.ps1`. Drop signing for now.

## Out of scope (v2.0)

- Raft HA (archived; could return as opt-in later).
- gRPC mTLS agents (quarantined behind feature).
- Postgres.
- Code signing / notarization.

## Success criteria

1. `cargo build` / `cargo test` green with Raft + Postgres + gRPC removed from
   default path.
2. Single `rusty-sched server` boots: SQLite + scheduler + new web UI.
3. New UI: themable, covers jobs/runs/logs/DAG/calendars/resources/globals/admin,
   visibly nicer than v1.
4. Cross-OS binaries still build in CI.
5. Docs updated to reflect simplified architecture.

## Risk / reversibility

- All on a branch; `main` untouched. Removed crates recoverable from git history
  (will tag `v1-archive` before deletion).
- If user wants literal greenfield rewrite instead, this doc + branch make the
  pivot cheap.

## Phasing (detail in implementation plan)

1. **Decompose & quarantine** — remove Raft/Postgres/gRPC/chaos from default;
   green build + tests.
2. **UI rebuild** — new themable dashboard, all feature pages.
3. **AutoSys UI surfacing** — JIL screen, condition/box editors, sendevent.
4. **Satellite workers (simple HTTP)** — optional, if time.
5. **Release** — binaries, docs, README refresh.
