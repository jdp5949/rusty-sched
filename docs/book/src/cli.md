# CLI reference

The CLI is a thin client over the REST API.

```bash
rusty-sched cli --url http://localhost:8080 <subcommand>
```

Defaults: `--url` from `RSCHED_URL` env var or `http://localhost:8080`.
Auth: `RSCHED_TOKEN` env var (API key from the UI), or none for read-only.

## Job management

| Command | What it does |
|---|---|
| `list` | List jobs (id + name + paused flag) |
| `apply -f spec.yaml` | Create a job from YAML/JSON |
| `update <name> -f spec.yaml` | Replace mutable fields |
| `show <name>` | Print full job JSON |
| `delete <name>` | Delete by name or ULID |
| `trigger <name>` | Manually fire |
| `pause <name>` / `resume <name>` | Toggle paused |
| `runs <name>` | Last 100 runs table |

## Autosys parity

| Command | Equivalent |
|---|---|
| `jil -f file.jil` | Parse + apply each `insert_job` / `update_job` / `delete_job` block |
| `autorep -J <name>` | Job + 20 recent runs |
| `autorep -A` | All jobs summary table |
| `sendevent <name> STARTJOB` | `trigger` |
| `sendevent <name> KILLJOB` | Kill running run |
| `sendevent <name> ON_HOLD` / `OFF_HOLD` | `pause` / `resume` |
| `sendevent <name> CHANGE_STATUS=SUCCESS` | Force most-recent run's state |
| `sendevent <name> SEND_SIGNAL=TERM` | Forward unix signal to running PID |

## Globals

| Command | What it does |
|---|---|
| `global list` | `name=value` per line |
| `global set NAME VALUE` | Upsert |
| `global delete NAME` | Remove |

## Spec format

YAML or JSON. Same shape as `CreateJobReq`:

```yaml
name: my-job
trigger:
  kind: cron
  expr: "*/5 * * * *"
cmd: /opt/etl/run.sh
timeout_secs: 300
retry:
  max_attempts: 3
  backoff:
    kind: exponential
    base_secs: 30
    max_secs: 600
alerts:
  events: [on_failure]
  channels:
    - kind: slack
      webhook_url: "https://hooks.slack.com/services/T.../B.../..."
```

Apply:

```bash
rusty-sched cli apply -f my-job.yaml
```
