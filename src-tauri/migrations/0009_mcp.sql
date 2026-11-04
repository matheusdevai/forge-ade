-- FORGE migration 0009 — MCP Engine (Milestone 12)
-- Secret values are NEVER stored here: only the names of the keys kept in the OS keychain.

CREATE TABLE mcp_servers (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL UNIQUE,         -- slug used as the server name for agents
  label         TEXT NOT NULL,
  transport     TEXT NOT NULL CHECK (transport IN ('stdio', 'http')),
  command       TEXT,
  args          TEXT NOT NULL DEFAULT '[]',   -- JSON string[]; may contain ${KEY} placeholders
  env           TEXT NOT NULL DEFAULT '{}',   -- JSON {KEY: value} (non-secret)
  url           TEXT,
  headers       TEXT NOT NULL DEFAULT '{}',   -- JSON {Header: value} (non-secret; may contain ${KEY})
  secret_keys   TEXT NOT NULL DEFAULT '[]',   -- JSON string[] of keys stored in the keychain
  read_only     INTEGER NOT NULL DEFAULT 0,   -- tools may be used by read-only agents
  enabled       INTEGER NOT NULL DEFAULT 1,
  template_id   TEXT,
  tools         TEXT NOT NULL DEFAULT '[]',   -- cached tools/list result
  server_info   TEXT,                         -- cached initialize result (JSON)
  last_check_at INTEGER,
  last_ok       INTEGER,
  last_error    TEXT,
  last_log      TEXT,
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL
);

CREATE TABLE mcp_links (
  server_id   TEXT NOT NULL REFERENCES mcp_servers(id) ON DELETE CASCADE,
  target_kind TEXT NOT NULL CHECK (target_kind IN ('profile', 'squad', 'mission')),
  target_id   TEXT NOT NULL,
  created_at  INTEGER NOT NULL,
  PRIMARY KEY (server_id, target_kind, target_id)
);
CREATE INDEX idx_mcp_links_target ON mcp_links(target_kind, target_id);

ALTER TABLE agent_runs ADD COLUMN mcp TEXT NOT NULL DEFAULT '[]';
