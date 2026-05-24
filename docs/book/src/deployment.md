# Deployment

## Single binary, SQLite (recommended for most teams)

The default. No external dependencies.

```bash
RSCHED_ADMIN_PASSWORD=change-me rusty-sched server \
  --bind 0.0.0.0:8080
```

Data lives at:

| OS | Path |
|---|---|
| Linux | `~/.local/share/io.rustysched.rusty-sched/rusty.db` |
| macOS | `~/Library/Application Support/io.rustysched.rusty-sched/rusty.db` |
| Windows | `%LOCALAPPDATA%\io\rustysched\rusty-sched\data\rusty.db` |

Override with `--db /path/to/rusty.db` or `RSCHED_DB`.

## Postgres backend

For multi-replica or external-DB deployments:

```bash
rusty-sched server --db-url postgres://user:pass@db.internal:5432/rusty
```

Migrations run automatically on startup. Schema is identical to the
SQLite version (via `sqlx::Any`).

## Service install

```bash
sudo rusty-sched service install
sudo systemctl enable --now rusty-sched-server  # linux
```

Generates:

- Linux: `/etc/systemd/system/rusty-sched-server.service`
- macOS: `/Library/LaunchDaemons/io.rustysched.server.plist`
- Windows: registered via the Service Control Manager

## Environment variables

| Var | Purpose |
|---|---|
| `RSCHED_BIND` | HTTP bind address (default `0.0.0.0:8080`) |
| `RSCHED_DB` | SQLite file path |
| `RSCHED_DB_URL` | Full DB URL (sqlite:// or postgres://) — overrides `--db` |
| `RSCHED_ADMIN_PASSWORD` | First-run admin password |
| `RSCHED_SMTP_HOST` / `_USER` / `_PASS` | SMTP for email alerts |
| `RSCHED_JSON` | Enable JSON-format `tracing` output |
| `RUST_LOG` | Standard `tracing-subscriber` filter |

## TLS

Run behind nginx / Caddy / Traefik. rusty-sched terminates HTTP only;
mutual-TLS gRPC is reserved for the M4-full remote agent.

```nginx
server {
  listen 443 ssl http2;
  server_name scheduler.example.com;

  ssl_certificate     /etc/ssl/scheduler.crt;
  ssl_certificate_key /etc/ssl/scheduler.key;

  location / {
    proxy_pass         http://127.0.0.1:8080;
    proxy_set_header   Host $host;
    proxy_set_header   Upgrade $http_upgrade;
    proxy_set_header   Connection "upgrade";
  }
}
```

(The `Upgrade` headers are required for the WebSocket log-tail endpoint.)

## Backup

SQLite: stop the server, `cp` the DB file (or use `VACUUM INTO 'backup.db'`
for an online snapshot).

Postgres: standard `pg_dump`.

## Graceful shutdown

`SIGINT` / `SIGTERM` triggers axum graceful shutdown. In-flight HTTP
requests are allowed to complete. Running child processes are NOT killed
— rusty-sched detaches and they continue. Their final state is captured
when the next scheduler boots and the agent reconnects (M4-full) or, on
the local executor, lost as `RunState::Lost`.

## Single-node vs HA

This version is **single-node**. Multi-node Raft HA (`openraft`) is
reserved for v0.8 (M10). For high availability today: run with Postgres
backend behind a load balancer using DNS failover; only one rusty-sched
process should be running at a time.
