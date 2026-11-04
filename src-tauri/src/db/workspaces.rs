use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;

use super::{new_id, now_ms};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub path: String,
    pub pinned: bool,
    pub created_at: i64,
    pub last_opened_at: Option<i64>,
}

const COLS: &str = "id, name, path, pinned, created_at, last_opened_at";

fn map(r: &Row) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: r.get(0)?,
        name: r.get(1)?,
        path: r.get(2)?,
        pinned: r.get::<_, i64>(3)? != 0,
        created_at: r.get(4)?,
        last_opened_at: r.get(5)?,
    })
}

pub fn list(conn: &Connection) -> AppResult<Vec<Workspace>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM workspaces ORDER BY pinned DESC, COALESCE(last_opened_at, created_at) DESC"
    ))?;
    let rows = stmt.query_map([], map)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Workspace> {
    conn.query_row(&format!("SELECT {COLS} FROM workspaces WHERE id = ?1"), [id], map)
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("workspace not found: {id}")))
}

/// Registers a directory as a workspace. `canonical_path` must already be
/// canonicalized by the caller. Re-adding an existing path returns it.
pub fn add(conn: &Connection, canonical_path: &str, name: &str) -> AppResult<Workspace> {
    conn.execute(
        "INSERT INTO workspaces (id, name, path, created_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(path) DO NOTHING",
        params![new_id(), name, canonical_path, now_ms()],
    )?;
    let ws = conn.query_row(
        &format!("SELECT {COLS} FROM workspaces WHERE path = ?1"),
        [canonical_path],
        map,
    )?;
    Ok(ws)
}

pub fn touch(conn: &Connection, id: &str) -> AppResult<Workspace> {
    let n = conn.execute(
        "UPDATE workspaces SET last_opened_at = ?2 WHERE id = ?1",
        params![id, now_ms()],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!("workspace not found: {id}")));
    }
    get(conn, id)
}

pub fn set_pinned(conn: &Connection, id: &str, pinned: bool) -> AppResult<Workspace> {
    conn.execute(
        "UPDATE workspaces SET pinned = ?2 WHERE id = ?1",
        params![id, pinned as i64],
    )?;
    get(conn, id)
}

pub fn rename(conn: &Connection, id: &str, name: &str) -> AppResult<Workspace> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 120 {
        return Err(AppError::Invalid("workspace name must be 1–120 characters".into()));
    }
    conn.execute("UPDATE workspaces SET name = ?2 WHERE id = ?1", params![id, name])?;
    get(conn, id)
}

/// Removes the workspace record only. Files on disk are never touched.
pub fn remove(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM workspaces WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn add_is_idempotent_by_path_and_orders_recent_first() {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            let a = add(c, "/tmp/a", "a")?;
            let a2 = add(c, "/tmp/a", "a")?;
            assert_eq!(a.id, a2.id);
            let b = add(c, "/tmp/b", "b")?;
            std::thread::sleep(std::time::Duration::from_millis(2));
            touch(c, &a.id)?;
            let all = list(c)?;
            assert_eq!(all.len(), 2);
            assert_eq!(all[0].id, a.id);
            set_pinned(c, &b.id, true)?;
            assert_eq!(list(c)?[0].id, b.id);
            remove(c, &a.id)?;
            assert!(get(c, &a.id).is_err());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn rename_validates() {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            let a = add(c, "/tmp/a", "a")?;
            assert!(rename(c, &a.id, "   ").is_err());
            assert_eq!(rename(c, &a.id, " Novo ")?.name, "Novo");
            Ok(())
        })
        .unwrap();
    }
}
