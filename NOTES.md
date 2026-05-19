# Project notes — post v0.1.0 (2026-05-19)

## What shipped in v0.1.0

Single Rust binary, runs on Linux/macOS/Windows. `rusty-sched server` boots:

- Embedded SQLite (auto-located per-OS, WAL, FK enabled)
- Cron + interval + one-shot + manual trigger evaluation
- Scheduler tick loop (1s), bounded dispatch queue (10k)
- `LocalExecutor` — `tokio::process` w/ cross-platform shell selection, hard timeout, manual kill, stdout+stderr streaming
- REST API (`/api/v1/jobs` CRUD + pause/resume/trigger + run history)
- React + Tailwind dashboard at `/`, embedded in binary via `rust-embed`
- CLI client (`rusty-sched cli list|apply|trigger|pause|resume`)
- Slack + generic-webhook alerts + SLA evaluator (`OnFailure`, `OnSlaMiss`, `OnLateStart`)
- Audit-friendly `tracing` JSON logs (set `RSCHED_JSON=1`)
- Graceful shutdown (SIGINT/SIGTERM/Ctrl-C)
- systemd / launchd / Windows service installers
- GitHub Actions release pipeline → 6 platform artifacts on every `v*` tag
- 80+ unit + integration tests, CI matrix green Linux/macOS/Windows

Repo: https://github.com/jdp5949/rusty-sched
Landing: https://jdp5949.github.io/rusty-sched/
v0.1.0 release: https://github.com/jdp5949/rusty-sched/releases/tag/v0.1.0

## Deferred to later releases

Tracked here so the next session can pick up without re-deriving context.

### M7 — file + webhook triggers (next)
- `notify` crate file watcher → fires Run on filesystem event
- HTTP webhook receiver with HMAC verification + 5-minute replay dedup
- Tests: drop a file → run fires; POST → run fires
- Owns: new `rsched-triggers` crate OR extend `rsched-scheduler`

### M5.1 — auth + RBAC + audit
- Local users (bcrypt) + session cookie + CSRF
- Roles: admin / operator / viewer
- Per-route RBAC middleware
- Audit log entry on every write + login
- OpenAPI surface via `utoipa`

### M4-full — remote agent over mTLS gRPC
- `.proto` + tonic codegen
- Bidi stream: server pushes `Dispatch{run_id,cmd,env,...}`, agent streams `LogChunk` + `Result`
- mTLS via `rustls`, cert pinning by fingerprint
- `rusty-sched cert init` helper for bootstrap
- Reconnect + resume on disconnect
- Cross-platform exec already lives in `rsched-agent::LocalExecutor` — wrap in gRPC server

### M10 — Raft HA cluster
- `openraft` integration, SQLite state machine
- Snapshot via `VACUUM INTO`
- `--peers` flag + `cluster join|leave|status` commands
- Followers proxy writes to leader; only leader runs scheduler tick
- 3-node soak: kill leader mid-run, assert no missed/duplicate fires

### M11-full — signed packaging
- Apple `codesign + notarytool` for `.pkg`
- WiX `.msi` for Windows w/ proper EULA + install dir
- `cargo-deb` + `cargo-generate-rpm` for Linux native pkgs
- Homebrew tap repo (`homebrew-rusty-sched`)
- winget manifest PR to `microsoft/winget-pkgs`

### M12 — chaos + load + docs
- docker-compose w/ toxiproxy + `tc netem` for partition / latency tests
- `criterion` benches: 10k jobs / 100 runs/sec target
- mdBook docs site
- Nightly CI runs full acceptance suite

## Operational notes

### Hot paths
- `Store::jobs().due(now)` — uses partial index `idx_jobs_due` (`WHERE paused=0 AND next_fire_at IS NOT NULL`). Verify with `EXPLAIN QUERY PLAN` if scan latency creeps up.
- Dispatcher mpsc capacity = 10k. If queue overflows the tick loop logs `dispatch queue full`. Investigate dispatch consumer backlog.

### Failure modes
- LocalExecutor cannot exec missing binary → `AgentError::Io` → run marked `Failed` (no retry today; retry policy honored once retry engine is wired into the dispatch consumer — currently stubbed).
- Timeout kill is `Child::start_kill()` (SIGKILL on unix, `TerminateProcess` on Windows). Grace SIGTERM is M6.1.
- Manual trigger uses `set_next_fire_at(now)` so the tick loop picks the job up within ≤1s. Means runs always go through the cron path — uniform.

### Files of interest
- `crates/rsched-bin/src/main.rs` — single-process wiring
- `crates/rsched-scheduler/src/tick.rs` — tick loop
- `crates/rsched-agent/src/local.rs` — exec + timeout + kill
- `crates/rsched-api/src/routes.rs` — REST surface
- `crates/rsched-ui/assets/index.html` — full UI (single file)
- `installers/` — per-OS service units
- `.github/workflows/release.yml` — cross-platform build matrix

### Open questions
- Log retention: currently logs only accumulate `log_bytes` counter; full `run_logs` table writes are deferred. Decide default retention (suggest 30d) and per-job override.
- Webhook trigger replay dedup window: 5min default — confirm before M7 lands.
- Default `misfire_grace_secs` (currently 300s in `SchedulerConfig::default`) — confirm against ops use case.

## Versioning + release process

- Conventional Commits (feat/fix/chore/docs/refactor).
- Squash-merge to `main`. Branch protection enforces PR + CI + CODEOWNERS.
- Tag `vX.Y.Z` on `main` → GitHub Actions builds 6 artifacts + creates Release.
- Pre-release: tag with hyphen, e.g. `v0.2.0-rc1`.
