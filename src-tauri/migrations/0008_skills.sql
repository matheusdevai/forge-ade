-- FORGE migration 0008 — Skills Engine (Milestone 11)

CREATE TABLE skills (
  id          TEXT PRIMARY KEY,
  slug        TEXT NOT NULL UNIQUE,
  name        TEXT NOT NULL,
  description TEXT NOT NULL,
  version     INTEGER NOT NULL DEFAULT 1,
  body        TEXT NOT NULL,           -- SKILL.md body (instructions)
  enabled     INTEGER NOT NULL DEFAULT 1,
  builtin     INTEGER NOT NULL DEFAULT 0,
  origin      TEXT NOT NULL DEFAULT 'local' CHECK (origin IN ('builtin', 'local', 'import')),
  source      TEXT,                    -- where it was installed from (display only)
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
);

CREATE TABLE skill_versions (
  skill_id    TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
  version     INTEGER NOT NULL,
  name        TEXT NOT NULL,
  description TEXT NOT NULL,
  body        TEXT NOT NULL,
  note        TEXT NOT NULL DEFAULT '',
  created_at  INTEGER NOT NULL,
  PRIMARY KEY (skill_id, version)
);

-- Supporting files shipped with a skill. Stored for reference only: FORGE
-- never executes them and never injects them into prompts.
CREATE TABLE skill_files (
  skill_id  TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
  path      TEXT NOT NULL,
  size      INTEGER NOT NULL,
  is_script INTEGER NOT NULL DEFAULT 0,
  content   TEXT,                      -- text files only (NULL for binaries)
  PRIMARY KEY (skill_id, path)
);

CREATE TABLE skill_links (
  skill_id    TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
  target_kind TEXT NOT NULL CHECK (target_kind IN ('profile', 'squad', 'mission')),
  target_id   TEXT NOT NULL,
  created_at  INTEGER NOT NULL,
  PRIMARY KEY (skill_id, target_kind, target_id)
);
CREATE INDEX idx_skill_links_target ON skill_links(target_kind, target_id);

-- Which skills (slug@version) were injected into each agent run.
ALTER TABLE agent_runs ADD COLUMN skills TEXT NOT NULL DEFAULT '[]';
