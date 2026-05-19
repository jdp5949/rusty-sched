-- Initial schema for rusty-sched (single-node SQLite, also used as Raft state machine).
-- All TEXT IDs are ULIDs.

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS users (
    id              TEXT PRIMARY KEY,
    username        TEXT NOT NULL UNIQUE,
    password_hash   TEXT NOT NULL,
    role            TEXT NOT NULL CHECK (role IN ('admin','operator','viewer')),
    disabled        INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    token       TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at  TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    ip          TEXT
);
CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);

CREATE TABLE IF NOT EXISTS audit_log (
    id              TEXT PRIMARY KEY,
    user_id         TEXT,
    action          TEXT NOT NULL,
    target_type     TEXT NOT NULL,
    target_id       TEXT,
    payload_json    TEXT,
    ts              TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_audit_ts ON audit_log(ts DESC);

CREATE TABLE IF NOT EXISTS calendars (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL UNIQUE,
    definition_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS boxes (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL UNIQUE,
    parent_box_id   TEXT REFERENCES boxes(id) ON DELETE SET NULL,
    paused          INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS jobs (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL UNIQUE,
    box_id              TEXT REFERENCES boxes(id) ON DELETE SET NULL,
    trigger_kind        TEXT NOT NULL,
    trigger_data_json   TEXT NOT NULL,
    cmd                 TEXT NOT NULL,
    args_json           TEXT NOT NULL DEFAULT '[]',
    env_json            TEXT NOT NULL DEFAULT '{}',
    cwd                 TEXT,
    shell               TEXT NOT NULL DEFAULT 'auto',
    target_json         TEXT NOT NULL,
    retry_json          TEXT NOT NULL,
    timeout_secs        INTEGER NOT NULL DEFAULT 0,
    sla_secs            INTEGER NOT NULL DEFAULT 0,
    calendar_id         TEXT REFERENCES calendars(id) ON DELETE SET NULL,
    misfire_policy      TEXT NOT NULL DEFAULT 'fire_once',
    paused              INTEGER NOT NULL DEFAULT 0,
    alert_config_json   TEXT NOT NULL DEFAULT '{"events":[],"channels":[]}',
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    next_fire_at        TEXT
);
CREATE INDEX IF NOT EXISTS idx_jobs_due ON jobs(next_fire_at) WHERE paused = 0 AND next_fire_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_jobs_box ON jobs(box_id);

CREATE TABLE IF NOT EXISTS dependencies (
    job_id              TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    depends_on_job_id   TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    condition           TEXT NOT NULL,
    PRIMARY KEY (job_id, depends_on_job_id)
);
CREATE INDEX IF NOT EXISTS idx_deps_up ON dependencies(depends_on_job_id);

CREATE TABLE IF NOT EXISTS agents (
    id                  TEXT PRIMARY KEY,
    hostname            TEXT NOT NULL,
    cert_fingerprint    TEXT NOT NULL UNIQUE,
    tags_json           TEXT NOT NULL DEFAULT '[]',
    last_seen           TEXT,
    state               TEXT NOT NULL DEFAULT 'unknown',
    version             TEXT,
    os                  TEXT,
    arch                TEXT
);

CREATE TABLE IF NOT EXISTS runs (
    id                  TEXT PRIMARY KEY,
    job_id              TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    agent_id            TEXT REFERENCES agents(id) ON DELETE SET NULL,
    state               TEXT NOT NULL,
    attempt             INTEGER NOT NULL,
    queued_at           TEXT NOT NULL,
    started_at          TEXT,
    finished_at         TEXT,
    exit_code           INTEGER,
    parent_run_ids_json TEXT NOT NULL DEFAULT '[]',
    log_truncated       INTEGER NOT NULL DEFAULT 0,
    log_bytes           INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_runs_job_started ON runs(job_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_runs_active ON runs(state) WHERE state IN ('queued','running');

CREATE TABLE IF NOT EXISTS run_logs (
    run_id      TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    seq         INTEGER NOT NULL,
    ts          TEXT NOT NULL,
    stream      TEXT NOT NULL CHECK (stream IN ('stdout','stderr')),
    chunk       BLOB NOT NULL,
    PRIMARY KEY (run_id, seq)
);
