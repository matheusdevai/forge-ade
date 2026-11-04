-- FORGE migration 0003 — Provider verification results (Milestone 06)
-- One row per provider: the latest real headless check. Usage/cost are the
-- values reported by the CLI itself (never estimated); NULL when not reported.

CREATE TABLE provider_checks (
  provider_id   TEXT PRIMARY KEY,
  ok            INTEGER NOT NULL,
  version       TEXT,
  model         TEXT,
  message       TEXT NOT NULL,
  duration_ms   INTEGER NOT NULL,
  input_tokens  INTEGER,
  output_tokens INTEGER,
  cost_usd      REAL,
  checked_at    INTEGER NOT NULL
);
