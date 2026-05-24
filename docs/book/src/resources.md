# Virtual resources

A **virtual resource** is a named counter with a fixed capacity. Jobs
declare `ResourceClaim { resource_name, units }`. Before dispatch the
scheduler attempts to acquire every claim **atomically** inside a single
transaction. If any claim exceeds remaining capacity the job is left
queued (next_fire_at unchanged) and retried next tick.

## Why

Classic use cases:

- **DB connection pool** — `resources: "db.connections(1)"` caps concurrent
  jobs hitting the warehouse.
- **License keys** — `resources: "matlab(1)"` serializes runs that need
  the one available license.
- **GPU slots** — `resources: "gpu(1)"` ensures one training job per GPU.

## Create

```bash
curl -sX POST -H "Authorization: Bearer $RSCHED_TOKEN" \
     -H "content-type: application/json" \
     -d '{"name":"db.connections","capacity":5,"description":"Snowflake pool"}' \
     http://localhost:8080/api/v1/resources
```

Or use the **Resources** UI tab (admin / operator).

## Claim from a job

REST `Job.resource_claims`:

```json
{
  "resource_claims": [
    {"resource_name": "db.connections", "units": 1},
    {"resource_name": "gpu", "units": 2}
  ]
}
```

JIL:

```jil
resources: "db.connections(1), gpu(2)"
```

A bare name (no `(N)`) defaults to 1 unit.

## Lifecycle

1. Tick finds job due → `ResourceRepo::try_acquire(run_id, claims)`.
2. If any claim doesn't fit, return `Ok(false)`, rollback the transaction,
   skip dispatch. Run row is NOT inserted, `next_fire_at` unchanged.
3. If all fit, hold rows are inserted, run is dispatched.
4. On terminal run state (`Success` / `Failed` / `Killed` / `Lost`), the
   bin dispatch consumer calls `ResourceRepo::release(run_id)`.
5. Also released if the dispatch queue overflows after acquire, or via
   `sendevent CHANGE_STATUS=killed`.

## Available units

`GET /api/v1/resources` returns each resource with `available = capacity -
SUM(units in resource_holds)`. The UI shows a capacity bar.

## Atomicity

`try_acquire` runs inside `pool.begin()`. SQLite serializes transactions
on a per-connection basis (WAL); Postgres uses standard MVCC. In either
case, two concurrent ticks can't both succeed when only one slot is left.
