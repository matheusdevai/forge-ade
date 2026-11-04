//! Activity log shown on the dashboard. Messages are short, human-readable
//! and must never contain secrets or file contents.

use rusqlite::{params, Connection};
use serde::Serialize;

use super::now_ms;
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEvent {
    pub id: i64,
    pub kind: String,
    pub message: String,
    pub workspace_id: Option<String>,
    pub mission_id: Option<String>,
    pub created_at: i64,
}

pub fn log(
    conn: &Connection,
    kind: &str,
    message: &str,
    workspace_id: Option<&str>,
    mission_id: Option<&str>,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO activity_events (kind, message, workspace_id, mission_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![kind, message, workspace_id, mission_id, now_ms()],
    )?;
    // Keep the table bounded.
    conn.execute(
        "DELETE FROM activity_events WHERE id <= (SELECT MAX(id) - 5000 FROM activity_events)",
        [],
    )?;
    Ok(())
}

pub fn recent(conn: &Connection, limit: u32) -> AppResult<Vec<ActivityEvent>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, message, workspace_id, mission_id, created_at
         FROM activity_events ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map([limit.clamp(1, 200) as i64], |r| {
            Ok(ActivityEvent {
                id: r.get(0)?,
                kind: r.get(1)?,
                message: r.get(2)?,
                workspace_id: r.get(3)?,
                mission_id: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn logs_newest_first() {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            log(c, "a", "first", None, None)?;
            log(c, "b", "second", None, None)?;
            let r = recent(c, 10)?;
            assert_eq!(r[0].message, "second");
            assert_eq!(r.len(), 2);
            Ok(())
        })
        .unwrap();
    }
}
