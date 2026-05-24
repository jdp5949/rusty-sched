# Triggers

A trigger answers: **what causes this job to fire?** Stored on every job as
a tagged union; the scheduler walks each kind every tick.

## `cron`

Standard cron expression in 5- or 6-field form. Optional IANA timezone.

```json
{"kind": "cron", "expr": "*/5 * * * *", "timezone": "America/New_York"}
```

- Empty `expr` rejected at validation.
- Unknown timezone rejected.
- `chrono-tz` resolves DST transitions correctly.

## `interval`

Fixed period between fires.

```json
{"kind": "interval", "every": 300, "start_at": "2026-05-23T02:00:00Z"}
```

- `every` is in seconds. Cannot be zero.
- `start_at` optional — defaults to `now + every`.

## `one_shot`

Single fire at a wall-clock time.

```json
{"kind": "one_shot", "at": "2026-06-01T09:00:00Z"}
```

## `manual`

Never fires on its own. Trigger via REST `POST /jobs/:id/trigger` or CLI
`rusty-sched cli trigger <name>` or Autosys-style
`sendevent <name> STARTJOB`.

```json
{"kind": "manual"}
```

## `dep`

Fires when an upstream job's dependencies are satisfied per
`Job.dependencies` edges.

```json
{"kind": "dep", "on": ["01J...", "01J..."]}
```

Use [Condition DSL](./conditions.md) for richer logic.

## `condition`

Most flexible: an Autosys-style condition expression evaluated against
upstream job state + globals every tick.

```json
{"kind": "condition", "expr": "success(jobA, 01.30) and notrunning(jobB) and value(IS_RUN_DAY)"}
```

Parsed by `rsched-conditions`. See [Condition DSL](./conditions.md) for the
full grammar.

## `file`

Fires on filesystem event. Reserved for v0.3.3 (file watcher integration).

```json
{"kind": "file", "path": "/data/incoming", "event": "create"}
```

## `webhook`

Fires when an HTTP POST hits a server-issued URL. Reserved for v0.3.3.

```json
{"kind": "webhook", "slug": "long-opaque-path-segment", "secret": "hmac-secret-16+ chars"}
```

The slug must be ≥ 8 chars and the secret ≥ 16 chars. HMAC-SHA256 of the
body is checked against the `X-Sig` header on each request.

### Replay protection (v0.3.4+)

After HMAC verification the receiver records `sha256(body)` for the slug
in an in-process cache. Subsequent identical requests within the dedup
window return **`409 Conflict`** with
`{"error":"duplicate request within replay window"}`. This blocks an
attacker who captures a valid signed request from re-firing the job
indefinitely.

| Env var | Default | Meaning |
|---|---|---|
| `RSCHED_WEBHOOK_DEDUP_WINDOW_SECS` | `300` | TTL (seconds) of a fingerprint |
| `RSCHED_WEBHOOK_DEDUP_MAX` | `10000` | Cap on cached fingerprints (oldest evicted) |

**Limitations.** The cache is **per-process**. A multi-replica
deployment (Raft / load-balanced API) will not dedup across replicas in
v0.3.4 — a request hitting two different nodes inside the window can
still fire twice. A shared coordinator (Raft log or Redis) is planned
but out of scope for this release.

A background task prunes expired entries every 60s; entries also expire
lazily when the slot is reused.

## Misfire policy

`Job.misfire` controls what the tick loop does for fires that were missed
because the server was down or the job was paused:

| Policy | Behavior |
|---|---|
| `skip` | Drop missed fires |
| `fire_once` (default) | Fire exactly once on resume |
| `fire_all_missed` | Fire every missed fire (rare — be careful) |
