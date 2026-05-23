# Project notes — post v0.1.0 (2026-05-19)

## v0.5.0 (in progress — branch `feat/v0.5-virtual-resources`, 2026-05-23)

Autosys-style virtual resources — named counters with fixed capacity.

### Shipped
- Migration `0004_resources.sql` (sqlite + postgres) — `resources`, `resource_holds`, `jobs.resource_claims_json`
- `rsched_core::{Resource, ResourceClaim, ResourceId}`
- `Job.resource_claims: Vec<ResourceClaim>` persisted in store
- `ResourceRepo` — insert/list/get_by_name/delete/available_units/try_acquire/release
- `try_acquire` is atomic (per-claim transaction): all-or-nothing, rolls back on partial failure or unknown resource name
- Scheduler tick acquires before dispatch; blocked job is left queued for the next tick (no next_fire advance — retries same fire)
- `bin` releases holds on every terminal run state

### Out of scope
- REST routes for resource CRUD (use direct DB access for now)
- UI page for resources
- JIL `resources:` attr → `Vec<ResourceClaim>` translation

## v0.3.2 (merged 2026-05-23 — PR #34)

Helpers for box rollup + must_times alerts. Pure functions only; runtime
wiring is a follow-up.

### Shipped
- `rsched_scheduler::evaluate_box_state(box, children, child_states) -> BoxState`
  Eval order: paused → custom failure expr → custom success expr → default
  rule (all success / any failed / any running / pending).
- `rsched_alert::evaluate_must_times(now, started_at, must_start_times,
  must_complete_times) -> SlaBreach`
  LateStart when all must_start_times today have passed without a start;
  SlaMiss when running past any must_complete_time that fell after start.
- 9 new tests (7 box-eval, 6 must_times).

### Out of scope
- Persisting BoxState (runtime view computed on demand for now)
- Wiring evaluator + must_times into tick loop + alert dispatch
- box_terminator runtime: cross-job kill via HandleRegistry

## v0.4.1 (merged 2026-05-23 — PR #33)

Lock down mutation routes. Anonymous writes now return 401.

### Shipped
- `RequireWrite` extractor enforced on: `POST /jobs`, `PUT /jobs/:id`,
  `DELETE /jobs/:id`, `POST /jobs/:id/pause`, `/resume`, `/trigger`,
  `DELETE /runs/:id`
- Audit log entries on every write: `job.create`, `job.update`, `job.delete`,
  `job.pause`, `job.resume`, `job.trigger`, `run.kill`
- CLI client reads `RSCHED_TOKEN` env var and sends `Authorization: Bearer …`
  on every request
- Integration tests seed an admin + session cookie and pass it to all write
  requests; new `anonymous_writes_rejected` test confirms 401 without creds

### Out of scope (deferred)
- CSRF tokens for cookie-only flow (current SameSite=Lax suffices for now)
- Password change + admin reset routes
- User disable/delete + own-key restrictions for non-admin users
- Webhook trigger endpoint auth-bypass (file/webhook triggers are M7)

## v0.7.0-alpha (merged 2026-05-23 — PR #32)

UI polish slice — Cronicle-style multi-page dashboard, theme toggle, search.

### Shipped
- Tab navigation: Jobs / API keys / Users (admin) / Audit (admin)
- Dark mode toggle (persists in localStorage, respects `prefers-color-scheme`)
- Job filter box (matches name OR command)
- `ApiKeysPage` — create / list / delete keys; plaintext token shown once
- `UsersPage` (admin) — create users with role; list with status
- `AuditPage` (admin) — recent audit entries, auto-refreshes every 10s
- Login + header + all cards picked up `dark:` variants

### Deferred → later v0.7 slices
- Visual DAG/box workflow editor
- Performance graphs (CPU/mem time series)
- Cronicle-compatible plugin host (JSON-over-stdio)

## v0.4.0 (merged 2026-05-23 — PR #31)

Auth + RBAC + API keys. First security slice.

### Shipped
- `rsched_core::auth`: `Role` (admin / operator / viewer), `User`, `ApiKey`
- `rsched_store`: `UserRepo`, `SessionRepo`, `ApiKeyRepo`, `AuditRepo` (full CRUD)
- Migration `0003_api_keys.sql` (sqlite + postgres)
- `rsched_api::auth` middleware: `rsched_session` cookie OR `Authorization: Bearer <token>`
- Extractors: `AuthUser`, `RequireWrite`, `RequireAdmin`
- Routes: `POST /auth/login`, `POST /auth/logout`, `GET /auth/me`, `GET|POST /auth/api-keys`, `DELETE /auth/api-keys/:id`, `GET|POST /users`, `GET /audit`
- bcrypt password hashing + bcrypt-hashed API key tokens (one-shot plaintext at creation)
- 12-hour session TTL with prune
- Audit log entries on login, api-key create/delete, user create
- First-run admin seed: `RSCHED_ADMIN_PASSWORD` env var (or random + log warning)
- UI: login page, header with username/role/logout

### Deferred → v0.4.1
- `RequireWrite` / `RequireAdmin` on mutation routes (kept off so existing integration tests pass; flip after follow-up that updates tests)
- CSRF tokens for cookie flow
- Password change + admin reset routes
- User disable/delete

## v0.3.1 (merged 2026-05-23)

- Tick loop honors `Job.calendar_id` (include) + `Job.exclude_calendar_id` (exclude). Blocked jobs advance `next_fire_at` and skip dispatch.

## v0.3.0 (in progress — branch `feat/v0.3-autosys-parity-core`, 2026-05-23)

Autosys parity core. Closes the largest JIL feature gap and adds a Cronicle-style
live log tail.

### Shipped in v0.3.0 slice 1
- `ExitCodePolicy` (`max_exit_success`, `fail_codes`, `condition_code`) + `RunOutcome`
- `Job.exclude_calendar_id`, `must_start_times`, `must_complete_times`
- `Box.box_success_expr`, `box_failure_expr`, `box_terminator`, `job_terminator`, `auto_hold`
- Condition DSL look-back operand `success(jobA, HH.MM)` on `success`/`failure`/`done`
- New condition fns `numrun`, `numsuc`, `numfail` (with windowed eval)
- `StoreUpstreamState` caches 200 recent runs per job → windowed counts work
- Migration `0002_autosys_parity.sql` (sqlite + postgres)
- JIL parser covers: `exclude_calendar`, `must_start_times`, `must_complete_times`,
  `fail_codes`, `max_exit_success`, `condition_code`, `box_success`, `box_failure`,
  `box_terminator`, `job_terminator`, `auto_hold`
- REST `GET /api/v1/runs/:id/logs/ws` — WebSocket live log tail
- UI auto-subscribes to WS when run detail opens; shows a "live" pulse indicator
- Run dispatcher honors `ExitCodePolicy` → `RunState` mapping
- 170+ tests pass workspace-wide (45 conditions, 38 core, 18 JIL, 14 store, …)

### Deferred from v0.3 slice 1 → v0.3.1
- Tick loop honoring `calendar_id` + `exclude_calendar_id` (data model + JIL + repo
  in place; runtime filter still TODO).
- Box success/failure expression rollup against children states.
- `must_start_times` / `must_complete_times` alert firing (data persisted; SLA
  watcher needs new code paths).
- Resolving `exclude_calendar` name → CalendarId at JIL apply time.

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
