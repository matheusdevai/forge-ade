-- FORGE migration 0012 — Media Engine (Milestone 17)
-- Files live in <app data>/media; only metadata is stored here. API keys are
-- in the OS keychain, never in this database.

CREATE TABLE media_assets (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT REFERENCES workspaces(id) ON DELETE SET NULL,
  -- generate-image | analyze-image | tts | transcribe | video | screenshot
  op           TEXT NOT NULL,
  -- image | audio | video | text
  kind         TEXT NOT NULL,
  provider     TEXT NOT NULL,               -- gemini | claude | macos | browser
  model        TEXT,
  prompt       TEXT NOT NULL DEFAULT '',
  path         TEXT,                        -- produced file (absolute, inside app data)
  source_path  TEXT,                        -- input file (analysis / transcription)
  mime         TEXT,
  bytes        INTEGER,
  result_text  TEXT,                        -- analysis / transcription output
  status       TEXT NOT NULL DEFAULT 'done' CHECK (status IN ('running', 'done', 'failed')),
  error        TEXT,
  job          TEXT,                        -- provider operation id (long-running video)
  created_at   INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL
);
CREATE INDEX idx_media_created ON media_assets(created_at DESC);
CREATE INDEX idx_media_workspace ON media_assets(workspace_id);

-- Model chosen per operation, provider verification (no secrets).
CREATE TABLE media_providers (
  id             TEXT PRIMARY KEY,           -- gemini
  models         TEXT NOT NULL DEFAULT '[]', -- cached list from the provider's API
  choices        TEXT NOT NULL DEFAULT '{}', -- {op: model}
  last_check_at  INTEGER,
  last_ok        INTEGER,
  last_error     TEXT
);
