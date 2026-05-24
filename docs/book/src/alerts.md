# Alerts + SLA

Every job carries an `alerts: AlertConfig` shaped like:

```rust
struct AlertConfig {
    events: Vec<AlertEvent>,     // which events to alert on
    channels: Vec<AlertChannel>, // where to deliver
}
```

## Events

| Event | When it fires |
|---|---|
| `on_failure` | Run state == `Failed` |
| `on_success` | Run state == `Success` |
| `on_sla_miss` | Run exceeded `Job.sla_secs` while still running |
| `on_late_start` | Run not started by `scheduled_for + grace_secs` |
| `on_lost` | Agent disappeared mid-run |

## Channels

```rust
enum AlertChannel {
    Email   { to: Vec<String> },
    Slack   { webhook_url: String },
    Webhook { url: String, secret: Option<String> },
}
```

- **Slack**: standard incoming-webhook URL. Posts a formatted message
  with job name, state, exit code, duration.
- **Webhook**: generic POST. Body is JSON `AlertPayload`. If `secret` is
  set, an `X-Sig` header with HMAC-SHA256 of the body is added.
- **Email**: requires SMTP config via `RSCHED_SMTP_HOST` /
  `RSCHED_SMTP_USER` / `RSCHED_SMTP_PASS` env vars at bin startup.

All channels deliver concurrently via `rsched_alert::deliver_all` — one
slow webhook doesn't block another.

## SLA evaluation

Helper: `rsched_alert::evaluate_sla(now, scheduled_for, started_at,
sla_secs, late_start_grace_secs) -> SlaBreach`.

| Breach | Mapping |
|---|---|
| `None` | All good |
| `SlaMiss` | Started + exceeded `sla_secs` |
| `LateStart` | Not started + waited > `late_start_grace_secs` |

## `must_times` SLA

For Autosys-style wall-clock SLAs (`must_start_times` / `must_complete_times`),
the helper is `rsched_alert::evaluate_must_times`:

- `LateStart` when `started_at == None` AND every `must_start_times` entry
  has passed for today.
- `SlaMiss` when running past any `must_complete_time` that fell after
  the run started.

*Runtime wiring of these helpers into the tick loop + alert dispatch is
deferred to a future slice. The helpers themselves are stable and unit-tested.*

## Example

```json
{
  "alerts": {
    "events": ["on_failure", "on_sla_miss"],
    "channels": [
      {"kind":"slack","webhook_url":"https://hooks.slack.com/services/T.../B.../..."},
      {"kind":"email","to":["oncall@example.com"]}
    ]
  }
}
```

JIL:

```jil
alarm_if_fail: y
```

Translates to `{ events: [OnFailure], channels: [] }` — you still need to
add a Slack / email channel via REST.
