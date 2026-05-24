-- v0.4.2 — CSRF double-submit token column on sessions.
-- Existing rows are backfilled with NULL (treated as "no CSRF token required"
-- until the user logs in again, at which point a fresh session row with a
-- non-NULL csrf_token is created).
ALTER TABLE sessions ADD COLUMN csrf_token TEXT;
