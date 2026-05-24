# Troubleshooting

## "401 unauthorized" on write endpoints

You're trying to POST/PUT/DELETE without credentials. Two options:

1. Browser: log in via the UI (`/`) — cookie is set automatically.
2. CLI / curl: create an API key in the **API keys** tab, then export
   `RSCHED_TOKEN=<token>` (CLI) or pass `-H "Authorization: Bearer
   <token>"` (curl).

## "First-run admin password lost"

The bootstrap-generated password is logged at WARN level on first
startup. If you missed it:

1. Stop the server.
2. Set `RSCHED_ADMIN_PASSWORD=new-password` and restart — **does not**
   reset existing users.
3. To reset admin: connect to the DB and run
   `DELETE FROM users WHERE username = 'admin';` then restart. The
   bootstrap will re-create with your env var.

## Jobs not firing

Walk the tick path:

1. `paused` is false?
2. `next_fire_at` is set + in the past? Check `GET /api/v1/jobs/:id`.
3. Calendar (include + exclude) allows now? Inspect with
   `GET /api/v1/calendars/:id`.
4. Resource claims acquirable? Check `GET /api/v1/resources` — `available`
   should be ≥ each claimed `units`.
5. Condition trigger expression resolves to `Some(true)`? Test by setting
   the globals + checking upstream run state.

Tail server logs with `RUST_LOG=info,rsched=debug`.

## "dispatch queue full"

The mpsc between tick loop and dispatcher (capacity 10k) is full. Usually
means the local executor isn't draining fast enough — too many concurrent
runs vs. CPU available. Solutions:

- Add virtual-resource claims to cap concurrency.
- Reduce the cron fan-out.
- Wait for the M4-full mTLS agent to distribute load across multiple
  hosts.

## Logs truncated at 100 MB

Hard cap per run. Set lower-noise commands or write logs to a file and
inspect via `std_out_file` reference.

## Postgres migrations fail

Symptom: `relation "jobs" does not exist` after switching `--db-url`. The
sqlx migrator is per-database — switching from SQLite to Postgres needs
a fresh Postgres database; rusty-sched doesn't auto-migrate data
between engines.

## Tests fail on macOS but pass on Linux

`ru_maxrss` is bytes on macOS and KiB on Linux — `capture_rusage_children`
handles both. If you've extended `getrusage` consumers, check the
platform branch.

## WebSocket log tail closes immediately

Behind a reverse proxy? Make sure the `Upgrade` + `Connection` headers
are forwarded — see the nginx example in [Deployment](./deployment.md).

## "the JIL parser warning says 'is not yet mapped'"

The attribute is recognized as valid JIL but not yet translated to a
`Job` field. Common ones: `date_conditions`, `timezone` (per-job),
`run_calendar`, `max_run_alarm`, `watch_file*`, `profile`, `application`,
`group`, `permission`. They're persisted in `JobSpec.warnings` for audit.
