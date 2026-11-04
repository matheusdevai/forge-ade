-- FORGE migration 0004 — Agent Engine (Milestone 07)

CREATE TABLE agent_profiles (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  role          TEXT NOT NULL,
  description   TEXT NOT NULL DEFAULT '',
  provider_id   TEXT NOT NULL,
  model         TEXT,
  instructions  TEXT NOT NULL DEFAULT '',
  -- read: never modifies files · edit: may edit files · full: unrestricted (explicit confirmation per run)
  permission    TEXT NOT NULL DEFAULT 'read' CHECK (permission IN ('read', 'edit', 'full')),
  max_turns     INTEGER,
  builtin       INTEGER NOT NULL DEFAULT 0,
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL
);

CREATE TABLE agent_runs (
  id              TEXT PRIMARY KEY,
  workspace_id    TEXT REFERENCES workspaces(id) ON DELETE SET NULL,
  profile_id      TEXT REFERENCES agent_profiles(id) ON DELETE SET NULL,
  parent_run_id   TEXT REFERENCES agent_runs(id) ON DELETE SET NULL,
  provider_id     TEXT NOT NULL,
  model           TEXT,
  permission      TEXT NOT NULL,
  prompt          TEXT NOT NULL,
  status          TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed', 'cancelled')),
  session_id      TEXT,
  cwd             TEXT NOT NULL,
  worktree_branch TEXT,
  base_commit     TEXT,
  result_text     TEXT,
  error           TEXT,
  input_tokens    INTEGER,
  output_tokens   INTEGER,
  cache_read_tokens INTEGER,
  cost_usd        REAL,
  duration_ms     INTEGER,
  started_at      INTEGER NOT NULL,
  ended_at        INTEGER
);
CREATE INDEX idx_runs_workspace ON agent_runs(workspace_id, started_at DESC);
CREATE INDEX idx_runs_status ON agent_runs(status);

CREATE TABLE agent_events (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id     TEXT NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
  seq        INTEGER NOT NULL,
  kind       TEXT NOT NULL,
  payload    TEXT NOT NULL,   -- JSON, secrets redacted, size-capped
  created_at INTEGER NOT NULL
);
CREATE INDEX idx_events_run ON agent_events(run_id, seq);
