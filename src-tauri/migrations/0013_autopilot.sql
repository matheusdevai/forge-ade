-- FORGE migration 0013 — Autopilot (Milestone 18)
-- Scheduled routines. They run only while FORGE is open; missed runs are
-- not replayed. Every execution is recorded in routine_runs.

CREATE TABLE routines (
  id               TEXT PRIMARY KEY,
  name             TEXT NOT NULL,
  enabled          INTEGER NOT NULL DEFAULT 1,
  workspace_id     TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  -- agent | script | command | mission | verify
  action           TEXT NOT NULL CHECK (action IN ('agent', 'script', 'command', 'mission', 'verify')),
  config           TEXT NOT NULL DEFAULT '{}',   -- JSON, per action
  -- manual | interval | daily | commit
  trigger          TEXT NOT NULL CHECK (trigger IN ('manual', 'interval', 'daily', 'commit')),
  interval_min     INTEGER,                      -- interval
  daily_time       TEXT,                         -- daily: "HH:MM" (local time)
  days_mask        INTEGER NOT NULL DEFAULT 127, -- daily: bit0 = Monday … bit6 = Sunday
  max_runs_per_day INTEGER NOT NULL DEFAULT 24,
  timeout_min      INTEGER NOT NULL DEFAULT 30,
  max_failures     INTEGER NOT NULL DEFAULT 3,   -- consecutive failures before auto-disable
  budget_usd_day   REAL,                         -- agent: reported cost cap per day
  -- command routines: sha256 of the exact command the user approved
  approved_hash    TEXT,
  consecutive_failures INTEGER NOT NULL DEFAULT 0,
  disabled_reason  TEXT,
  next_run_at      INTEGER,
  last_head        TEXT,                         -- commit trigger
  created_at       INTEGER NOT NULL,
  updated_at       INTEGER NOT NULL
);
CREATE INDEX idx_routines_next ON routines(enabled, next_run_at);

CREATE TABLE routine_runs (
  id           TEXT PRIMARY KEY,
  routine_id   TEXT NOT NULL REFERENCES routines(id) ON DELETE CASCADE,
  trigger      TEXT NOT NULL,                    -- schedule | manual | commit
  status       TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'failed', 'skipped', 'cancelled')),
  output       TEXT,                             -- redacted, size-capped
  error        TEXT,
  agent_run_id TEXT,
  mission_id   TEXT,
  cost_usd     REAL,                             -- only when reported by the CLI
  started_at   INTEGER NOT NULL,
  ended_at     INTEGER
);
CREATE INDEX idx_routine_runs ON routine_runs(routine_id, started_at DESC);
