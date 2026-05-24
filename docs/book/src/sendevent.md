# `sendevent` / `autorep`

The two main Autosys CLI verbs are mapped to native rusty-sched calls.

## `sendevent`

`sendevent <JOB> <EVENT>`. Resolves the job by name (or ULID) and dispatches.

| Event | Aliases | Action |
|---|---|---|
| `STARTJOB` | `START`, `FORCE_STARTJOB` | Trigger a manual run |
| `KILLJOB` | `KILL` | `SIGKILL` the most recent running run |
| `ON_HOLD` | `HOLD`, `OFF_ICE` | Pause |
| `OFF_HOLD` | `UNHOLD`, `ON_ICE` | Resume |
| `CHANGE_STATUS=<state>` | — | Force most recent run to `success/failed/killed/lost/skipped` |
| `SEND_SIGNAL=<sig>` | — | Forward unix signal (`TERM`, `HUP`, `USR1`, `15`, …) |
| `SET_GLOBAL` | — | Returns hint: use `cli global set NAME VALUE` instead |

```bash
rusty-sched cli sendevent prod_etl STARTJOB
rusty-sched cli sendevent prod_etl SEND_SIGNAL=TERM
rusty-sched cli sendevent prod_etl CHANGE_STATUS=SUCCESS
rusty-sched cli sendevent prod_etl ON_HOLD
```

## `autorep`

`autorep -J <name>` or `autorep -A` for the all-jobs summary.

```bash
rusty-sched cli autorep -J nightly_etl
# Job: nightly_etl
#   id:           01H8...
#   state:        active
#   cmd:          /opt/etl/run.sh
#   next_fire_at: 2026-05-24T02:00:00Z
# Recent runs:
#   01H...  Success  exit=0  started=2026-05-23T02:00:01Z
#   01H...  Failed   exit=7  started=2026-05-22T02:00:00Z

rusty-sched cli autorep -A
# JOB_NAME             STATE      LAST_RUN   NEXT_FIRE
# nightly_etl          active     Success    2026-05-24T02:00:00Z
# downstream           ON_HOLD    -          -
```

## REST equivalents

| Event | REST |
|---|---|
| `STARTJOB` | `POST /api/v1/jobs/:id/trigger` |
| `KILLJOB` | `DELETE /api/v1/runs/:run_id` |
| `ON_HOLD` / `OFF_HOLD` | `POST /api/v1/jobs/:id/pause` / `/resume` |
| `CHANGE_STATUS=X` | `POST /api/v1/runs/:run_id/state {state: "X"}` |
| `SEND_SIGNAL=X` | `POST /api/v1/runs/:run_id/signal {signal: "X"}` |
| `SET_GLOBAL` | `POST /api/v1/globals {name, value}` |

Every successful write records an audit entry (`run.kill`, `run.signal`,
`run.change_status`, `global.set`, etc.). View via `GET /api/v1/audit` or
the **Audit** tab.
