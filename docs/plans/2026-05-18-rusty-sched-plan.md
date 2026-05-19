# rusty-sched — Implementation Plan (v1)

**Date:** 2026-05-18
**Spec:** [`docs/specs/2026-05-18-rusty-sched-design.md`](../specs/2026-05-18-rusty-sched-design.md)
**Workflow:** every milestone = own feature branch + PR + review → merge to main.

## Milestones (vertical slices, each shippable)

### M0 — Scaffold + CI (foundation)
Branch: `chore/scaffold`
- Cargo workspace + all crate stubs (just `lib.rs` w/ doc comment).
- `rustfmt.toml`, `clippy.toml`, `deny.toml` (cargo-deny).
- `.github/workflows/ci.yml`: fmt + clippy + test on Linux/macOS/Windows.
- `.github/workflows/release.yml`: cargo-dist stub.
- Dockerfile (distroless) + docker-compose.yml for local 3-node + 2-agent dev.
- `Makefile` / `justfile`: `just dev`, `just test`, `just fmt`, `just lint`.
- README skeleton.
- **Exit**: green CI on empty workspace, all 3 OS.

### M1 — Core domain (rsched-core)
Branch: `feat/core-types`
- `Job`, `Run`, `Box`, `Calendar`, `Trigger` enum, `RetryPolicy`, `AlertConfig`, `Target`, `RunState`.
- Serde JSON + bincode (for Raft log).
- ULID for IDs.
- Unit tests + proptest on serialization round-trip.
- **Exit**: 80%+ coverage on core, all types serializable, doc'd.

### M2 — Store layer (rsched-store)
Branch: `feat/store-sqlite`
- sqlx migrations dir, schema from spec §6.
- Repo trait + SQLite impl. CRUD for jobs, runs, boxes, calendars, agents, users, audit.
- WAL mode, busy_timeout, foreign keys on.
- `sqlx::test` for every query.
- Index correctness checks.
- **Exit**: all repos round-trip, migrations idempotent, query plan verified for `jobs_due` hot path.

### M3 — Cron parser + scheduler core (single-node, no Raft yet)
Branch: `feat/scheduler-core`
- Cron expr parser (5+6 field), `next_fire(now)` (uses `chrono-tz` for timezones).
- Calendar engine: business-day + blackout windows.
- DAG resolver: cycle detection, deps_satisfied check.
- Tick loop (1s) + dispatch queue (bounded mpsc).
- Misfire policies.
- Property tests on cron correctness vs reference (`croner` crate as oracle).
- **Exit**: tick loop schedules jobs from in-mem store, dispatches to mock executor.

### M4 — Proto + agent + local exec
Branch: `feat/agent-grpc`
- `.proto` definitions; tonic build.
- Server-side gRPC service: bidi `JobStream(stream AgentMsg) → stream ServerMsg`.
- Agent runtime: connects to N endpoints, heartbeats, exec via `tokio::process`, streams log chunks.
- Cross-platform shell selection + kill (SIGTERM/SIGKILL on unix, taskkill on win).
- mTLS w/ rustls. Cert generation helper (`rusty-sched cert init`).
- Integration test: server + agent in same process, run real `echo` job.
- **Exit**: real job runs on real agent on Linux+mac+win, log streamed back, timeout kill works.

### M5 — REST API + auth (rsched-api)
Branch: `feat/api`
- axum routes per OpenAPI spec.
- Local users (bcrypt) + session cookie + CSRF.
- RBAC middleware per route.
- Audit log on every write.
- `/healthz`, `/readyz`, `/metrics`.
- OpenAPI doc generated (utoipa).
- **Exit**: full CRUD via curl, all roles enforced, audit log populated, integration tests green.

### M6 — Alert engine + SLA watcher (rsched-alert)
Branch: `feat/alerts`
- SMTP, Slack webhook, generic webhook adapters.
- SLA watcher task: late-start, long-run.
- Retry engine: exponential/fixed/none backoff.
- Test w/ mock SMTP + mock webhook receiver.
- **Exit**: failed run fires alert, SLA breach fires alert, retry re-dispatches.

### M7 — File + webhook triggers
Branch: `feat/triggers-non-time`
- `notify` crate file watcher.
- Webhook receiver w/ HMAC verify + 5-min replay dedup.
- Integration tests for both.
- **Exit**: drop a file → job fires; POST webhook → job fires.

### M8 — React UI (rsched-ui)
Branch: `feat/ui`
- Vite + React + TS + Tailwind + shadcn/ui.
- Pages: Login, Dashboard, Jobs, Job Detail, Runs, Run Detail (live log tail via WS), Boxes, Calendars, Agents, Users, Audit, Cluster Status.
- Dependency graph view (vis-network).
- Gantt timeline (Recharts or vis-timeline).
- Build → `rust-embed` into `rsched-ui` crate.
- Playwright E2E: login, create job, trigger, see log tail.
- **Exit**: UI shipped inside binary, served at `/`, all flows work.

### M9 — CLI (rsched-cli)
Branch: `feat/cli`
- clap v4 commands: `job apply|list|trigger|logs|pause|resume`, `run list|kill`, `agent list`, `cluster status`, `user add|passwd`, `cert init`.
- Reads `~/.rusty-sched/config.toml` for endpoint + token.
- YAML/HCL job spec format.
- **Exit**: full CRUD from CLI, scriptable.

### M10 — Raft HA (rsched-raft)
Branch: `feat/raft-ha`
- openraft integration. State machine = SQLite ops batch.
- Network layer: tonic.
- Snapshot via SQLite `VACUUM INTO`.
- `cluster join`, `cluster leave`, `cluster status` admin commands.
- Reconciler on leader change: scan running runs, query agents for status.
- 3-node integration test: kill leader mid-run, assert no missed/duplicate fires.
- **Exit**: 3-node cluster runs 1hr soak w/ random leader kills, zero data loss.

### M11 — Packaging + service install
Branch: `feat/packaging`
- cargo-dist config; release workflow builds all targets.
- systemd units, launchd plists, Windows Service via `windows-service`.
- `rusty-sched service install` auto-registers on each OS.
- WiX MSI installer.
- Homebrew tap + winget manifest in separate repos.
- Tagged 0.1.0 release w/ all artifacts.
- **Exit**: install one-liners work on Linux, macOS, Windows; `systemctl start rusty-sched-server` (etc) works.

### M12 — Chaos + load + docs
Branch: `chore/v1-hardening`
- docker-compose chaos suite (toxiproxy / tc netem).
- criterion benches at scale (10k jobs, 100 runs/sec).
- mdBook for full docs.
- v1 acceptance test runs in nightly CI.
- **Exit**: all v1 acceptance criteria from spec §10.3 pass in CI.

### M13 — GitHub Pages landing
Branch: `docs/pages`
- gh-pages branch w/ static landing site.
- Why + 60-sec quickstart + install one-liners + screenshots.
- Configure repo to publish from gh-pages.
- **Exit**: https://jdp5949.github.io/rusty-sched live.

## Ordering / parallelism
- M0 first.
- M1, M2 sequential.
- M3 depends on M1+M2.
- M4 depends on M1 (proto in M1's deps).
- M5 depends on M1+M2.
- M6, M7 depend on M3+M4+M5.
- M8 depends on M5 (consumes API).
- M9 depends on M5.
- M10 depends on M2+M3+M5 (wraps store + scheduler).
- M11 depends on M0+M9.
- M12 depends on M10+M11.
- M13 can run anytime after M0.

## Branch / PR rules (per global hard rule)
- Every milestone = own feature branch.
- Open PR, request review, merge to main ONLY after explicit user approval.
- No direct commits to main.
- Conventional Commits style.

## Definition of Done (each milestone)
- All tests green on Linux/macOS/Windows CI.
- Clippy clean (deny warnings).
- cargo-deny clean (no banned licenses / advisories).
- Coverage threshold met for touched crates.
- Docs updated.
- Changelog entry.
- User approves PR.
