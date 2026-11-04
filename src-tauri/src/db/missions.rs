//! Missions. In Milestone 01 missions can be drafted, listed, cancelled and
//! deleted. Planning/execution arrive with the Mission Engine (M08) and the
//! Maestro (M09); nothing here pretends to execute anything.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use super::{new_id, now_ms};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mission {
    pub id: String,
    pub workspace_id: Option<String>,
    pub title: String,
    pub briefing: String,
    pub mode: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub plan_run_id: Option<String>,
    pub planning: bool,
    pub max_parallel: i64,
    pub require_review: bool,
    pub base_branch: Option<String>,
    pub base_commit: Option<String>,
    pub approved_at: Option<i64>,
    pub integrated_at: Option<i64>,
    pub max_attempts: i64,
    pub max_runs: i64,
    pub budget_usd: Option<f64>,
    pub report: Option<String>,
    pub squad_id: Option<String>,
    pub recipe_id: Option<String>,
    /// Harness picks the Claude model tier per task (M16).
    pub harness: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMission {
    pub workspace_id: Option<String>,
    pub title: String,
    pub briefing: String,
    pub mode: String,
}

const COLS: &str = "id, workspace_id, title, briefing, mode, status, created_at, updated_at, plan_run_id, planning, max_parallel, require_review, base_branch, base_commit, approved_at, integrated_at, max_attempts, max_runs, budget_usd, report, squad_id, recipe_id, harness";
const MODES: &[&str] = &["free", "squad", "agentic"];

fn map(r: &Row) -> rusqlite::Result<Mission> {
    Ok(Mission {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        title: r.get(2)?,
        briefing: r.get(3)?,
        mode: r.get(4)?,
        status: r.get(5)?,
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
        plan_run_id: r.get(8)?,
        planning: r.get::<_, i64>(9)? != 0,
        max_parallel: r.get(10)?,
        require_review: r.get::<_, i64>(11)? != 0,
        base_branch: r.get(12)?,
        base_commit: r.get(13)?,
        approved_at: r.get(14)?,
        integrated_at: r.get(15)?,
        max_attempts: r.get(16)?,
        max_runs: r.get(17)?,
        budget_usd: r.get(18)?,
        report: r.get(19)?,
        squad_id: r.get(20)?,
        recipe_id: r.get(21)?,
        harness: r.get::<_, i64>(22)? != 0,
    })
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Mission> {
    conn.query_row(&format!("SELECT {COLS} FROM missions WHERE id = ?1"), [id], map)
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("mission not found: {id}")))
}

pub fn list(conn: &Connection, workspace_id: Option<&str>, limit: u32) -> AppResult<Vec<Mission>> {
    let limit = limit.clamp(1, 500) as i64;
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM missions WHERE (?1 IS NULL OR workspace_id = ?1)
         ORDER BY updated_at DESC LIMIT ?2"
    ))?;
    let rows = stmt
        .query_map(params![workspace_id, limit], map)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn create(conn: &Connection, input: &NewMission) -> AppResult<Mission> {
    let title = input.title.trim();
    let briefing = input.briefing.trim();
    if title.is_empty() || title.chars().count() > 200 {
        return Err(AppError::Invalid("mission title must be 1–200 characters".into()));
    }
    if briefing.is_empty() || briefing.chars().count() > 20_000 {
        return Err(AppError::Invalid("briefing must be 1–20000 characters".into()));
    }
    if !MODES.contains(&input.mode.as_str()) {
        return Err(AppError::Invalid(format!("invalid mission mode: {}", input.mode)));
    }
    let id = new_id();
    let now = now_ms();
    conn.execute(
        "INSERT INTO missions (id, workspace_id, title, briefing, mode, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'draft', ?6, ?6)",
        params![id, input.workspace_id, title, briefing, input.mode, now],
    )?;
    get(conn, &id)
}

pub fn set_status(conn: &Connection, id: &str, status: &str) -> AppResult<()> {
    conn.execute("UPDATE missions SET status = ?2, updated_at = ?3 WHERE id = ?1", params![id, status, now_ms()])?;
    Ok(())
}

pub fn set_settings(conn: &Connection, id: &str, max_parallel: i64, require_review: bool) -> AppResult<Mission> {
    if !(1..=4).contains(&max_parallel) {
        return Err(AppError::Invalid("paralelismo deve ficar entre 1 e 4".into()));
    }
    conn.execute(
        "UPDATE missions SET max_parallel = ?2, require_review = ?3, updated_at = ?4 WHERE id = ?1",
        params![id, max_parallel, require_review as i64, now_ms()],
    )?;
    get(conn, id)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaestroSettings {
    pub mode: String,
    pub max_parallel: i64,
    pub require_review: bool,
    pub max_attempts: i64,
    pub max_runs: i64,
    pub budget_usd: Option<f64>,
}

pub fn set_maestro(conn: &Connection, id: &str, s: &MaestroSettings) -> AppResult<Mission> {
    if !MODES.contains(&s.mode.as_str()) {
        return Err(AppError::Invalid(format!("modo inválido: {}", s.mode)));
    }
    if !(1..=4).contains(&s.max_parallel) || !(1..=10).contains(&s.max_attempts) || !(1..=500).contains(&s.max_runs) {
        return Err(AppError::Invalid("limites inválidos (paralelismo 1–4, tentativas 1–10, execuções 1–500)".into()));
    }
    if s.budget_usd.is_some_and(|b| !(b > 0.0 && b < 100_000.0)) {
        return Err(AppError::Invalid("orçamento inválido".into()));
    }
    let m = get(conn, id)?;
    if matches!(m.status.as_str(), "completed" | "cancelled") {
        return Err(AppError::Invalid("a missão já terminou".into()));
    }
    conn.execute(
        "UPDATE missions SET mode = ?2, max_parallel = ?3, require_review = ?4, max_attempts = ?5, max_runs = ?6, budget_usd = ?7, updated_at = ?8 WHERE id = ?1",
        params![id, s.mode, s.max_parallel, s.require_review as i64, s.max_attempts, s.max_runs, s.budget_usd, now_ms()],
    )?;
    get(conn, id)
}

pub fn set_report(conn: &Connection, id: &str, report: &str) -> AppResult<()> {
    conn.execute("UPDATE missions SET report = ?2 WHERE id = ?1", params![id, report])?;
    Ok(())
}

pub fn set_squad(conn: &Connection, id: &str, squad_id: Option<&str>) -> AppResult<()> {
    let m = get(conn, id)?;
    if !matches!(m.status.as_str(), "draft" | "planned") {
        return Err(AppError::Invalid("o squad só pode ser trocado antes do início da missão".into()));
    }
    conn.execute("UPDATE missions SET squad_id = ?2, updated_at = ?3 WHERE id = ?1", params![id, squad_id, now_ms()])?;
    Ok(())
}

pub fn update_brief(conn: &Connection, id: &str, title: &str, briefing: &str, workspace_id: Option<&str>) -> AppResult<Mission> {
    let m = get(conn, id)?;
    if !matches!(m.status.as_str(), "draft" | "planned") {
        return Err(AppError::Invalid("o briefing só pode ser editado antes do início da missão".into()));
    }
    let (title, briefing) = (title.trim(), briefing.trim());
    if title.is_empty() || title.chars().count() > 200 || briefing.is_empty() || briefing.chars().count() > 20_000 {
        return Err(AppError::Invalid("título (1–200) e briefing (1–20000) obrigatórios".into()));
    }
    conn.execute(
        "UPDATE missions SET title = ?2, briefing = ?3, workspace_id = ?4, updated_at = ?5 WHERE id = ?1",
        params![id, title, briefing, workspace_id, now_ms()],
    )?;
    get(conn, id)
}

pub fn cancel(conn: &Connection, id: &str) -> AppResult<Mission> {
    let m = get(conn, id)?;
    if matches!(m.status.as_str(), "completed" | "cancelled") {
        return Err(AppError::Invalid(format!("mission is already {}", m.status)));
    }
    conn.execute(
        "UPDATE missions SET status = 'cancelled', updated_at = ?2 WHERE id = ?1",
        params![id, now_ms()],
    )?;
    get(conn, id)
}

/// Only drafts and cancelled missions may be deleted; history of executed
/// missions must be preserved.
pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    let m = get(conn, id)?;
    if !matches!(m.status.as_str(), "draft" | "cancelled") {
        return Err(AppError::Invalid("only draft or cancelled missions can be deleted".into()));
    }
    conn.execute("DELETE FROM missions WHERE id = ?1", [id])?;
    Ok(())
}

pub fn set_harness(conn: &Connection, id: &str, on: bool) -> AppResult<Mission> {
    conn.execute("UPDATE missions SET harness = ?2, updated_at = ?3 WHERE id = ?1", params![id, on as i64, now_ms()])?;
    get(conn, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{workspaces, Db};

    fn input(ws: Option<String>) -> NewMission {
        NewMission {
            workspace_id: ws,
            title: "  SaaS  ".into(),
            briefing: "Auth, dashboard, payments".into(),
            mode: "squad".into(),
        }
    }

    #[test]
    fn create_list_cancel_delete() {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            let ws = workspaces::add(c, "/tmp/w", "w")?;
            let m = create(c, &input(Some(ws.id.clone())))?;
            assert_eq!(m.title, "SaaS");
            assert_eq!(m.status, "draft");
            create(c, &input(None))?;
            assert_eq!(list(c, None, 50)?.len(), 2);
            assert_eq!(list(c, Some(&ws.id), 50)?.len(), 1);
            assert_eq!(cancel(c, &m.id)?.status, "cancelled");
            assert!(cancel(c, &m.id).is_err());
            delete(c, &m.id)?;
            assert!(get(c, &m.id).is_err());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn validates_input() {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            let mut bad = input(None);
            bad.mode = "yolo".into();
            assert!(create(c, &bad).is_err());
            let mut bad = input(None);
            bad.title = " ".into();
            assert!(create(c, &bad).is_err());
            let mut bad = input(None);
            bad.workspace_id = Some("missing".into());
            assert!(create(c, &bad).is_err(), "foreign key must be enforced");
            Ok(())
        })
        .unwrap();
    }
}

