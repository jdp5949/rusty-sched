-- v0.7.4 — extend run_logs.stream CHECK to include 'plugin'.
ALTER TABLE run_logs DROP CONSTRAINT IF EXISTS run_logs_stream_check;
ALTER TABLE run_logs ADD CONSTRAINT run_logs_stream_check
    CHECK (stream IN ('stdout','stderr','plugin'));
