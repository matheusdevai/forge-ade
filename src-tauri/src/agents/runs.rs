//! Persistence of agent runs and their (redacted) event logs.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use serde_json::Value;

use crate::db::now_ms;
use crate::error::{AppError, AppResult};

pub const MAX_EVENTS_PER_RUN: i64 = 5000;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    pub workspace_id: Option<String>,
    pub profile_id: Option<String>,
    pub parent_run_id: Option<String>,
    pub provider_id: String,
    pub model: Option<String>,
    pub permission: String,
    pub prompt: String,
    pub status: String,
    pub session_id: Option<String>,
    pub cwd: String,
    pub worktree_branch: Option<String>,
    pub base_commit: Option<String>,
    pub result_text: Option<String>,
    pub error: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<i64>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    /// Skills injected into this run (`slug@vN`).
    pub skills: Vec<String>,
    /// MCP servers attached to this run.
    pub mcp: Vec<String>,
    /// Memories injected into this run (`kind: title`).
    pub memory: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredEvent {
    pub seq: i64,
    pub event: Value,
    pub created_at: i64,
}

const COLS: &str = "id, workspace_id, profile_id, parent_run_id, provider_id, model, permission, prompt, status, session_id, cwd,
  worktree_branch, base_commit, result_text, error, input_tokens, output_tokens, cache_read_tokens, cost_usd, duration_ms, started_at, ended_at, skills, mcp, memory";

fn map(r: &Row) -> rusqlite::Result<Run> {
    Ok(Run {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        profile_id: r.get(2)?,
        parent_run_id: r.get(3)?,
        provider_id: r.get(4)?,
        model: r.get(5)?,
        permission: r.get(6)?,
        prompt: r.get(7)?,
        status: r.get(8)?,
        session_id: r.get(9)?,
        cwd: r.get(10)?,
        worktree_branch: r.get(11)?,
        base_commit: r.get(12)?,
        result_text: r.get(13)?,
        error: r.get(14)?,
        input_tokens: r.get(15)?,
        output_tokens: r.get(16)?,
        cache_read_tokens: r.get(17)?,
        cost_usd: r.get(18)?,
        duration_ms: r.get(19)?,
        started_at: r.get(20)?,
        ended_at: r.get(21)?,
        skills: serde_json::from_str(&r.get::<_, String>(22)?).unwrap_or_default(),
        mcp: serde_json::from_str(&r.get::<_, String>(23)?).unwrap_or_default(),
        memory: serde_json::from_str(&r.get::<_, String>(24)?).unwrap_or_default(),
    })
}

pub fn insert(conn: &Connection, r: &Run) -> AppResult<()> {
    conn.execute(
        &format!("INSERT INTO agent_runs ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25)"),
        params![
            r.id, r.workspace_id, r.profile_id, r.parent_run_id, r.provider_id, r.model, r.permission, r.prompt, r.status,
            r.session_id, r.cwd, r.worktree_branch, r.base_commit, r.result_text, r.error, r.input_tokens, r.output_tokens,
            r.cache_read_tokens, r.cost_usd, r.duration_ms, r.started_at, r.ended_at, serde_json::to_string(&r.skills)?, serde_json::to_string(&r.mcp)?,
            serde_json::to_string(&r.memory)?
        ],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Run> {
    conn.query_row(&format!("SELECT {COLS} FROM agent_runs WHERE id = ?1"), [id], map)
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("execução não encontrada: {id}")))
}

pub fn list(conn: &Connection, workspace_id: Option<&str>, status: Option<&str>, limit: u32) -> AppResult<Vec<Run>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM agent_runs WHERE (?1 IS NULL OR workspace_id = ?1) AND (?2 IS NULL OR status = ?2)
         ORDER BY started_at DESC LIMIT ?3"
    ))?;
    let rows = stmt
        .query_map(params![workspace_id, status, limit.clamp(1, 500) as i64], map)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn finish(conn: &Connection, r: &Run) -> AppResult<()> {
    conn.execute(
        "UPDATE agent_runs SET status = ?2, session_id = ?3, model = COALESCE(?4, model), result_text = ?5, error = ?6,
         input_tokens = ?7, output_tokens = ?8, cache_read_tokens = ?9, cost_usd = ?10, duration_ms = ?11, ended_at = ?12
         WHERE id = ?1",
        params![
            r.id, r.status, r.session_id, r.model, r.result_text, r.error, r.input_tokens, r.output_tokens,
            r.cache_read_tokens, r.cost_usd, r.duration_ms, r.ended_at
        ],
    )?;
    Ok(())
}

pub fn set_session(conn: &Connection, id: &str, session_id: &str) -> AppResult<()> {
    conn.execute("UPDATE agent_runs SET session_id = ?2 WHERE id = ?1", params![id, session_id])?;
    Ok(())
}

pub fn insert_event(conn: &Connection, run_id: &str, seq: i64, kind: &str, payload: &Value) -> AppResult<()> {
    conn.execute(
        "INSERT INTO agent_events (run_id, seq, kind, payload, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![run_id, seq, kind, payload.to_string(), now_ms()],
    )?;
    Ok(())
}

pub fn events(conn: &Connection, run_id: &str) -> AppResult<Vec<StoredEvent>> {
    let mut stmt = conn.prepare("SELECT seq, payload, created_at FROM agent_events WHERE run_id = ?1 ORDER BY seq")?;
    let rows = stmt
        .query_map([run_id], |r| {
            let payload: String = r.get(1)?;
            Ok(StoredEvent { seq: r.get(0)?, event: serde_json::from_str(&payload).unwrap_or(Value::Null), created_at: r.get(2)? })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Runs left "running" by a previous app session can never finish.
pub fn mark_orphans(conn: &Connection) -> AppResult<usize> {
    Ok(conn.execute(
        "UPDATE agent_runs SET status = 'failed', error = 'interrompida: o FORGE foi fechado durante a execução', ended_at = ?1
         WHERE status = 'running'",
        [now_ms()],
    )?)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    if get(conn, id)?.status == "running" {
        return Err(AppError::Invalid("cancele a execução antes de excluí-la".into()));
    }
    conn.execute("DELETE FROM agent_runs WHERE id = ?1", [id])?;
    Ok(())
}
