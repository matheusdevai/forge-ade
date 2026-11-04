-- FORGE migration 0001 — Foundation (Milestone 01)
-- Timestamps are unix epoch milliseconds (UTC).

CREATE TABLE app_settings (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,            -- JSON-encoded value
  updated_at INTEGER NOT NULL
);

CREATE TABLE workspaces (
  id             TEXT PRIMARY KEY,
  name           TEXT NOT NULL,
  path           TEXT NOT NULL UNIQUE, -- canonical absolute path
  pinned         INTEGER NOT NULL DEFAULT 0,
  created_at     INTEGER NOT NULL,
  last_opened_at INTEGER
);

CREATE TABLE pane_layouts (
  workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
  layout_json  TEXT NOT NULL,
  updated_at   INTEGER NOT NULL
);

CREATE TABLE missions (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT REFERENCES workspaces(id) ON DELETE SET NULL,
  title        TEXT NOT NULL,
  briefing     TEXT NOT NULL,
  mode         TEXT NOT NULL CHECK (mode IN ('free', 'squad', 'agentic')),
  status       TEXT NOT NULL DEFAULT 'draft'
               CHECK (status IN ('draft', 'planned', 'running', 'paused', 'completed', 'failed', 'cancelled')),
  created_at   INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL
);
CREATE INDEX idx_missions_workspace ON missions(workspace_id);
CREATE INDEX idx_missions_updated ON missions(updated_at DESC);

CREATE TABLE activity_events (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  kind         TEXT NOT NULL,
  message      TEXT NOT NULL,
  workspace_id TEXT REFERENCES workspaces(id) ON DELETE SET NULL,
  mission_id   TEXT REFERENCES missions(id) ON DELETE SET NULL,
  created_at   INTEGER NOT NULL
);
CREATE INDEX idx_activity_created ON activity_events(created_at DESC);
