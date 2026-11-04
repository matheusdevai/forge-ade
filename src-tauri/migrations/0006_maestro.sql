-- FORGE migration 0006 — Maestro (Milestone 09)

ALTER TABLE missions ADD COLUMN max_attempts INTEGER NOT NULL DEFAULT 3;
ALTER TABLE missions ADD COLUMN max_runs INTEGER NOT NULL DEFAULT 40;
ALTER TABLE missions ADD COLUMN budget_usd REAL;
ALTER TABLE missions ADD COLUMN report TEXT;

ALTER TABLE mission_tasks ADD COLUMN base_commit TEXT;
ALTER TABLE mission_tasks ADD COLUMN review_run_id TEXT;
ALTER TABLE mission_tasks ADD COLUMN review_verdict TEXT;   -- approve | reject | NULL (pending/none)
ALTER TABLE mission_tasks ADD COLUMN review_summary TEXT;
ALTER TABLE mission_tasks ADD COLUMN provider_override TEXT;

-- mission_runs gains the 'review' kind (SQLite cannot alter a CHECK: rebuild).
CREATE TABLE mission_runs_new (
  mission_id TEXT NOT NULL REFERENCES missions(id) ON DELETE CASCADE,
  task_id    TEXT,
  run_id     TEXT NOT NULL,
  kind       TEXT NOT NULL CHECK (kind IN ('plan', 'task', 'review')),
  created_at INTEGER NOT NULL,
  PRIMARY KEY (mission_id, run_id)
);
INSERT INTO mission_runs_new SELECT mission_id, task_id, run_id, kind, created_at FROM mission_runs;
DROP TABLE mission_runs;
ALTER TABLE mission_runs_new RENAME TO mission_runs;
