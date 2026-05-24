# Calendars

A **Calendar** is a named set of rules that says when a job is allowed to
run. Every job can reference up to two calendars:

- `calendar_id` — **include** calendar. Job runs only when this allows `now`.
- `exclude_calendar_id` — **exclude** calendar. Job blocked when this allows `now`.

Tick loop loads all calendars once per tick and filters every due job
through `calendar_allows(&job, now, &cals)` before dispatch. Blocked jobs
advance `next_fire_at` (cron path) and skip — Autosys "skip then next
normal fire" semantics.

## Rule kinds

```rust
pub enum CalendarRule {
    Weekdays { days: Vec<u8> },        // 1=Mon .. 7=Sun
    Blackout { dates: Vec<NaiveDate> },
    TimeWindow { start: NaiveTime, end: NaiveTime },
}
```

Rules within a calendar are **AND**-combined. A weekday calendar with a
single blackout date allows every Monday–Sunday *except* that date.

## Common patterns

### "Business days only"

```json
{
  "name": "business_days",
  "rules": [
    {"kind": "weekdays", "days": [1,2,3,4,5]}
  ]
}
```

Attach as `calendar_id` (include).

### "US federal holidays"

```json
{
  "name": "us_holidays",
  "rules": [
    {"kind": "blackout", "dates": ["2026-07-04", "2026-12-25", "2026-01-01"]}
  ]
}
```

Attach as `exclude_calendar_id`. Holidays satisfy the no-rule "always
allow" → block the job.

### "Maintenance window 2–4 am UTC"

```json
{
  "name": "maintenance",
  "rules": [
    {"kind": "time_window", "start": "02:00:00", "end": "04:00:00"}
  ]
}
```

## CLI

Direct REST calls for now — UI page is a future v0.7.x slice.

```bash
curl -sX POST -H "Authorization: Bearer $RSCHED_TOKEN" \
     -H "content-type: application/json" \
     -d '{"name":"business_days","rules":[{"kind":"weekdays","days":[1,2,3,4,5]}]}' \
     http://localhost:8080/api/v1/calendars
```

## JIL

```jil
exclude_calendar: us_holidays
```

The JIL parser keeps the calendar **name** in `JobSpec.exclude_calendar`
and emits a warning; the apply step must resolve the name to a
`CalendarId` via `CalendarRepo::get_by_name()` before persisting.
