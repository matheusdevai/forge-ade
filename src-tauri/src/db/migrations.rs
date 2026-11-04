//! Forward-only migrations tracked through `PRAGMA user_version`.
//! Never edit a released migration — append a new file instead.

use rusqlite::Connection;

use crate::error::AppResult;

const MIGRATIONS: &[&str] = &[
    include_str!("../../migrations/0001_foundation.sql"),
    include_str!("../../migrations/0002_sessions_presets.sql"),
    include_str!("../../migrations/0003_provider_checks.sql"),
    include_str!("../../migrations/0004_agents.sql"),
    include_str!("../../migrations/0005_missions_engine.sql"),
    include_str!("../../migrations/0006_maestro.sql"),
    include_str!("../../migrations/0007_squads_recipes.sql"),
    include_str!("../../migrations/0008_skills.sql"),
    include_str!("../../migrations/0009_mcp.sql"),
    include_str!("../../migrations/0010_memory.sql"),
    include_str!("../../migrations/0011_harness.sql"),
    include_str!("../../migrations/0012_media.sql"),
    include_str!("../../migrations/0013_autopilot.sql"),
];

pub fn current_version(conn: &Connection) -> AppResult<usize> {
    let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    Ok(v as usize)
}

pub fn latest_version() -> usize {
    MIGRATIONS.len()
}

pub fn run(conn: &mut Connection) -> AppResult<()> {
    run_to(conn, MIGRATIONS.len())
}

/// Applies migrations up to (and including) version `target`.
pub fn run_to(conn: &mut Connection, target: usize) -> AppResult<()> {
    let current = current_version(conn)?;
    for (idx, sql) in MIGRATIONS.iter().enumerate().take(target).skip(current) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (idx + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_to_latest_and_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), latest_version());
        run(&mut conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), latest_version());
        let tables: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN \
                 ('app_settings','workspaces','workspace_sessions','layout_presets','missions','activity_events')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 6);
    }

    #[test]
    fn v2_moves_existing_layouts_into_a_principal_session() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        run_to(&mut conn, 1).unwrap();
        conn.execute_batch(
            "INSERT INTO workspaces (id, name, path, created_at) VALUES ('w1', 'w', '/tmp/w', 1), ('w2', 'v', '/tmp/v', 1);
             INSERT INTO pane_layouts (workspace_id, layout_json, updated_at) VALUES ('w1', '{\"version\":1}', 5);",
        )
        .unwrap();
        run(&mut conn).unwrap();
        let (name, json, active): (String, String, Option<String>) = conn
            .query_row(
                "SELECT s.name, s.layout_json, w.active_session_id = s.id FROM workspace_sessions s
                 JOIN workspaces w ON w.id = s.workspace_id WHERE s.workspace_id = 'w1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, Option<i64>>(2)?.map(|v| v.to_string()))),
            )
            .unwrap();
        assert_eq!(name, "Principal");
        assert_eq!(json, r#"{"version":1}"#);
        assert_eq!(active.as_deref(), Some("1"));
        let w2: Option<String> = conn
            .query_row("SELECT active_session_id FROM workspaces WHERE id = 'w2'", [], |r| r.get(0))
            .unwrap();
        assert!(w2.is_none());
        let old: i64 = conn
            .query_row("SELECT count(*) FROM sqlite_master WHERE name = 'pane_layouts'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(old, 0);
    }
}
