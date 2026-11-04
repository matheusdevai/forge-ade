-- FORGE migration 0007 — Squads & Recipes (Milestone 10)

CREATE TABLE squads (
  id           TEXT PRIMARY KEY,
  name         TEXT NOT NULL,
  description  TEXT NOT NULL DEFAULT '',
  -- JSON [{ "profileId": "...", "function": "...", "stage": 1 }]
  members      TEXT NOT NULL,
  rules        TEXT NOT NULL DEFAULT '',
  flow         TEXT NOT NULL DEFAULT '',
  default_mode TEXT NOT NULL DEFAULT 'squad' CHECK (default_mode IN ('free', 'squad', 'agentic')),
  builtin      INTEGER NOT NULL DEFAULT 0,
  origin       TEXT NOT NULL DEFAULT 'local' CHECK (origin IN ('builtin', 'local', 'import')),
  created_at   INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL
);

CREATE TABLE recipes (
  id                TEXT PRIMARY KEY,
  name              TEXT NOT NULL,
  objective         TEXT NOT NULL DEFAULT '',
  squad_id          TEXT REFERENCES squads(id) ON DELETE SET NULL,
  mode              TEXT NOT NULL DEFAULT 'squad' CHECK (mode IN ('free', 'squad', 'agentic')),
  max_parallel      INTEGER NOT NULL DEFAULT 2,
  max_attempts      INTEGER NOT NULL DEFAULT 3,
  max_runs          INTEGER NOT NULL DEFAULT 40,
  budget_usd        REAL,
  require_review    INTEGER NOT NULL DEFAULT 1,
  briefing_template TEXT NOT NULL,
  -- JSON [{ "key": "...", "label": "...", "placeholder": "...", "required": true }]
  fields            TEXT NOT NULL DEFAULT '[]',
  -- JSON TaskInput[] (optional preset plan; placeholders allowed in text)
  plan              TEXT,
  builtin           INTEGER NOT NULL DEFAULT 0,
  origin            TEXT NOT NULL DEFAULT 'local' CHECK (origin IN ('builtin', 'local', 'import')),
  created_at        INTEGER NOT NULL,
  updated_at        INTEGER NOT NULL
);

ALTER TABLE missions ADD COLUMN squad_id TEXT REFERENCES squads(id) ON DELETE SET NULL;
ALTER TABLE missions ADD COLUMN recipe_id TEXT REFERENCES recipes(id) ON DELETE SET NULL;
ALTER TABLE agent_profiles ADD COLUMN origin TEXT NOT NULL DEFAULT 'local';
UPDATE agent_profiles SET origin = 'builtin' WHERE builtin = 1;
