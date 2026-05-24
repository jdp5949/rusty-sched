# Global variables

Globals are simple string key→value pairs persisted in the `globals` table.
They power the `value(name)` operand in [conditions](./conditions.md), and
provide an Autosys-compatible `sendevent SET_GLOBAL` workflow.

## Set / get / list / delete

UI: **Globals** tab.

CLI:

```bash
rusty-sched cli global set IS_RUN_DAY y
rusty-sched cli global set REGION us-east-1
rusty-sched cli global list
rusty-sched cli global delete IS_RUN_DAY
```

REST:

```bash
curl -sX POST -H "Authorization: Bearer $RSCHED_TOKEN" \
     -d '{"name":"IS_RUN_DAY","value":"y"}' \
     http://localhost:8080/api/v1/globals

curl http://localhost:8080/api/v1/globals          # list
curl http://localhost:8080/api/v1/globals/REGION   # get one
curl -X DELETE -H "Authorization: Bearer $RSCHED_TOKEN" \
     http://localhost:8080/api/v1/globals/REGION
```

## Truthy rule

`value(name)` in a condition expression returns:

- `Some(true)`  — when the raw value is one of `y`, `yes`, `true`, `1`
  (case-insensitive, trimmed).
- `Some(false)` — for any other value (including empty string).
- `None`        — when the name is unknown. The condition stays unresolved
  (no fire).

This matches Autosys's `value(VAR)="Y"` convention.

## Snapshot semantics

The scheduler loads every global into a per-tick snapshot when building
`StoreUpstreamState`. Changes are visible on the **next** tick. Writes
during a tick don't affect that tick's evaluation.

## Use cases

- **Holiday switch.** Cron-fire a job that sets `IS_RUN_DAY=n` on holidays;
  guard downstreams with `value(IS_RUN_DAY)`.
- **Region awareness.** `value(REGION) = "us-east-1"` — wait, conditions
  don't support string comparison yet; use a boolean-style global instead
  (`IS_PROD=y`).
- **Feature flag.** `value(FEAT_NEW_PIPELINE) and success(prep)` — flip
  the global to roll a release forward / backward.
