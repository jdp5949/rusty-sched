# Roadmap

## Shipped

- **v0.1** — Single binary core, REST, React UI, CLI, local executor,
  alerts (Slack/webhook/SMTP), cross-platform installers + release CI.
- **v0.2** — Postgres backend, JIL parser, retry engine, conditions DSL,
  inline editor, log persistence, job stats sparkline, live KILLJOB.
- **v0.3** — Autosys parity core (boxes, exit policy, look-back
  conditions, WS log tail), calendar tick filter, box state evaluator +
  must_times helpers.
- **v0.4** — bcrypt users + sessions + API keys + RBAC + audit log + UI
  login; mutation routes locked down.
- **v0.5** — Virtual resources with atomic acquire/release; REST + UI +
  JIL `resources:` syntax.
- **v0.6** — Globals + `value(name)` eval; `autorep`; sendevent
  CHANGE_STATUS + SEND_SIGNAL.
- **v0.7-alpha** — UI tabs (Resources / Globals / API keys / Users /
  Audit), dark mode toggle, job filter; per-run rusage (peak RSS + CPU).

## Next slices

| Version | Theme | Highlights |
|---|---|---|
| v0.3.3 | File + webhook triggers | `notify` watcher; HMAC webhook receiver |
| v0.4.2 | Auth polish | CSRF tokens; password change; user disable/delete |
| v0.5.2 | Resource UX | Capacity charts, resource ownership in JIL apply |
| v0.6.3 | Sendevent extras | `CHANGE_PRIORITY`, `RUN_REPORT` |
| v0.7.x | UI polish | Visual DAG editor; perf-time-series chart; plugin host |
| v0.8 | HA + packaging | Raft via `openraft`; signed `.pkg` / WiX MSI / brew / winget |
| M4-full | Remote agents | mTLS gRPC; machine load + `factor` |
| v1.0 | Hardening | Chaos suite (toxiproxy/tc netem); criterion benches (10k jobs / 100 runs/sec); nightly acceptance |

## How to contribute

PRs welcome. See [`CONTRIBUTING.md`](https://github.com/jdp5949/rusty-sched/blob/main/CONTRIBUTING.md)
in the repo for branch conventions + CI gates. Every PR must:

- Land green on the Linux + macOS + Windows test matrix
- Pass `cargo clippy --workspace --all-targets -- -D warnings`
- Pass `cargo fmt --all --check`
- Pass `cargo deny check`

## Versioning

[SemVer](https://semver.org/). Breaking changes bump major. Tag
`vX.Y.Z` on `main`; GitHub Actions builds artifacts + a release.
