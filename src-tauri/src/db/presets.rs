//! User-saved layout presets (global, not tied to a workspace). Built-in
//! presets live in the frontend and are not stored here.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;

use super::sessions::{validate_layout, validate_name};
use super::{new_id, now_ms};
use crate::error::{AppError, AppResult};

pub const MAX_PRESETS: i64 = 100;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub layout_json: String,
    pub created_at: i64,
}

fn map(r: &Row) -> rusqlite::Result<Preset> {
    Ok(Preset { id: r.get(0)?, name: r.get(1)?, layout_json: r.get(2)?, created_at: r.get(3)? })
}

fn name_taken(conn: &Connection, name: &str, except: Option<&str>) -> AppResult<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM layout_presets WHERE name = ?1 COLLATE NOCASE AND (?2 IS NULL OR id <> ?2)",
            params![name, except],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

pub fn list(conn: &Connection) -> AppResult<Vec<Preset>> {
    let mut stmt = conn.prepare("SELECT id, name, layout_json, created_at FROM layout_presets ORDER BY name COLLATE NOCASE")?;
    let rows = stmt.query_map([], map)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Preset> {
    conn.query_row("SELECT id, name, layout_json, created_at FROM layout_presets WHERE id = ?1", [id], map)
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("layout not found: {id}")))
}

pub fn create(conn: &Connection, name: &str, layout_json: &str) -> AppResult<Preset> {
    let name = validate_name(name)?;
    validate_layout(layout_json)?;
    if name_taken(conn, &name, None)? {
        return Err(AppError::Invalid(format!("a layout named “{name}” already exists")));
    }
    let count: i64 = conn.query_row("SELECT count(*) FROM layout_presets", [], |r| r.get(0))?;
    if count >= MAX_PRESETS {
        return Err(AppError::Invalid(format!("at most {MAX_PRESETS} saved layouts")));
    }
    let id = new_id();
    conn.execute(
        "INSERT INTO layout_presets (id, name, layout_json, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![id, name, layout_json, now_ms()],
    )?;
    get(conn, &id)
}

pub fn rename(conn: &Connection, id: &str, name: &str) -> AppResult<Preset> {
    let name = validate_name(name)?;
    if name_taken(conn, &name, Some(id))? {
        return Err(AppError::Invalid(format!("a layout named “{name}” already exists")));
    }
    conn.execute("UPDATE layout_presets SET name = ?2 WHERE id = ?1", params![id, name])?;
    get(conn, id)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM layout_presets WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn crud_with_case_insensitive_unique_names() {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            let a = create(c, "Review", r#"{"version":1}"#)?;
            assert!(create(c, "review", r#"{"version":1}"#).is_err());
            let b = create(c, "Debug", r#"{"version":1}"#)?;
            assert!(rename(c, &b.id, "REVIEW").is_err());
            assert_eq!(rename(c, &a.id, "review")?.name, "review", "renaming to itself is fine");
            assert_eq!(list(c)?.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Debug", "review"]);
            delete(c, &a.id)?;
            assert_eq!(list(c)?.len(), 1);
            Ok(())
        })
        .unwrap();
    }
}
