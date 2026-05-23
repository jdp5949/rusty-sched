-- v0.3.0 — Autosys parity: extra columns on jobs + boxes.

ALTER TABLE jobs ADD COLUMN IF NOT EXISTS exclude_calendar_id     TEXT REFERENCES calendars(id) ON DELETE SET NULL;
ALTER TABLE jobs ADD COLUMN IF NOT EXISTS must_start_times_json   TEXT;
ALTER TABLE jobs ADD COLUMN IF NOT EXISTS must_complete_times_json TEXT;
ALTER TABLE jobs ADD COLUMN IF NOT EXISTS exit_policy_json        TEXT;

ALTER TABLE boxes ADD COLUMN IF NOT EXISTS box_success_expr  TEXT;
ALTER TABLE boxes ADD COLUMN IF NOT EXISTS box_failure_expr  TEXT;
ALTER TABLE boxes ADD COLUMN IF NOT EXISTS box_terminator    SMALLINT NOT NULL DEFAULT 0;
ALTER TABLE boxes ADD COLUMN IF NOT EXISTS job_terminator    SMALLINT NOT NULL DEFAULT 0;
ALTER TABLE boxes ADD COLUMN IF NOT EXISTS auto_hold         SMALLINT NOT NULL DEFAULT 0;
