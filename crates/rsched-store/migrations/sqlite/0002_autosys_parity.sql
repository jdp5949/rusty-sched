-- v0.3.0 — Autosys parity: extra columns on jobs + boxes.
-- All columns are nullable / have safe defaults so existing rows still load.

ALTER TABLE jobs ADD COLUMN exclude_calendar_id   TEXT REFERENCES calendars(id) ON DELETE SET NULL;
ALTER TABLE jobs ADD COLUMN must_start_times_json TEXT;
ALTER TABLE jobs ADD COLUMN must_complete_times_json TEXT;
ALTER TABLE jobs ADD COLUMN exit_policy_json      TEXT;

ALTER TABLE boxes ADD COLUMN box_success_expr   TEXT;
ALTER TABLE boxes ADD COLUMN box_failure_expr   TEXT;
ALTER TABLE boxes ADD COLUMN box_terminator     INTEGER NOT NULL DEFAULT 0;
ALTER TABLE boxes ADD COLUMN job_terminator     INTEGER NOT NULL DEFAULT 0;
ALTER TABLE boxes ADD COLUMN auto_hold          INTEGER NOT NULL DEFAULT 0;
