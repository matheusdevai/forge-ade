//! Mission tasks, events and the planner's JSON plan format.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agents::profiles::Profile;
use crate::db::{new_id, now_ms};
use crate::error::{AppError, AppResult};

pub const MAX_TASKS: usize = 20;
pub const PLAN_OPEN: &str = "<forge-plan>";
pub const PLAN_CLOSE: &str = "</forge-plan>";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub mission_id: String,
    pub position: i64,
    pub key: String,
    pub title: String,
    pub description: String,
    pub acceptance: String,
    pub profile_id: Option<String>,
    pub depends_on: Vec<String>,
    pub status: String,
    pub run_id: Option<String>,
    pub worktree_path: Option<String>,
    pub worktree_branch: Option<String>,
    pub attempts: i64,
    pub feedback: Option<String>,
    pub integrated_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub base_commit: Option<String>,
    pub review_run_id: Option<String>,
    pub review_verdict: Option<String>,
    pub review_summary: Option<String>,
    pub provider_override: Option<String>,
}

/// Editable task definition (dependencies by `key`).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskInput {
    pub key: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub acceptance: String,
    pub profile_id: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissionEvent {
    pub id: i64,
    pub task_id: Option<String>,
    pub kind: String,
    pub message: String,
    pub created_at: i64,
}

const COLS: &str = "id, mission_id, position, key, title, description, acceptance, profile_id, depends_on, status, run_id,
  worktree_path, worktree_branch, attempts, feedback, integrated_at, created_at, updated_at,
  base_commit, review_run_id, review_verdict, review_summary, provider_override";

fn map(r: &Row) -> rusqlite::Result<Task> {
    let deps: String = r.get(8)?;
    Ok(Task {
        id: r.get(0)?,
        mission_id: r.get(1)?,
        position: r.get(2)?,
        key: r.get(3)?,
        title: r.get(4)?,
        description: r.get(5)?,
        acceptance: r.get(6)?,
        profile_id: r.get(7)?,
        depends_on: serde_json::from_str(&deps).unwrap_or_default(),
        status: r.get(9)?,
        run_id: r.get(10)?,
        worktree_path: r.get(11)?,
        worktree_branch: r.get(12)?,
        attempts: r.get(13)?,
        feedback: r.get(14)?,
        integrated_at: r.get(15)?,
        created_at: r.get(16)?,
        updated_at: r.get(17)?,
        base_commit: r.get(18)?,
        review_run_id: r.get(19)?,
        review_verdict: r.get(20)?,
        review_summary: r.get(21)?,
        provider_override: r.get(22)?,
    })
}

pub fn list(conn: &Connection, mission_id: &str) -> AppResult<Vec<Task>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM mission_tasks WHERE mission_id = ?1 ORDER BY position"))?;
    let rows = stmt.query_map([mission_id], map)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Task> {
    conn.query_row(&format!("SELECT {COLS} FROM mission_tasks WHERE id = ?1"), [id], map)
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("tarefa não encontrada: {id}")))
}

pub fn set_status(conn: &Connection, id: &str, status: &str) -> AppResult<()> {
    conn.execute("UPDATE mission_tasks SET status = ?2, updated_at = ?3 WHERE id = ?1", params![id, status, now_ms()])?;
    Ok(())
}

pub fn set_run(conn: &Connection, id: &str, run_id: &str, worktree_path: &str, worktree_branch: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE mission_tasks SET status = 'running', run_id = ?2, worktree_path = ?3, worktree_branch = ?4,
         attempts = attempts + 1, updated_at = ?5 WHERE id = ?1",
        params![id, run_id, worktree_path, worktree_branch, now_ms()],
    )?;
    Ok(())
}

pub fn set_base_commit(conn: &Connection, id: &str, sha: Option<&str>) -> AppResult<()> {
    conn.execute("UPDATE mission_tasks SET base_commit = ?2 WHERE id = ?1", params![id, sha])?;
    Ok(())
}

pub fn set_review(conn: &Connection, id: &str, run_id: Option<&str>, verdict: Option<&str>, summary: Option<&str>) -> AppResult<()> {
    conn.execute(
        "UPDATE mission_tasks SET review_run_id = COALESCE(?2, review_run_id), review_verdict = ?3, review_summary = ?4, updated_at = ?5 WHERE id = ?1",
        params![id, run_id, verdict, summary, now_ms()],
    )?;
    Ok(())
}

pub fn set_provider_override(conn: &Connection, id: &str, provider: Option<&str>) -> AppResult<()> {
    conn.execute("UPDATE mission_tasks SET provider_override = ?2, updated_at = ?3 WHERE id = ?1", params![id, provider, now_ms()])?;
    Ok(())
}

pub fn set_feedback(conn: &Connection, id: &str, feedback: Option<&str>) -> AppResult<()> {
    conn.execute("UPDATE mission_tasks SET feedback = ?2, updated_at = ?3 WHERE id = ?1", params![id, feedback, now_ms()])?;
    Ok(())
}

pub fn mark_integrated(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("UPDATE mission_tasks SET integrated_at = ?2 WHERE id = ?1", params![id, now_ms()])?;
    Ok(())
}

pub fn clear_worktree(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("UPDATE mission_tasks SET worktree_path = NULL WHERE id = ?1", [id])?;
    Ok(())
}

pub fn event(conn: &Connection, mission_id: &str, task_id: Option<&str>, kind: &str, message: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO mission_events (mission_id, task_id, kind, message, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![mission_id, task_id, kind, crate::redact::sanitize(message, 4000), now_ms()],
    )?;
    Ok(())
}

pub fn events(conn: &Connection, mission_id: &str, limit: u32) -> AppResult<Vec<MissionEvent>> {
    let mut stmt = conn.prepare(
        "SELECT id, task_id, kind, message, created_at FROM mission_events WHERE mission_id = ?1 ORDER BY id DESC LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![mission_id, limit.clamp(1, 1000) as i64], |r| {
            Ok(MissionEvent { id: r.get(0)?, task_id: r.get(1)?, kind: r.get(2)?, message: r.get(3)?, created_at: r.get(4)? })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn link_run(conn: &Connection, mission_id: &str, task_id: Option<&str>, run_id: &str, kind: &str) -> AppResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO mission_runs (mission_id, task_id, run_id, kind, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![mission_id, task_id, run_id, kind, now_ms()],
    )?;
    Ok(())
}

pub fn run_ids(conn: &Connection, mission_id: &str) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare("SELECT run_id FROM mission_runs WHERE mission_id = ?1 ORDER BY created_at")?;
    let rows = stmt.query_map([mission_id], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

// ── Validation & replacement ──────────────────────────────────────────────

fn valid_key(k: &str) -> bool {
    !k.is_empty() && k.len() <= 40 && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Validates a plan: unique keys, known profiles, existing dependencies, no cycles.
/// Returns the tasks in a dependency-respecting order (stable w.r.t. input order).
pub fn validate(inputs: &[TaskInput], profiles: &[Profile]) -> AppResult<Vec<TaskInput>> {
    if inputs.is_empty() || inputs.len() > MAX_TASKS {
        return Err(AppError::Invalid(format!("o plano deve ter de 1 a {MAX_TASKS} tarefas")));
    }
    let mut seen = HashSet::new();
    for t in inputs {
        if !valid_key(&t.key) {
            return Err(AppError::Invalid(format!("chave de tarefa inválida: {:?}", t.key)));
        }
        if !seen.insert(t.key.as_str()) {
            return Err(AppError::Invalid(format!("chave duplicada: {}", t.key)));
        }
        if t.title.trim().is_empty() || t.title.chars().count() > 200 {
            return Err(AppError::Invalid(format!("título inválido na tarefa {}", t.key)));
        }
        if t.description.trim().is_empty() || t.description.chars().count() > 20_000 || t.acceptance.chars().count() > 5_000 {
            return Err(AppError::Invalid(format!("descrição inválida na tarefa {}", t.key)));
        }
        if let Some(p) = &t.profile_id {
            if !profiles.iter().any(|x| &x.id == p) {
                return Err(AppError::Invalid(format!("perfil desconhecido na tarefa {}", t.key)));
            }
        } else {
            return Err(AppError::Invalid(format!("a tarefa {} precisa de um agente", t.key)));
        }
    }
    for t in inputs {
        for d in &t.depends_on {
            if d == &t.key {
                return Err(AppError::Invalid(format!("a tarefa {} depende de si mesma", t.key)));
            }
            if !seen.contains(d.as_str()) {
                return Err(AppError::Invalid(format!("a tarefa {} depende de {d}, que não existe", t.key)));
            }
        }
    }
    // Kahn's algorithm, picking ready tasks in input order.
    let mut indeg: HashMap<&str, usize> = inputs.iter().map(|t| (t.key.as_str(), t.depends_on.len())).collect();
    let mut done: HashSet<&str> = HashSet::new();
    let mut out = Vec::with_capacity(inputs.len());
    while out.len() < inputs.len() {
        let next = inputs.iter().find(|t| !done.contains(t.key.as_str()) && indeg[t.key.as_str()] == 0);
        let Some(t) = next else {
            return Err(AppError::Invalid("o plano tem dependências circulares".into()));
        };
        done.insert(t.key.as_str());
        out.push(t.clone());
        for u in inputs {
            if u.depends_on.iter().any(|d| d == &t.key) {
                *indeg.get_mut(u.key.as_str()).unwrap() -= 1;
            }
        }
    }
    Ok(out)
}

/// Replaces all tasks of a mission with a validated plan.
pub fn replace(conn: &Connection, mission_id: &str, inputs: &[TaskInput], profiles: &[Profile]) -> AppResult<Vec<Task>> {
    let ordered = validate(inputs, profiles)?;
    let ids: HashMap<&str, String> = ordered.iter().map(|t| (t.key.as_str(), new_id())).collect();
    // A savepoint (not a transaction) so callers may already be inside one.
    conn.execute_batch("SAVEPOINT replace_tasks")?;
    let result = (|| -> AppResult<()> {
    conn.execute("DELETE FROM mission_tasks WHERE mission_id = ?1", [mission_id])?;
    let now = now_ms();
    for (i, t) in ordered.iter().enumerate() {
        let deps: Vec<&String> = t.depends_on.iter().map(|k| &ids[k.as_str()]).collect();
        conn.execute(
            &format!("INSERT INTO mission_tasks ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'pending',NULL,NULL,NULL,0,NULL,NULL,?10,?10,NULL,NULL,NULL,NULL,NULL)"),
            params![
                ids[t.key.as_str()],
                mission_id,
                i as i64,
                t.key,
                t.title.trim(),
                t.description.trim(),
                t.acceptance.trim(),
                t.profile_id,
                serde_json::to_string(&deps)?,
                now
            ],
        )?;
    }
    Ok(())
    })();
    match result {
        Ok(()) => conn.execute_batch("RELEASE replace_tasks")?,
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK TO replace_tasks; RELEASE replace_tasks");
            return Err(e);
        }
    }
    list(conn, mission_id)
}

// ── Planner output ────────────────────────────────────────────────────────

/// Resolves a planner role/name to a profile id (case-insensitive), falling
/// back to the backend engineer and then the first profile.
fn resolve_profile(role: &str, profiles: &[Profile]) -> Option<String> {
    let r = role.trim().to_lowercase();
    profiles
        .iter()
        .find(|p| p.role.to_lowercase() == r || p.name.to_lowercase() == r || p.id.to_lowercase() == r)
        .or_else(|| profiles.iter().find(|p| p.id == "builtin:backend"))
        .or_else(|| profiles.first())
        .map(|p| p.id.clone())
}

/// Extracts the plan JSON from the planner's final answer.
pub fn parse_plan(text: &str, profiles: &[Profile]) -> AppResult<Vec<TaskInput>> {
    let body = match (text.find(PLAN_OPEN), text.rfind(PLAN_CLOSE)) {
        (Some(a), Some(b)) if b > a => &text[a + PLAN_OPEN.len()..b],
        _ => text,
    };
    let body = body.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    let start = body.find('{').ok_or_else(|| AppError::Invalid("o planejador não retornou um JSON de plano".into()))?;
    let end = body.rfind('}').ok_or_else(|| AppError::Invalid("JSON do plano incompleto".into()))?;
    let v: Value = serde_json::from_str(&body[start..=end]).map_err(|e| AppError::Invalid(format!("JSON do plano inválido: {e}")))?;
    let arr = v.get("tasks").and_then(Value::as_array).ok_or_else(|| AppError::Invalid("o plano não tem a lista \"tasks\"".into()))?;
    let str_of = |t: &Value, k: &str| t.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let inputs: Vec<TaskInput> = arr
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let key = Some(str_of(t, "key")).filter(|k| valid_key(k)).unwrap_or_else(|| format!("t{}", i + 1));
            TaskInput {
                key,
                title: str_of(t, "title"),
                description: str_of(t, "description"),
                acceptance: str_of(t, "acceptance"),
                profile_id: resolve_profile(&str_of(t, "profile"), profiles),
                depends_on: t
                    .get("depends_on")
                    .and_then(Value::as_array)
                    .map(|d| d.iter().filter_map(Value::as_str).map(str::to_string).collect())
                    .unwrap_or_default(),
            }
        })
        .collect();
    validate(&inputs, profiles)
}

pub fn planner_prompt(title: &str, briefing: &str, profiles: &[Profile]) -> String {
    let roles: Vec<String> = profiles.iter().map(|p| format!("- {}: {} ({})", p.role, p.name, p.description)).collect();
    format!(
        "You are planning a software mission for a team of AI coding agents. Analyze the repository in the current directory first.\n\n\
MISSION: {title}\n\nBRIEFING:\n{briefing}\n\n\
AVAILABLE AGENT ROLES:\n{roles}\n\n\
Break the mission into 1 to {MAX_TASKS} tasks (prefer 2-8). Rules:\n\
- Each task is executed by ONE agent, alone, in its OWN git worktree branched from the current code plus the branches of its dependencies.\n\
- Tasks that would edit the same files must NOT run in parallel: add a dependency between them.\n\
- `depends_on` may only reference keys of tasks listed EARLIER.\n\
- `description` must be complete, self-contained instructions for the agent (files, approach, constraints).\n\
- `acceptance` lists concrete, verifiable acceptance criteria.\n\
- `profile` is one of the role identifiers above.\n\n\
Reply with a short summary followed by the plan as JSON, exactly in this format:\n\
{PLAN_OPEN}\n{{\"tasks\": [{{\"key\": \"t1\", \"title\": \"...\", \"profile\": \"backend\", \"depends_on\": [], \"description\": \"...\", \"acceptance\": \"...\"}}]}}\n{PLAN_CLOSE}",
        roles = roles.join("\n")
    )
}

pub fn task_prompt(mission_title: &str, briefing: &str, task: &Task, deps: &[Task], can_edit: bool) -> String {
    let mut s = format!("You are one agent in a team working on the mission \"{mission_title}\".\n\nMISSION BRIEFING:\n{briefing}\n\n");
    if !deps.is_empty() {
        s.push_str("Work already completed by other agents (already present in this directory):\n");
        for d in deps {
            s.push_str(&format!("- {}\n", d.title));
        }
        s.push('\n');
    }
    s.push_str(&format!("YOUR TASK: {}\n\n{}\n", task.title, task.description));
    if !task.acceptance.trim().is_empty() {
        s.push_str(&format!("\nACCEPTANCE CRITERIA:\n{}\n", task.acceptance));
    }
    if let Some(f) = task.feedback.as_deref().filter(|f| !f.trim().is_empty()) {
        s.push_str(&format!("\nREVIEWER FEEDBACK ON A PREVIOUS ATTEMPT (address it):\n{f}\n"));
    }
    s.push_str(if can_edit {
        "\nYou work in an isolated git worktree: change only what the task needs, keep the project building, run relevant checks if available. \
Your changes will be committed automatically when you finish. End with a concise summary of what you changed and how you verified it."
    } else {
        "\nYou have read-only access. Do not modify files. End with a concise report of your findings."
    });
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profiles() -> Vec<Profile> {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|c| {
            crate::agents::profiles::ensure_builtins(c, "claude")?;
            crate::agents::profiles::list(c)
        })
        .unwrap()
    }

    fn t(key: &str, deps: &[&str]) -> TaskInput {
        TaskInput {
            key: key.into(),
            title: format!("Task {key}"),
            description: "do it".into(),
            acceptance: "".into(),
            profile_id: Some("builtin:backend".into()),
            depends_on: deps.iter().map(|d| d.to_string()).collect(),
        }
    }

    #[test]
    fn validates_dag_and_orders_topologically() {
        let p = profiles();
        let ordered = validate(&[t("b", &["a"]), t("a", &[]), t("c", &["a", "b"])], &p).unwrap();
        assert_eq!(ordered.iter().map(|x| x.key.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        assert!(validate(&[t("a", &["b"]), t("b", &["a"])], &p).unwrap_err().to_string().contains("circulares"));
        assert!(validate(&[t("a", &["zzz"])], &p).is_err());
        assert!(validate(&[t("a", &["a"])], &p).is_err());
        assert!(validate(&[t("a", &[]), t("a", &[])], &p).is_err());
        assert!(validate(&[], &p).is_err());
        let mut bad = t("a", &[]);
        bad.profile_id = Some("nope".into());
        assert!(validate(&[bad], &p).is_err());
    }

    #[test]
    fn parses_planner_output_with_markers_fences_and_roles() {
        let p = profiles();
        let text = "Here is the plan.\n<forge-plan>\n```json\n{\"tasks\":[\
{\"key\":\"api\",\"title\":\"API\",\"profile\":\"backend\",\"depends_on\":[],\"description\":\"Build API\",\"acceptance\":\"tests pass\"},\
{\"key\":\"ui\",\"title\":\"UI\",\"profile\":\"Frontend Engineer\",\"depends_on\":[\"api\"],\"description\":\"Build UI\"},\
{\"title\":\"Review\",\"profile\":\"unknown-role\",\"depends_on\":[\"ui\"],\"description\":\"Review all\"}]}\n```\n</forge-plan>";
        let plan = parse_plan(text, &p).unwrap();
        assert_eq!(plan.len(), 3);
        assert_eq!(plan[1].profile_id.as_deref(), Some("builtin:frontend"));
        assert_eq!(plan[2].key, "t3", "missing keys are generated");
        assert_eq!(plan[2].profile_id.as_deref(), Some("builtin:backend"), "unknown roles fall back");
        assert!(parse_plan("no json here", &p).is_err());
        assert!(parse_plan("<forge-plan>{\"tasks\": []}</forge-plan>", &p).is_err());
    }

    #[test]
    fn replace_maps_dependency_keys_to_ids() {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|c| {
            crate::agents::profiles::ensure_builtins(c, "claude")?;
            let p = crate::agents::profiles::list(c)?;
            let m = crate::db::missions::create(
                c,
                &crate::db::missions::NewMission { workspace_id: None, title: "m".into(), briefing: "b".into(), mode: "squad".into() },
            )?;
            let tasks = replace(c, &m.id, &[t("b", &["a"]), t("a", &[])], &p)?;
            assert_eq!(tasks[0].key, "a");
            assert_eq!(tasks[1].depends_on, vec![tasks[0].id.clone()]);
            assert!(tasks.iter().all(|t| t.status == "pending"));
            event(c, &m.id, None, "info", "token sk-proj-abcdefghijklmnopqrstuvwxyz1234")?;
            assert!(events(c, &m.id, 10)?[0].message.contains("REDACTED"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn prompts_contain_contract() {
        let p = profiles();
        let pp = planner_prompt("SaaS", "auth + billing", &p);
        assert!(pp.contains(PLAN_OPEN) && pp.contains("security") && pp.contains("auth + billing"));
    }
}
