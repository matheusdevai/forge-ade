-- FORGE migration 0002 — Multiple sessions per workspace + saved layouts (Milestone 02)

CREATE TABLE workspace_sessions (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  name         TEXT NOT NULL,
  position     INTEGER NOT NULL,
  layout_json  TEXT NOT NULL,
  created_at   INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL
);
CREATE INDEX idx_sessions_workspace ON workspace_sessions(workspace_id, position);

-- Existing per-workspace layouts become the workspace's first session.
INSERT INTO workspace_sessions (id, workspace_id, name, position, layout_json, created_at, updated_at)
  SELECT lower(hex(randomblob(16))), workspace_id, 'Principal', 0, layout_json, updated_at, updated_at
  FROM pane_layouts;

DROP TABLE pane_layouts;

ALTER TABLE workspaces ADD COLUMN active_session_id TEXT;
UPDATE workspaces SET active_session_id =
  (SELECT s.id FROM workspace_sessions s WHERE s.workspace_id = workspaces.id ORDER BY s.position LIMIT 1);

CREATE TABLE layout_presets (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL UNIQUE COLLATE NOCASE,
  layout_json TEXT NOT NULL,
  created_at  INTEGER NOT NULL
);
