-- FORGE migration 0010 — OverMemory (Milestone 14)
-- Shared memory with explicit scopes. Workspace/mission items never leave their
-- workspace; agent items are bound to a workspace unless the user explicitly
-- makes them cross-project (workspace_id NULL); global items are user-made only.

CREATE TABLE memories (
  id            TEXT PRIMARY KEY,
  scope         TEXT NOT NULL CHECK (scope IN ('global', 'workspace', 'mission', 'agent')),
  workspace_id  TEXT REFERENCES workspaces(id) ON DELETE CASCADE,
  mission_id    TEXT REFERENCES missions(id) ON DELETE CASCADE,
  profile_id    TEXT REFERENCES agent_profiles(id) ON DELETE CASCADE,
  kind          TEXT NOT NULL CHECK (kind IN ('decision', 'convention', 'context', 'history', 'error', 'preference', 'doc')),
  title         TEXT NOT NULL,
  body          TEXT NOT NULL,
  tags          TEXT NOT NULL DEFAULT '[]',          -- JSON string[]
  pinned        INTEGER NOT NULL DEFAULT 0,
  -- suggested: proposed by an agent, inactive until the user approves it
  status        TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suggested', 'archived')),
  source        TEXT NOT NULL DEFAULT 'manual' CHECK (source IN ('manual', 'agent')),
  source_run_id TEXT REFERENCES agent_runs(id) ON DELETE SET NULL,
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL,
  CHECK (
    (scope = 'global'    AND workspace_id IS NULL AND mission_id IS NULL AND profile_id IS NULL) OR
    (scope = 'workspace' AND workspace_id IS NOT NULL AND mission_id IS NULL AND profile_id IS NULL) OR
    (scope = 'mission'   AND mission_id IS NOT NULL AND profile_id IS NULL) OR
    (scope = 'agent'     AND profile_id IS NOT NULL AND mission_id IS NULL)
  )
);
CREATE INDEX idx_memories_workspace ON memories(workspace_id, status);
CREATE INDEX idx_memories_mission ON memories(mission_id);
CREATE INDEX idx_memories_profile ON memories(profile_id);
CREATE INDEX idx_memories_updated ON memories(updated_at DESC);

-- Titles of the memories injected into each run (traceability).
ALTER TABLE agent_runs ADD COLUMN memory TEXT NOT NULL DEFAULT '[]';
