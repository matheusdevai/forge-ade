-- FORGE migration 0011 — Harness (Milestone 16)
-- Per-mission opt-in: the Harness picks the Claude model tier per task
-- (only when the task's agent profile has no fixed model).
ALTER TABLE missions ADD COLUMN harness INTEGER NOT NULL DEFAULT 0;
