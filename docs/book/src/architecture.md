# Architecture

```
┌───────────────────────────────────────────────────────────────────────┐
│                         rusty-sched (one binary)                       │
│                                                                       │
│  ┌──────────┐    ┌───────────────┐    ┌───────────────────────────┐   │
│  │  axum    │◄──►│  rsched-api   │◄──►│   rsched-store (sqlx)     │   │
│  │  HTTP    │    │ + auth + WS   │    │  SQLite | Postgres        │   │
│  └──────────┘    └───────────────┘    └───────────────────────────┘   │
│        ▲                ▲                    ▲       ▲                 │
│        │                │                    │       │                 │
│   ┌────┴────┐       ┌───┴────────┐      ┌────┴───┐ ┌─┴────────────┐    │
│   │ rsched- │       │  rsched-   │      │ rsched-│ │  rsched-     │    │
│   │   ui    │       │ scheduler  │◄────►│ alert  │ │ conditions   │    │
│   │ React   │       │ tick + DAG │      │        │ │ (DSL eval)   │    │
│   │ embed   │       └─────┬──────┘      └────────┘ └──────────────┘    │
│   └─────────┘             │                                            │
│                           ▼                                            │
│                ┌────────────────────┐         ┌────────────────────┐   │
│                │   rsched-agent     │◄───────►│  rsched-jil        │   │
│                │  LocalExecutor     │         │  Autosys text DSL  │   │
│                │  (tokio::process)  │         └────────────────────┘   │
│                └────────────────────┘                                  │
│                                                                       │
└───────────────────────────────────────────────────────────────────────┘
```

## Crates

| Crate | Responsibility |
|---|---|
| `rsched-core` | Pure domain types — `Job`, `Run`, `Box`, `Calendar`, `Trigger`, `Resource`, `User`, `Role`. No IO. |
| `rsched-store` | sqlx repos for every entity; embedded migrations (sqlite + postgres). |
| `rsched-conditions` | Condition DSL parser + evaluator (`UpstreamState` trait). |
| `rsched-scheduler` | Tick loop, DAG resolution, cron, dispatch queue, box state evaluator, calendar filter. |
| `rsched-agent` | `Executor` trait + `LocalExecutor` (tokio::process w/ stdout/stderr streaming + rusage). |
| `rsched-alert` | Slack / webhook / SMTP channels + SLA + must_times evaluators. |
| `rsched-api` | axum router + middleware (auth, RBAC) + WebSocket log tail. |
| `rsched-ui` | React + Tailwind SPA embedded via `rust-embed`. |
| `rsched-cli` | reqwest-based REST client + Autosys-style `sendevent` / `autorep` subcommands. |
| `rsched-jil` | Autosys JIL parser + `JobSpec` → `Job` translator. |
| `rsched-bin` | Single entrypoint binary. Wires every crate together. |

## Data model

```
users ─┬─< sessions
       ├─< api_keys
       └─< audit_log
calendars ─< jobs ─┬─< runs ─┬─< run_logs
                   │         └─< resource_holds >─ resources
                   └─< dependencies (self-edge)
boxes ─< jobs
globals
```

`jobs` carries denormalized JSON blobs for `trigger`, `retry`, `alerts`,
`target`, `exit_policy`, `resource_claims`, etc. — the scheduler reads
them as a single row + serde-deserializes inline.

## Tick loop (1 Hz)

```
for now in tick():
    cals = store.calendars().list()       # ~1 query
    for job in store.jobs().due(now):     # uses partial index idx_jobs_due
        if !calendar_allows(&job, now, &cals):
            advance(job, next_cron); continue
        if !try_acquire(run.id, job.claims):
            continue
        store.runs().insert(&run)         # state=Queued
        dispatcher.try_send(intent)       # bounded mpsc capacity 10k
        advance(job, next_cron)
    # condition-trigger jobs evaluated separately
    for job in store.jobs().list():
        if Condition trigger:
            ctx = StoreUpstreamState::new(store)   # snapshot
            if evaluate(&expr, &ctx) == Some(true):
                dispatch(job)
```

## Dispatch consumer (bin/main.rs)

```
while intent in dispatch_rx:
    handle = executor.dispatch(intent)
    registry.insert(run_id, kill_tx, signal_tx)
    for chunk in handle.logs: store.run_logs().append(...)
    outcome = handle.outcome.await
    run.state = exit_policy.evaluate(outcome.exit_code)
    run.peak_rss_bytes = outcome.peak_rss_bytes
    store.runs().update(&run)
    store.resources().release(run_id)
    if should_retry(...): schedule_retry(...)
```
