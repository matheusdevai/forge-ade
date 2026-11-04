-- FORGE migration 0005 — Mission Engine (Milestone 08)

ALTER TABLE missions ADD COLUMN plan_run_id TEXT;
ALTER TABLE missions ADD COLUMN planning INTEGER NOT NULL DEFAULT 0;
ALTER TABLE missions ADD COLUMN max_parallel INTEGER NOT NULL DEFAULT 2;
ALTER TABLE missions ADD COLUMN require_review INTEGER NOT NULL DEFAULT 1;
ALTER TABLE missions ADD COLUMN base_branch TEXT;
ALTER TABLE missions ADD COLUMN base_commit TEXT;
ALTER TABLE missions ADD COLUMN approved_at INTEGER;
ALTER TABLE missions ADD COLUMN integrated_at INTEGER;

CREATE TABLE mission_tasks (
  id              TEXT PRIMARY KEY,
  mission_id      TEXT NOT NULL REFERENCES missions(id) ON DELETE CASCADE,
  position        INTEGER NOT NULL,
  key             TEXT NOT NULL,
  title           TEXT NOT NULL,
  description     TEXT NOT NULL,
  acceptance      TEXT NOT NULL DEFAULT '',
  profile_id      TEXT REFERENCES agent_profiles(id) ON DELETE SET NULL,
  depends_on      TEXT NOT NULL DEFAULT '[]',   -- JSON array of task ids
  status          TEXT NOT NULL DEFAULT 'pending'
                  CHECK (status IN ('pending', 'running', 'review', 'approved', 'rejected', 'failed', 'cancelled', 'skipped')),
  run_id          TEXT,
  worktree_path   TEXT,
  worktree_branch TEXT,
  attempts        INTEGER NOT NULL DEFAULT 0,
  feedback        TEXT,
  integrated_at   INTEGER,
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL,
  UNIQUE (mission_id, key)
);
CREATE INDEX idx_tasks_mission ON mission_tasks(mission_id, position);

CREATE TABLE mission_events (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  mission_id TEXT NOT NULL REFERENCES missions(id) ON DELETE CASCADE,
  task_id    TEXT,
  kind       TEXT NOT NULL,
  message    TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX idx_mission_events ON mission_events(mission_id, id);

-- Every agent run made on behalf of a mission (plan + task attempts), for totals/history.
CREATE TABLE mission_runs (
  mission_id TEXT NOT NULL REFERENCES missions(id) ON DELETE CASCADE,
  task_id    TEXT,
  run_id     TEXT NOT NULL,
  kind       TEXT NOT NULL CHECK (kind IN ('plan', 'task')),
  created_at INTEGER NOT NULL,
  PRIMARY KEY (mission_id, run_id)
);
