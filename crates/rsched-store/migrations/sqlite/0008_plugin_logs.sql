-- v0.7.4 — extend run_logs.stream CHECK to include 'plugin' for Cronicle-style plugin events.

CREATE TABLE run_logs_new (
    run_id      TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    seq         INTEGER NOT NULL,
    ts          TEXT NOT NULL,
    stream      TEXT NOT NULL CHECK (stream IN ('stdout','stderr','plugin')),
    chunk       BLOB NOT NULL,
    PRIMARY KEY (run_id, seq)
);

INSERT INTO run_logs_new (run_id, seq, ts, stream, chunk)
    SELECT run_id, seq, ts, stream, chunk FROM run_logs;

DROP TABLE run_logs;
ALTER TABLE run_logs_new RENAME TO run_logs;
