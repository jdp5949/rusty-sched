-- v0.7.1 — per-run rusage metrics: peak resident set size + total CPU seconds.
ALTER TABLE runs ADD COLUMN peak_rss_bytes INTEGER;
ALTER TABLE runs ADD COLUMN cpu_user_secs  REAL;
ALTER TABLE runs ADD COLUMN cpu_sys_secs   REAL;
