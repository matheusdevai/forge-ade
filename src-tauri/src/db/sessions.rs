//! Workspace sessions: each workspace has 1..=MAX_SESSIONS named sessions
//! (tabs), each owning a pane layout. The layout JSON schema belongs to the
//! frontend (`src/features/panes/layout.ts`); here we only enforce
//! well-formedness and size.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;

use super::{new_id, now_ms};
use crate::error::{AppError, AppResult};

pub const MAX_SESSIONS: i64 = 20;
const MAX_LAYOUT_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub position: i64,
    pub layout_json: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionsState {
    pub sessions: Vec<Session>,
    pub active_id: Option<String>,
}

const COLS: &str = "id, workspace_id, name, position, layout_json, created_at, updated_at";

fn map(r: &Row) -> rusqlite::Result<Session> {
    Ok(Session {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        name: r.get(2)?,
        position: r.get(3)?,
        layout_json: r.get(4)?,
        created_at: r.get(5)?,
        updated_at: r.get(6)?,
    })
}

pub fn validate_layout(json: &str) -> AppResult<()> {
    if json.len() > MAX_LAYOUT_BYTES {
        return Err(AppError::Invalid("layout too large".into()));
    }
    let v: serde_json::Value = serde_json::from_str(json)?;
    if !v.is_object() {
        return Err(AppError::Invalid("layout must be a JSON object".into()));
    }
    Ok(())
}

pub fn validate_name(name: &str) -> AppResult<String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > 60 {
        return Err(AppError::Invalid("name must be 1–60 characters".into()));
    }
    Ok(n.to_string())
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Session> {
    conn.query_row(&format!("SELECT {COLS} FROM workspace_sessions WHERE id = ?1"), [id], map)
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("session not found: {id}")))
}

pub fn state(conn: &Connection, workspace_id: &str) -> AppResult<SessionsState> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM workspace_sessions WHERE workspace_id = ?1 ORDER BY position, created_at"
    ))?;
    let sessions = stmt.query_map([workspace_id], map)?.collect::<Result<Vec<_>, _>>()?;
    let stored: Option<String> = conn
        .query_row("SELECT active_session_id FROM workspaces WHERE id = ?1", [workspace_id], |r| r.get(0))
        .optional()?
        .flatten();
    let active_id = stored
        .filter(|id| sessions.iter().any(|s| &s.id == id))
        .or_else(|| sessions.first().map(|s| s.id.clone()));
    Ok(SessionsState { sessions, active_id })
}

pub fn create(conn: &Connection, workspace_id: &str, name: &str, layout_json: &str) -> AppResult<Session> {
    let name = validate_name(name)?;
    validate_layout(layout_json)?;
    let (count, max_pos): (i64, Option<i64>) = conn.query_row(
        "SELECT count(*), max(position) FROM workspace_sessions WHERE workspace_id = ?1",
        [workspace_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if count >= MAX_SESSIONS {
        return Err(AppError::Invalid(format!("a workspace can have at most {MAX_SESSIONS} sessions")));
    }
    let id = new_id();
    let now = now_ms();
    conn.execute(
        "INSERT INTO workspace_sessions (id, workspace_id, name, position, layout_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![id, workspace_id, name, max_pos.map_or(0, |p| p + 1), layout_json, now],
    )?;
    set_active(conn, workspace_id, &id)?;
    get(conn, &id)
}

pub fn rename(conn: &Connection, id: &str, name: &str) -> AppResult<Session> {
    let name = validate_name(name)?;
    conn.execute(
        "UPDATE workspace_sessions SET name = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, name, now_ms()],
    )?;
    get(conn, id)
}

pub fn save_layout(conn: &Connection, id: &str, layout_json: &str) -> AppResult<()> {
    validate_layout(layout_json)?;
    let n = conn.execute(
        "UPDATE workspace_sessions SET layout_json = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, layout_json, now_ms()],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!("session not found: {id}")));
    }
    Ok(())
}

pub fn set_active(conn: &Connection, workspace_id: &str, id: &str) -> AppResult<()> {
    let s = get(conn, id)?;
    if s.workspace_id != workspace_id {
        return Err(AppError::Invalid("session belongs to another workspace".into()));
    }
    conn.execute(
        "UPDATE workspaces SET active_session_id = ?2 WHERE id = ?1",
        params![workspace_id, id],
    )?;
    Ok(())
}

/// Deletes a session. The last session of a workspace cannot be deleted.
/// If the active session is removed, its neighbour becomes active.
pub fn delete(conn: &Connection, id: &str) -> AppResult<SessionsState> {
    let s = get(conn, id)?;
    let before = state(conn, &s.workspace_id)?;
    if before.sessions.len() <= 1 {
        return Err(AppError::Invalid("a workspace must keep at least one session".into()));
    }
    let idx = before.sessions.iter().position(|x| x.id == id).unwrap_or(0);
    conn.execute("DELETE FROM workspace_sessions WHERE id = ?1", [id])?;
    if before.active_id.as_deref() == Some(id) {
        let remaining: Vec<_> = before.sessions.iter().filter(|x| x.id != id).collect();
        let next = remaining[idx.min(remaining.len() - 1)];
        set_active(conn, &s.workspace_id, &next.id)?;
    }
    state(conn, &s.workspace_id)
}

/// Reorders sessions to match `ids` (must be a permutation of the workspace's sessions).
pub fn reorder(conn: &Connection, workspace_id: &str, ids: &[String]) -> AppResult<SessionsState> {
    let current = state(conn, workspace_id)?;
    let mut a: Vec<&str> = current.sessions.iter().map(|s| s.id.as_str()).collect();
    let mut b: Vec<&str> = ids.iter().map(String::as_str).collect();
    a.sort_unstable();
    b.sort_unstable();
    if a != b {
        return Err(AppError::Invalid("reorder must include every session exactly once".into()));
    }
    for (pos, id) in ids.iter().enumerate() {
        conn.execute("UPDATE workspace_sessions SET position = ?2 WHERE id = ?1", params![id, pos as i64])?;
    }
    state(conn, workspace_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{workspaces, Db};

    const L: &str = r#"{"version":1}"#;

    #[test]
    fn create_list_activate_delete() {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            let ws = workspaces::add(c, "/tmp/w", "w")?;
            assert!(state(c, &ws.id)?.sessions.is_empty());
            let a = create(c, &ws.id, "Principal", L)?;
            let b = create(c, &ws.id, " Debug ", L)?;
            let st = state(c, &ws.id)?;
            assert_eq!(st.sessions.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Principal", "Debug"]);
            assert_eq!(st.active_id.as_deref(), Some(b.id.as_str()), "new session becomes active");
            set_active(c, &ws.id, &a.id)?;
            assert_eq!(state(c, &ws.id)?.active_id.as_deref(), Some(a.id.as_str()));
            let st = delete(c, &a.id)?;
            assert_eq!(st.active_id.as_deref(), Some(b.id.as_str()), "neighbour becomes active");
            assert!(delete(c, &b.id).is_err(), "last session is protected");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn validates_and_isolates_workspaces() {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            let w1 = workspaces::add(c, "/tmp/1", "1")?;
            let w2 = workspaces::add(c, "/tmp/2", "2")?;
            assert!(create(c, &w1.id, "", L).is_err());
            assert!(create(c, &w1.id, "x", "[]").is_err());
            let s = create(c, &w1.id, "x", L)?;
            assert!(set_active(c, &w2.id, &s.id).is_err());
            save_layout(c, &s.id, r#"{"version":1,"root":null}"#)?;
            assert!(save_layout(c, &s.id, "nope").is_err());
            assert_eq!(rename(c, &s.id, "Renomeada")?.name, "Renomeada");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn reorder_and_cap() {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            let ws = workspaces::add(c, "/tmp/w", "w")?;
            let a = create(c, &ws.id, "a", L)?;
            let b = create(c, &ws.id, "b", L)?;
            let st = reorder(c, &ws.id, &[b.id.clone(), a.id.clone()])?;
            assert_eq!(st.sessions[0].id, b.id);
            assert!(reorder(c, &ws.id, std::slice::from_ref(&a.id)).is_err());
            for i in 0..(MAX_SESSIONS - 2) {
                create(c, &ws.id, &format!("s{i}"), L)?;
            }
            assert!(create(c, &ws.id, "overflow", L).is_err());
            workspaces::remove(c, &ws.id)?;
            assert!(get(c, &a.id).is_err(), "sessions cascade with workspace");
            Ok(())
        })
        .unwrap();
    }
}
