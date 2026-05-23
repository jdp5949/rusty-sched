-- v0.5 — Autosys-style virtual resources.

CREATE TABLE IF NOT EXISTS resources (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE,
    capacity     INTEGER NOT NULL CHECK (capacity >= 0),
    description  TEXT,
    created_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS resource_holds (
    run_id       TEXT NOT NULL,
    resource_id  TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    units        INTEGER NOT NULL CHECK (units > 0),
    acquired_at  TEXT NOT NULL,
    PRIMARY KEY (run_id, resource_id)
);
CREATE INDEX IF NOT EXISTS idx_resource_holds_res ON resource_holds(resource_id);
CREATE INDEX IF NOT EXISTS idx_resource_holds_run ON resource_holds(run_id);

ALTER TABLE jobs ADD COLUMN IF NOT EXISTS resource_claims_json TEXT;
