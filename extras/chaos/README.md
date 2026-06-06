# Chaos test suite

Smoke-test rusty-sched against transient network faults and backend restarts.
Built on top of [Toxiproxy](https://github.com/Shopify/toxiproxy) sitting
between rusty-sched and Postgres 16, all wired up via Docker Compose.

## Run locally

```sh
# From repo root.
docker compose -f docker-compose.chaos.yml up --abort-on-container-exit chaos-runner
```

The `chaos-runner` container streams a summary at the end:

```
=== Summary ===
latency-500ms                    PASS  ready under 500ms postgres latency
loss-10pct                       PASS  ready under 10% stream timeouts
partition-30s                    PASS  recovered after 30s partition
postgres-restart                 PASS  recovered after simulated restart

4 pass, 0 fail
```

Exit code is non-zero if any scenario fails.

## Scenarios

| Name              | Fault                                       | Pass criterion                                |
| ----------------- | ------------------------------------------- | --------------------------------------------- |
| latency-500ms     | 500ms latency on postgres traffic           | `/readyz` returns 200 within 30s              |
| loss-10pct        | Stream timeouts at 10% toxicity (1s)        | `/readyz` returns 200 within 30s              |
| partition-30s     | Toxiproxy `enabled=false` for 30s           | `/readyz` returns 200 within 60s of recovery  |
| postgres-restart  | Proxy disabled long enough to drop conns    | `/readyz` returns 200 within 60s of recovery  |

## Tear down

```sh
docker compose -f docker-compose.chaos.yml down -v
```

## Adding a scenario

1. Edit `chaos/scenarios.sh`.
2. Use `add_toxic '<json>'` or `set_proxy_enabled false/true` to inject the fault.
3. Call `wait_ready <seconds>` then `record <name> PASS|FAIL <detail>`.
4. Always finish with `clear_toxics`.

Toxiproxy's [HTTP API](https://github.com/Shopify/toxiproxy#http-api) is
reachable at `http://toxiproxy:8474` from inside the compose network and
`http://localhost:8474` from the host.

## Caveats

- The "postgres restart" scenario approximates a restart by partitioning until
  idle connections drop. A true `docker restart` would need to mount the
  Docker socket into `chaos-runner`, which we intentionally avoid for safety.
- If you don't have a local `Dockerfile`, Compose falls back to
  `ghcr.io/jdp5949/rusty-sched:main`. Override with `--build` to force a
  rebuild from your working tree.
- Resource contention (CPU starvation, OOM) is not yet covered — open an
  issue if you need those.
