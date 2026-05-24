# REST API

Base path: `/api/v1/`. All responses are JSON. Authentication: cookie
(`rsched_session`) from `/auth/login`, OR `Authorization: Bearer
<api_key>` header.

## Auth

| Method | Path | Body | Returns |
|---|---|---|---|
| POST | `/auth/login` | `{username, password}` | sets `rsched_session` cookie + `{user_id, username, role}` |
| POST | `/auth/logout` | — | 204, clears cookie |
| GET | `/auth/me` | — | `{user_id, username, role}` (401 if no auth) |
| GET | `/auth/api-keys` | — | list of own keys (metadata only) |
| POST | `/auth/api-keys` | `{name, expires_at?}` | **plaintext token shown once** |
| DELETE | `/auth/api-keys/:id` | — | 204 (must own the key) |

## Jobs

| Method | Path | Auth | Body | Notes |
|---|---|---|---|---|
| GET | `/jobs` | any | — | list all |
| POST | `/jobs` | write | `CreateJobReq` | audit `job.create` |
| GET | `/jobs/:id` | any | — | |
| GET | `/jobs/by-name/:name` | any | — | |
| PUT | `/jobs/:id` | write | `CreateJobReq` | audit `job.update` |
| DELETE | `/jobs/:id` | write | — | audit `job.delete` |
| POST | `/jobs/:id/pause` | write | — | audit `job.pause` |
| POST | `/jobs/:id/resume` | write | — | audit `job.resume` |
| POST | `/jobs/:id/trigger` | write | — | sets `next_fire_at=now`, audit `job.trigger` |
| GET | `/jobs/:id/runs` | any | — | last 100 runs |

## Runs

| Method | Path | Auth | Body | Notes |
|---|---|---|---|---|
| GET | `/runs/:id` | any | — | |
| DELETE | `/runs/:id` | write | — | `SIGKILL` via handle registry, audit `run.kill` |
| POST | `/runs/:id/state` | write | `{state, exit_code?}` | CHANGE_STATUS, audit `run.change_status` |
| POST | `/runs/:id/signal` | write | `{signal}` | SEND_SIGNAL, audit `run.signal` |
| GET | `/runs/:id/logs` | any | `?from_seq&limit` | paginated rows |
| GET | `/runs/:id/logs/ws` | any | — | **WebSocket** — streams new log rows |

## Stats

| Method | Path | Returns |
|---|---|---|
| GET | `/stats/jobs/:id` | success-rate + p50/p99 + sparkline outcomes |

## Resources

| Method | Path | Auth | Notes |
|---|---|---|---|
| GET | `/resources` | any | list with `available` |
| POST | `/resources` | write | `{name, capacity, description?}` |
| GET | `/resources/:name` | any | one resource w/ available |
| DELETE | `/resources/:name` | write | cascades holds |

## Globals

| Method | Path | Auth | Notes |
|---|---|---|---|
| GET | `/globals` | any | list `{name, value, updated_at}` |
| POST | `/globals` | write | `{name, value}` — upsert |
| GET | `/globals/:name` | any | one global |
| DELETE | `/globals/:name` | write | |

## Users + audit (admin only)

| Method | Path | Body |
|---|---|---|
| GET | `/users` | — |
| POST | `/users` | `{username, password, role}` |
| GET | `/audit` | — (last 200 entries) |

## Health

| Method | Path |
|---|---|
| GET | `/healthz` |
| GET | `/readyz` |

## Error envelope

```json
{"error": "validation: bad job id"}
```

Status codes: 400 (validation), 401 (no auth), 403 (forbidden), 404 (not
found), 500 (store / scheduler).
