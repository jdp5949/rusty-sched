-- v0.6 — Autosys global variables.
CREATE TABLE IF NOT EXISTS globals (
    name        TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
