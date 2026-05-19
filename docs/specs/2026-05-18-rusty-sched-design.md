# rusty-sched — Design Spec (v1)

**Date:** 2026-05-18
**Status:** Draft, pending approval
**Repo:** https://github.com/jdp5949/rusty-sched

## 1. Purpose

Build a standalone, reliable, Autosys-class job scheduler with first-class web UI, multi-host execution, HA clustering, and zero external dependencies. Single binary, runs on Linux/macOS/Windows. Targets users frustrated by Airflow complexity, cron's lack of monitoring, and the unreliability/cost of commercial alternatives.

### Non-goals (v1)
- Distributed data processing (it schedules workloads, doesn't run map-reduce).
- Workflow-as-code DSL (jobs are config; code-defined DAGs deferred).
- Multi-tenant SaaS isolation (single-org install).
- OIDC/SAML SSO (v1.1).
- Plugin marketplace.

## 2. Success criteria

- One binary, no runtime deps, runs on Linux/macOS/Windows.
- 3-node HA cluster survives leader kill with zero missed/duplicate fires in 1hr soak.
- 10k active jobs, p99 dispatch latency < 200ms on a single leader.
- Agent reconnect after 5min outage correctly reconciles in-flight run state.
- Web UI: create job, view live log tail, manual trigger, pause/resume, run history — under 200ms perceived latency on local network.
- All v1 features (see §5) shipped and covered by automated tests.

## 3. Stack decisions

| Concern | Choice | Reason |
|---|---|---|
| Language | Rust (stable) | Single binary, reliability, perf, cross-compile |
| Async runtime | tokio | Standard |
| Web framework | axum | tokio-native, ergonomic |
| RPC server↔agent | tonic (gRPC) bidi-stream | Push dispatch + stream logs |
| Storage | SQLite via sqlx (bundled, WAL) | Zero deps, embeds in binary |
| HA / replication | openraft + SQLite state machine | Zero ext deps, sub-second failover |
| TLS | rustls | Pure Rust, no OpenSSL |
| UI | React + TypeScript + Tailwind + shadcn/ui | Strong UX requirement |
| UI embed | rust-embed | Single binary distribution |
| File watch | notify crate | Cross-platform |
| Cron | Hand-rolled parser + tick loop | Full control over misfire policy |
| CLI | clap v4 | Standard |
| Logging | tracing + tracing-subscriber (JSON) | Structured |
| Metrics | metrics crate → Prometheus `/metrics` | Standard |
| Packaging | cargo-dist | Industry-standard Rust release tool |

## 4. Architecture

### 4.1 Topology

```
                    ┌─────── clients (web UI, CLI, REST) ───────┐
                    │              (any node)                    │
                    ▼                                            ▼
   ┌────────────────────────────  Raft cluster  ─────────────────────────────┐
   │   ┌──────────────┐         ┌──────────────┐         ┌──────────────┐    │
   │   │  node 1      │◄──────► │  node 2      │◄──────► │  node 3      │    │
   │   │  LEADER      │  raft   │  follower    │  raft   │  follower    │    │
   │   │ tick+dispatch│         │ standby      │         │ standby      │    │
   │   │ axum API     │         │ axum API     │         │ axum API     │    │
   │   │ proxies→self │         │ proxies→ldr  │         │ proxies→ldr  │    │
   │   │ SQLite SM    │         │ SQLite SM    │         │ SQLite SM    │    │
   │   └──────┬───────┘         └──────────────┘         └──────────────┘    │
   │          │ dispatch (only leader)                                        │
   └──────────┼──────────────────────────────────────────────────────────────┘
              │ mTLS gRPC bidi-stream
   ┌──────────┼──────────┬──────────┬──────────┐
   ▼          ▼          ▼          ▼          ▼
  agent     agent      agent      agent      agent
 (each agent: list of N server endpoints, reconnect on disconnect)
```

### 4.2 Process modes (single binary)

```
rusty-sched server   --config /etc/rusty-sched/server.toml
rusty-sched agent    --config /etc/rusty-sched/agent.toml
rusty-sched cli      <command>
rusty-sched service install|uninstall|start|stop    # Win/mac/linux service mgmt
```

### 4.3 Workspace layout

```
rusty-sched/
├── Cargo.toml                  # workspace
├── crates/
│   ├── rsched-core/            # domain types: Job, Run, Box, Calendar, Trigger
│   ├── rsched-proto/           # gRPC .proto + tonic generated
│   ├── rsched-store/           # sqlx repo, migrations
│   ├── rsched-raft/            # openraft state machine
│   ├── rsched-scheduler/       # tick loop, cron, DAG, dispatcher
│   ├── rsched-alert/           # email/slack/webhook, SLA watcher
│   ├── rsched-api/             # axum routes, REST + WS log stream, auth, RBAC
│   ├── rsched-ui/              # rust-embed wrapper around React build
│   ├── rsched-agent/           # exec, log stream, heartbeat
│   ├── rsched-cli/             # clap CLI
│   └── rsched-bin/             # main.rs, mode dispatch
├── ui/                         # React + TS + Tailwind + shadcn
├── proto/                      # .proto sources
├── docs/
├── .github/workflows/          # CI + cargo-dist release
└── installers/                 # WiX (Windows), pkg (macOS), systemd/launchd units
```

### 4.4 Crate dependency graph (acyclic)

```
bin ──► api ──► scheduler ──► raft ──► store ──► core
   │     │       └─► alert ─────────────► store
   │     ├─► ui
   │     └─► cli ─► (api-client) ─► proto
   └─► agent ─► proto ─► core
```

## 5. Feature scope (v1)

### 5.1 Core scheduling
- Triggers: `Cron(expr)`, `Interval(dur)`, `OneShot(timestamp)`, `Dep(JobSelector)`, `File(path, event)`, `Webhook(secret)`, `Manual`.
- Pause / resume / disable per job and per box.
- Manual trigger with optional param override.
- Misfire policy per job: `skip`, `fire_once`, `fire_all_missed` (default `fire_once`).

### 5.2 Dependencies + boxes
- Job dependency: `run after A succeeds`, `after A or B`, `after A and B`, `after A regardless`.
- Box (Autosys-style): group of jobs treated as a unit. Box state = aggregate of children. Nested boxes supported.
- DAG cycle detection at create/edit time (reject).

### 5.3 Reliability
- Retry: max attempts + backoff (`fixed | exponential | none`).
- Timeout: hard kill (SIGTERM → grace → SIGKILL / `taskkill /F` on Windows).
- SLA: late-start alert, long-run alert (no kill).
- Calendars: business days, blackout windows, custom date lists. Run only when calendar allows.
- Alerts: email (SMTP), Slack (webhook), generic webhook. Channels per job + per box + per global. Events: `on_failure`, `on_success`, `on_sla_miss`, `on_late_start`, `on_lost`.

### 5.4 Surface
- Web UI (React/TS/Tailwind/shadcn): job list w/ filters, run history, live log tail (WebSocket), Gantt timeline of runs, dependency graph view (vis-network/Recharts), manual trigger, pause/resume, calendar editor, alert config, RBAC user mgmt, audit log viewer, cluster status (Raft leader, agent list).
- REST API: full CRUD + actions. OpenAPI spec generated.
- CLI: `rusty-sched job apply -f job.yaml`, `job list`, `job trigger NAME`, `job logs NAME --follow`, `run list`, `agent list`, `cluster status`.
- Triggers: file watcher (notify crate), webhook with HMAC.
- Auth: local users (bcrypt password) + session cookie + CSRF token. RBAC roles: `admin`, `operator`, `viewer`. Per-role permissions on every API route. Audit log of all writes + logins.
- Transport security: mTLS for agent↔server. TLS for web (BYO cert or self-signed bootstrap).

## 6. Data model (SQLite schema sketch)

```sql
users            (id, username UNIQUE, password_hash, role, created_at, disabled)
sessions         (token PK, user_id, expires_at, created_at, ip)
audit_log        (id, user_id, action, target_type, target_id, payload_json, ts)

calendars        (id, name UNIQUE, definition_json)
boxes            (id, name UNIQUE, parent_box_id NULL, paused, created_at)
jobs             (id, name UNIQUE, box_id NULL, trigger_kind, trigger_data_json,
                  cmd, args_json, env_json, cwd, shell, target_kind, target_data,
                  retry_max, retry_backoff_kind, retry_backoff_data,
                  timeout_secs, sla_secs, calendar_id NULL,
                  misfire_policy, paused, created_at, updated_at, next_fire_at,
                  alert_config_json)
dependencies     (job_id, depends_on_job_id, condition)  -- AND/OR/regardless
runs             (id ULID, job_id, agent_id NULL, state, attempt, queued_at,
                  started_at, finished_at, exit_code, parent_run_ids_json,
                  log_truncated, log_bytes)
run_logs         (run_id, seq, ts, stream, chunk)         -- append-only
agents           (id, hostname, cert_fingerprint UNIQUE, tags_json, last_seen,
                  state, version, os, arch)
raft_log, raft_state, raft_snapshot  -- managed by openraft
```

Indexes: `jobs(next_fire_at) WHERE NOT paused`, `runs(job_id, started_at DESC)`, `runs(state) WHERE state IN ('queued','running')`, `run_logs(run_id, seq)`.

## 7. Data flow

### 7.1 Write path (job create/edit/trigger)
1. Client → axum on any node.
2. Auth + RBAC check.
3. If follower: forward gRPC to leader.
4. Leader: validate (cron syntax, DAG cycle, etc.).
5. `raft.propose(Mutation)` → quorum commit → apply to local SQLite SM.
6. Return 200 + new resource. Audit log entry written via same Raft proposal.
7. Scheduler subscribes to apply stream; reloads affected job.

### 7.2 Scheduler tick (leader only, 1s)
```
now = utc_now()
due = store.jobs_due(now)              // uses next_fire_at index
for job in due:
    if !calendar.allows(now, job.cal): bump_next_fire; continue
    if !deps_satisfied(job):           continue
    run = Run::new(job, attempt=1)
    raft.propose(CreateRun(run))        // durable BEFORE dispatch
    dispatcher.enqueue(run)
    store.bump_next_fire(job)
```

### 7.3 Dispatch
1. Dispatcher pops run, picks agent matching `job.target` (specific agent_id or any agent w/ matching tag).
2. gRPC `Dispatch{run_id, cmd, env, cwd, timeout}` over open bidi stream.
3. Agent ACKs → leader proposes `UpdateRun(state=running, agent_id, started_at)`.
4. Agent streams `LogChunk{run_id, seq, ts, stream, chunk}` → server appends to `run_logs`.
5. On exit: agent sends `Result{run_id, exit_code, finished_at}`.
6. Leader proposes `UpdateRun(state=success|failed, ...)`.
7. Scheduler evaluates downstream dependents + alert engine fires alerts.

### 7.4 Failover
- Leader loss → openraft election (<1s) → new leader replays Raft log up to last applied → resumes tick.
- In-flight runs durable in log; agents reconnect to new leader and report status via `Resume{agent_id, in_flight: [run_ids]}`.
- Reconciler reconciles: agent reports running pid → state stays running; agent reports unknown → mark lost + retry per policy.

### 7.5 Read path
- Reads served from any node's local SQLite SM (stale <100ms typical).
- `?consistent=true` query param forces leader read.

## 8. Cross-platform + packaging

### 8.1 Targets (CI matrix)
| OS | Arch | Artifact |
|---|---|---|
| Linux | x86_64, aarch64 | tar.gz, .deb, .rpm, musl static, distroless Docker |
| macOS | x86_64, aarch64 | tar.gz, .pkg (signed+notarized), Homebrew tap |
| Windows | x86_64 | zip, .msi (WiX), Chocolatey, winget |

### 8.2 Service integration
- Linux: systemd units (`rusty-sched-server.service`, `rusty-sched-agent.service`).
- macOS: launchd plists in `/Library/LaunchDaemons/`.
- Windows: Windows Service via `windows-service` crate; auto-register via `rusty-sched service install`.

### 8.3 Filesystem layout (via `directories` crate)
| | Linux | macOS | Windows |
|---|---|---|---|
| Config | `/etc/rusty-sched/` | `/Library/Application Support/rusty-sched/` | `%PROGRAMDATA%\rusty-sched\` |
| Data | `/var/lib/rusty-sched/` | `/Library/Application Support/rusty-sched/data/` | `%PROGRAMDATA%\rusty-sched\data\` |
| Logs | `/var/log/rusty-sched/` | `/Library/Logs/rusty-sched/` | `%PROGRAMDATA%\rusty-sched\logs\` |

### 8.4 Cross-platform code rules
- All paths via `PathBuf`. POSIX-only behind `#[cfg(unix)]`.
- Job exec: `shell: auto|cmd|powershell|sh|bash` per job; `auto` picks per OS.
- Process kill: SIGTERM→SIGKILL on unix; `taskkill /T /F` on Windows.
- TLS: rustls only.
- SQLite: sqlx bundled feature (no system lib).

### 8.5 Install one-liners (cargo-dist generated)
```
# Linux / macOS
curl -fsSL https://github.com/jdp5949/rusty-sched/releases/latest/download/install.sh | sh
# Windows (PowerShell)
irm https://github.com/jdp5949/rusty-sched/releases/latest/download/install.ps1 | iex
```

## 9. Error handling + reliability

### 9.1 Error taxonomy
- **Transient** (network blip, sqlite busy): retry w/ jittered backoff.
- **Permanent** (bad cron, missing binary, perm denied): fail run, alert, no retry.
- **Consistency** (Raft not-leader, log conflict): transparent forward/retry.
- **User** (validation, RBAC deny): structured 4xx error.
- Each crate: own `thiserror` enum. `anyhow` only at bin layer.

### 9.2 Failure response matrix
| Failure | Detection | Response |
|---|---|---|
| Agent crash mid-run | gRPC EOF + heartbeat timeout (15s) | Mark `lost`, alert, retry per policy if `target=any` |
| Agent host reboot | TCP RST / heartbeat miss | Mark `lost`; agent reconnect confirms no pid |
| Leader crash | Raft heartbeat miss (300ms) | Election <1s, new leader resumes tick + reconciles |
| Network partition | Raft step-down on lost quorum | Old leader stops dispatch; majority elects new leader |
| SQLite corruption | Open / checksum fail | Refuse start; restore from peer snapshot |
| Disk full | sqlx write err | Reject new runs, alert, keep existing |
| Clock skew | NTP check on join | Refuse join if skew > 500ms; cron uses leader clock |
| Misfire after downtime | Scan `next_fire_at < now` on leader-up | Per-job policy: `skip` / `fire_once` / `fire_all_missed` |
| Webhook replay | Idempotency key | Dedup window 5min |
| Log storm | Per-run byte counter | Truncate at 100MB; mark `log_truncated`; keep tail |

### 9.3 Backpressure
- Per-agent `max_parallel_runs` (default 10).
- Per-server dispatch queue bounded (10k); overflow → run stays `queued`, alert.
- Log stream: agent bounded mpsc; on overflow drop oldest with `[LOG GAP n bytes]` marker.

### 9.4 Idempotency + safety
- Every Run gets ULID. Agent dedup window (1h TTL).
- At-most-once start: Raft commits dispatch decision before agent is told. If crash between commit and dispatch, reconciler re-dispatches; agent dedup rejects duplicate.
- Kill is idempotent.

### 9.5 Observability
- Logs: `tracing` JSON to stdout.
- Metrics: Prometheus `/metrics`. Key metrics: `jobs_total`, `runs_state{state}`, `dispatch_latency_seconds`, `raft_leader`, `agent_count`, `raft_log_lag`.
- Health: `/healthz` (liveness), `/readyz` (raft-joined + db-ok).
- Audit log: every write + login → table + Raft log.

## 10. Testing strategy

### 10.1 Layers
1. **Unit** — per crate, 80%+ coverage. cron parser, DAG resolver, calendar, RBAC.
2. **Integration** — `sqlx::test` with ephemeral SQLite + real scheduler.
3. **gRPC contract** — tonic test server + test agent. Resume, log stream, kill.
4. **Raft** — 3-node in-memory cluster; kill leader mid-tick; assert no missed/duplicate fires.
5. **End-to-end** — 3 servers + 2 agents in docker-compose; REST-driven; chaos: kill nodes, partition, `tc netem` slow network.
6. **Cross-platform smoke** — E2E subset on Linux/macOS/Windows CI runners.
7. **UI** — Playwright: login, create job, trigger, log tail, failover.
8. **Load** — `criterion` benches. Target: 10k jobs, 100 runs/sec, p99 dispatch < 200ms.

### 10.2 Discipline
- TDD discipline: red → green → refactor.
- No mocks for SQLite (use real ephemeral).
- Property tests (`proptest`): cron next-fire correctness, DAG cycle detection, calendar boundaries.
- Chaos suite in nightly CI; PR CI runs fast layers only.

### 10.3 v1 acceptance
- 3-node cluster: leader kill, zero missed fires in 1hr soak.
- 10k jobs, p99 dispatch < 200ms.
- Agent 5min outage → reconnect → in-flight reconciled correctly.
- Cross-platform: E2E subset green on Linux/macOS/Windows.

## 11. Open questions (defer to implementation)

- Exact Raft log compaction cadence + snapshot size threshold.
- Whether to ship a built-in `Result Set` operator like Autosys (probably no — keep core minimal).
- Log retention defaults (suggest: 30d configurable; per-job override).
- Web UI: build single-page-app vs split into auth/app bundles (defer to UI impl).

## 12. Out of scope (v1, candidates for later)

- OIDC/SAML SSO.
- Workflow-as-code (Python/TS DSL).
- Multi-tenant orgs.
- Plugin system.
- Postgres backend (design preserves option).
- Distributed log storage (S3 offload).
- Mobile app.
