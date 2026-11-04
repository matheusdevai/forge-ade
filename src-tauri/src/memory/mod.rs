//! OverMemory (M14): shared memory with explicit scopes.
//!
//! - Scopes: global · workspace · mission · agent.
//! - Isolation: a run only sees global items, items of ITS workspace, of its
//!   mission and of its agent profile (agent items are bound to a workspace
//!   unless the user explicitly made them cross-project).
//! - Agents may *suggest* memories (`<forge-memory>`); suggestions stay
//!   inactive until the user approves them. Secrets are never stored.

use std::sync::OnceLock;

use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::redact::redact_secrets;

pub const SCOPES: [&str; 4] = ["global", "workspace", "mission", "agent"];
pub const KINDS: [&str; 7] = ["decision", "convention", "context", "history", "error", "preference", "doc"];
pub const MAX_TITLE: usize = 160;
pub const MAX_BODY: usize = 20_000;
pub const MAX_TAGS: usize = 12;
/// Size budget of the `<memory>` block injected into a run.
pub const MAX_INJECTED: usize = 24 * 1024;
pub const MAX_SUGGESTIONS_PER_RUN: usize = 3;
const MAX_SUGGESTION_BODY: usize = 4000;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    pub id: String,
    pub scope: String,
    pub workspace_id: Option<String>,
    pub mission_id: Option<String>,
    pub profile_id: Option<String>,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub pinned: bool,
    pub status: String,
    pub source: String,
    pub source_run_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MemoryInput {
    pub scope: String,
    pub workspace_id: Option<String>,
    pub mission_id: Option<String>,
    pub profile_id: Option<String>,
    pub kind: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MemoryFilter {
    pub scope: Option<String>,
    pub workspace_id: Option<String>,
    pub mission_id: Option<String>,
    pub profile_id: Option<String>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub query: Option<String>,
    pub limit: Option<i64>,
}

const COLS: &str = "id, scope, workspace_id, mission_id, profile_id, kind, title, body, tags, pinned, status, source, source_run_id, created_at, updated_at";

fn map(r: &Row) -> rusqlite::Result<Memory> {
    Ok(Memory {
        id: r.get(0)?,
        scope: r.get(1)?,
        workspace_id: r.get(2)?,
        mission_id: r.get(3)?,
        profile_id: r.get(4)?,
        kind: r.get(5)?,
        title: r.get(6)?,
        body: r.get(7)?,
        tags: serde_json::from_str(&r.get::<_, String>(8)?).unwrap_or_default(),
        pinned: r.get::<_, i64>(9)? != 0,
        status: r.get(10)?,
        source: r.get(11)?,
        source_run_id: r.get(12)?,
        created_at: r.get(13)?,
        updated_at: r.get(14)?,
    })
}

fn nonempty(v: &Option<String>) -> Option<String> {
    v.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Validates the input and resolves the scope target. Returns the normalized
/// input (mission items inherit their mission's workspace).
fn normalize(conn: &Connection, input: &MemoryInput) -> AppResult<MemoryInput> {
    if !SCOPES.contains(&input.scope.as_str()) {
        return Err(AppError::Invalid(format!("escopo inválido: {}", input.scope)));
    }
    if !KINDS.contains(&input.kind.as_str()) {
        return Err(AppError::Invalid(format!("tipo inválido: {}", input.kind)));
    }
    let title = input.title.trim().to_string();
    let body = input.body.trim().to_string();
    if title.is_empty() {
        return Err(AppError::Invalid("informe um título".into()));
    }
    if title.chars().count() > MAX_TITLE {
        return Err(AppError::Invalid(format!("título com mais de {MAX_TITLE} caracteres")));
    }
    if body.is_empty() {
        return Err(AppError::Invalid("o conteúdo não pode ficar vazio".into()));
    }
    if body.len() > MAX_BODY {
        return Err(AppError::Invalid(format!("conteúdo com mais de {} KB", MAX_BODY / 1000)));
    }
    if redact_secrets(&title) != title || redact_secrets(&body) != body {
        return Err(AppError::Forbidden("o conteúdo parece conter um segredo ou credencial — a memória nunca guarda segredos".into()));
    }
    let mut tags: Vec<String> = Vec::new();
    for t in &input.tags {
        let t = t.trim().trim_start_matches('#').to_lowercase();
        if t.is_empty() || tags.contains(&t) {
            continue;
        }
        if t.chars().count() > 40 {
            return Err(AppError::Invalid("tag com mais de 40 caracteres".into()));
        }
        tags.push(t);
    }
    if tags.len() > MAX_TAGS {
        return Err(AppError::Invalid(format!("no máximo {MAX_TAGS} tags")));
    }
    let ws = nonempty(&input.workspace_id);
    let mission = nonempty(&input.mission_id);
    let profile = nonempty(&input.profile_id);
    let exists = |table: &str, id: &str| -> AppResult<bool> {
        Ok(conn.query_row(&format!("SELECT 1 FROM {table} WHERE id = ?1"), [id], |_| Ok(())).optional()?.is_some())
    };
    let (workspace_id, mission_id, profile_id) = match input.scope.as_str() {
        "global" => (None, None, None),
        "workspace" => {
            let w = ws.ok_or_else(|| AppError::Invalid("escolha o workspace".into()))?;
            if !exists("workspaces", &w)? {
                return Err(AppError::NotFound("workspace não encontrado".into()));
            }
            (Some(w), None, None)
        }
        "mission" => {
            let m = mission.ok_or_else(|| AppError::Invalid("escolha a missão".into()))?;
            let mws: Option<Option<String>> =
                conn.query_row("SELECT workspace_id FROM missions WHERE id = ?1", [&m], |r| r.get(0)).optional()?;
            let mws = mws.ok_or_else(|| AppError::NotFound("missão não encontrada".into()))?;
            (mws, Some(m), None)
        }
        _ => {
            let p = profile.ok_or_else(|| AppError::Invalid("escolha o agente".into()))?;
            if !exists("agent_profiles", &p)? {
                return Err(AppError::NotFound("agente não encontrado".into()));
            }
            if let Some(w) = &ws {
                if !exists("workspaces", w)? {
                    return Err(AppError::NotFound("workspace não encontrado".into()));
                }
            }
            (ws, None, Some(p))
        }
    };
    Ok(MemoryInput { scope: input.scope.clone(), workspace_id, mission_id, profile_id, kind: input.kind.clone(), title, body, tags, pinned: input.pinned })
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Memory> {
    conn.query_row(&format!("SELECT {COLS} FROM memories WHERE id = ?1"), [id], map)
        .optional()?
        .ok_or_else(|| AppError::NotFound("memória não encontrada".into()))
}

fn insert(conn: &Connection, i: &MemoryInput, status: &str, source: &str, run_id: Option<&str>) -> AppResult<Memory> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_ms();
    conn.execute(
        &format!("INSERT INTO memories ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?14)"),
        params![
            id, i.scope, i.workspace_id, i.mission_id, i.profile_id, i.kind, i.title, i.body,
            serde_json::to_string(&i.tags)?, i.pinned as i64, status, source, run_id, now
        ],
    )?;
    get(conn, &id)
}

pub fn create(conn: &Connection, input: &MemoryInput) -> AppResult<Memory> {
    let i = normalize(conn, input)?;
    insert(conn, &i, "active", "manual", None)
}

pub fn update(conn: &Connection, id: &str, input: &MemoryInput) -> AppResult<Memory> {
    get(conn, id)?;
    let i = normalize(conn, input)?;
    conn.execute(
        "UPDATE memories SET scope=?2, workspace_id=?3, mission_id=?4, profile_id=?5, kind=?6, title=?7, body=?8, tags=?9, pinned=?10, updated_at=?11 WHERE id=?1",
        params![id, i.scope, i.workspace_id, i.mission_id, i.profile_id, i.kind, i.title, i.body, serde_json::to_string(&i.tags)?, i.pinned as i64, now_ms()],
    )?;
    get(conn, id)
}

/// active ↔ archived, or suggested → active (approval).
pub fn set_status(conn: &Connection, id: &str, status: &str) -> AppResult<Memory> {
    if !matches!(status, "active" | "archived") {
        return Err(AppError::Invalid(format!("status inválido: {status}")));
    }
    get(conn, id)?;
    conn.execute("UPDATE memories SET status=?2, updated_at=?3 WHERE id=?1", params![id, status, now_ms()])?;
    get(conn, id)
}

pub fn set_pinned(conn: &Connection, id: &str, pinned: bool) -> AppResult<Memory> {
    get(conn, id)?;
    conn.execute("UPDATE memories SET pinned=?2, updated_at=?3 WHERE id=?1", params![id, pinned as i64, now_ms()])?;
    get(conn, id)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<Memory> {
    let m = get(conn, id)?;
    conn.execute("DELETE FROM memories WHERE id = ?1", [id])?;
    Ok(m)
}

pub fn list(conn: &Connection, f: &MemoryFilter) -> AppResult<Vec<Memory>> {
    let mut sql = format!("SELECT {COLS} FROM memories WHERE 1=1");
    let mut args: Vec<String> = Vec::new();
    let mut push = |sql: &mut String, cond: &str, v: String| {
        args.push(v);
        sql.push_str(&format!(" AND {cond} ?{}", args.len()));
    };
    if let Some(v) = nonempty(&f.scope) {
        push(&mut sql, "scope =", v);
    }
    if let Some(v) = nonempty(&f.workspace_id) {
        push(&mut sql, "workspace_id =", v);
    }
    if let Some(v) = nonempty(&f.mission_id) {
        push(&mut sql, "mission_id =", v);
    }
    if let Some(v) = nonempty(&f.profile_id) {
        push(&mut sql, "profile_id =", v);
    }
    if let Some(v) = nonempty(&f.kind) {
        push(&mut sql, "kind =", v);
    }
    if let Some(v) = nonempty(&f.status) {
        push(&mut sql, "status =", v);
    }
    if let Some(q) = nonempty(&f.query) {
        let like = format!("%{}%", q.to_lowercase().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        args.push(like);
        let n = args.len();
        sql.push_str(&format!(" AND (lower(title) LIKE ?{n} ESCAPE '\\' OR lower(body) LIKE ?{n} ESCAPE '\\' OR lower(tags) LIKE ?{n} ESCAPE '\\')"));
    }
    sql.push_str(&format!(" ORDER BY pinned DESC, updated_at DESC LIMIT {}", f.limit.unwrap_or(500).clamp(1, 2000)));
    let mut st = conn.prepare(&sql)?;
    let rows = st.query_map(rusqlite::params_from_iter(args.iter()), map)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCounts {
    pub active: i64,
    pub suggested: i64,
    pub archived: i64,
}

pub fn counts(conn: &Connection, workspace_id: Option<&str>) -> AppResult<MemoryCounts> {
    let mut c = MemoryCounts::default();
    let mut st = conn.prepare("SELECT status, COUNT(*) FROM memories WHERE (?1 IS NULL OR workspace_id = ?1) GROUP BY status")?;
    for row in st.query_map([workspace_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
        let (s, n) = row?;
        match s.as_str() {
            "active" => c.active = n,
            "suggested" => c.suggested = n,
            _ => c.archived = n,
        }
    }
    Ok(c)
}

// ── Composition into a run ────────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Composed {
    pub text: String,
    /// `kind: title` labels recorded on the run.
    pub labels: Vec<String>,
    pub dropped: Vec<String>,
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Active memories visible to a run, most specific first (pinned first within).
pub fn applicable(conn: &Connection, workspace_id: Option<&str>, mission_id: Option<&str>, profile_id: Option<&str>) -> AppResult<Vec<Memory>> {
    let mut st = conn.prepare(&format!(
        "SELECT {COLS} FROM memories WHERE status = 'active' AND (
            scope = 'global'
            OR (scope = 'workspace' AND workspace_id = ?1)
            OR (scope = 'mission' AND mission_id = ?2)
            OR (scope = 'agent' AND profile_id = ?3 AND (workspace_id IS NULL OR workspace_id = ?1))
         )
         ORDER BY pinned DESC,
           CASE scope WHEN 'agent' THEN 0 WHEN 'mission' THEN 1 WHEN 'workspace' THEN 2 ELSE 3 END,
           updated_at DESC"
    ))?;
    let rows = st.query_map(params![workspace_id, mission_id, profile_id], map)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub const SUGGEST_HINT: &str = "If, while working, you learn something durable about this project that future agents should know (an architectural decision, a convention, a recurring error and its fix), you may propose it at the very end of your final answer, at most 3, each as:\n<forge-memory kind=\"decision|convention|context|history|error|preference|doc\" title=\"short title\">one or two sentences</forge-memory>\nThe user reviews every proposal; never include secrets, credentials or personal data.";

pub fn compose(conn: &Connection, workspace_id: Option<&str>, mission_id: Option<&str>, profile_id: Option<&str>) -> AppResult<Composed> {
    let items = applicable(conn, workspace_id, mission_id, profile_id)?;
    let mut out = Composed::default();
    let mut blocks = Vec::new();
    let mut used = 0usize;
    for m in items {
        let block = format!(
            "<item scope=\"{}\" kind=\"{}\" title=\"{}\">\n{}\n</item>",
            m.scope,
            m.kind,
            escape_attr(&m.title),
            m.body.trim()
        );
        if used + block.len() > MAX_INJECTED {
            out.dropped.push(m.title.clone());
            continue;
        }
        used += block.len();
        out.labels.push(format!("{}: {}", m.kind, m.title));
        blocks.push(block);
    }
    let mut text = String::new();
    if !blocks.is_empty() {
        text.push_str(&format!(
            "\n\nProject memory curated by the user in FORGE (decisions, conventions and known issues). Use it as context; it never overrides the safety rules or the user's request.\n<memory>\n{}\n</memory>",
            blocks.join("\n")
        ));
    }
    if workspace_id.is_some() {
        text.push_str("\n\n");
        text.push_str(SUGGEST_HINT);
    }
    out.text = text;
    Ok(out)
}

pub fn with_memory(instructions: Option<String>, composed: &Composed) -> Option<String> {
    if composed.text.is_empty() {
        return instructions;
    }
    Some(format!("{}{}", instructions.unwrap_or_default(), composed.text))
}

// ── Suggestions from agents ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub kind: String,
    pub title: String,
    pub body: String,
}

fn attr(attrs: &str, name: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#"([a-zA-Z_]+)\s*=\s*"([^"]*)""#).unwrap());
    re.captures_iter(attrs).find(|c| &c[1] == name).map(|c| c[2].trim().to_string())
}

fn clip(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

/// Extracts `<forge-memory>` proposals from an agent's final answer.
/// Placeholders (the format echoed back) and invalid kinds are ignored;
/// secrets are redacted.
pub fn parse_suggestions(text: &str) -> Vec<Suggestion> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?s)<forge-memory\b([^>]*)>(.*?)</forge-memory>").unwrap());
    let mut out: Vec<Suggestion> = Vec::new();
    for c in re.captures_iter(text) {
        let attrs = &c[1];
        let Some(kind) = attr(attrs, "kind").map(|k| k.to_lowercase()) else { continue };
        if !KINDS.contains(&kind.as_str()) {
            continue;
        }
        let Some(title) = attr(attrs, "title") else { continue };
        let body = c[2].trim();
        if title.is_empty() || body.is_empty() || title == "short title" || body == "one or two sentences" {
            continue;
        }
        let title = clip(&redact_secrets(&title), MAX_TITLE);
        let body = clip(&redact_secrets(body), MAX_SUGGESTION_BODY);
        if out.iter().any(|s| s.title.eq_ignore_ascii_case(&title)) {
            continue;
        }
        out.push(Suggestion { kind, title, body });
        if out.len() >= MAX_SUGGESTIONS_PER_RUN {
            break;
        }
    }
    out
}

/// Stores suggestions from a finished run as inactive `suggested` items,
/// scoped to the run's mission (if any) or workspace. Duplicate titles in the
/// same scope are skipped. Returns the created items.
pub fn record_suggestions(
    conn: &Connection,
    run_id: &str,
    workspace_id: Option<&str>,
    mission_id: Option<&str>,
    text: &str,
) -> AppResult<Vec<Memory>> {
    let Some(ws) = workspace_id.filter(|w| !w.is_empty()) else { return Ok(vec![]) };
    let ws_exists = conn.query_row("SELECT 1 FROM workspaces WHERE id = ?1", [ws], |_| Ok(())).optional()?.is_some();
    if !ws_exists {
        return Ok(vec![]);
    }
    let mission = match mission_id {
        Some(m) => conn
            .query_row("SELECT id FROM missions WHERE id = ?1 AND workspace_id = ?2", [m, ws], |r| r.get::<_, String>(0))
            .optional()?,
        None => None,
    };
    let run_ref: Option<String> = conn.query_row("SELECT id FROM agent_runs WHERE id = ?1", [run_id], |r| r.get(0)).optional()?;
    let mut created = Vec::new();
    for s in parse_suggestions(text) {
        let dup: Option<String> = conn
            .query_row(
                "SELECT id FROM memories WHERE workspace_id = ?1 AND lower(title) = lower(?2) AND status != 'archived'",
                params![ws, s.title],
                |r| r.get(0),
            )
            .optional()?;
        if dup.is_some() {
            continue;
        }
        let input = MemoryInput {
            scope: if mission.is_some() { "mission".into() } else { "workspace".into() },
            workspace_id: Some(ws.to_string()),
            mission_id: mission.clone(),
            profile_id: None,
            kind: s.kind,
            title: s.title,
            body: s.body,
            tags: vec![],
            pinned: false,
        };
        created.push(insert(conn, &input, "suggested", "agent", run_ref.as_deref())?);
    }
    Ok(created)
}

/// Approves a suggestion (it becomes active and is injected from now on).
pub fn approve(conn: &Connection, id: &str) -> AppResult<Memory> {
    let m = get(conn, id)?;
    if m.status != "suggested" {
        return Err(AppError::Conflict("essa memória não é uma sugestão pendente".into()));
    }
    set_status(conn, id, "active")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn setup() -> Db {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            crate::agents::profiles::ensure_builtins(c, "claude")?;
            for (id, path) in [("w1", "/tmp/forge-w1"), ("w2", "/tmp/forge-w2")] {
                c.execute("INSERT INTO workspaces (id, name, path, pinned, created_at) VALUES (?1, ?1, ?2, 0, 0)", [id, path])?;
            }
            c.execute("INSERT INTO missions (id, workspace_id, title, briefing, mode, created_at, updated_at) VALUES ('m1','w1','M','b','free',0,0)", [])?;
            Ok(())
        })
        .unwrap();
        db
    }

    fn input(scope: &str, ws: Option<&str>, title: &str) -> MemoryInput {
        MemoryInput {
            scope: scope.into(),
            workspace_id: ws.map(String::from),
            kind: "convention".into(),
            title: title.into(),
            body: format!("body of {title}"),
            ..Default::default()
        }
    }

    #[test]
    fn validates_scope_targets_kinds_and_secrets() {
        let db = setup();
        db.with(|c| {
            assert!(create(c, &input("workspace", None, "x")).is_err());
            assert!(create(c, &input("workspace", Some("nope"), "x")).is_err());
            assert!(create(c, &MemoryInput { kind: "rumor".into(), ..input("global", None, "x") }).is_err());
            assert!(create(c, &input("bogus", None, "x")).is_err());
            assert!(create(c, &MemoryInput { body: "  ".into(), ..input("global", None, "x") }).is_err());
            let secret = MemoryInput { body: "use key sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789".into(), ..input("global", None, "k") };
            assert!(matches!(create(c, &secret), Err(AppError::Forbidden(_))));
            // Global ignores stray targets; mission inherits the mission's workspace.
            let g = create(c, &input("global", Some("w1"), "g"))?;
            assert_eq!(g.workspace_id, None);
            let m = create(c, &MemoryInput { mission_id: Some("m1".into()), ..input("mission", None, "m") })?;
            assert_eq!(m.workspace_id.as_deref(), Some("w1"));
            let t = create(c, &MemoryInput { tags: vec!["#API".into(), "api".into(), " db ".into()], ..input("workspace", Some("w1"), "t") })?;
            assert_eq!(t.tags, vec!["api", "db"]);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn runs_never_see_other_projects() {
        let db = setup();
        db.with(|c| {
            create(c, &input("global", None, "global rule"))?;
            create(c, &input("workspace", Some("w1"), "w1 only"))?;
            create(c, &input("workspace", Some("w2"), "w2 only"))?;
            create(c, &MemoryInput { mission_id: Some("m1".into()), ..input("mission", None, "mission note") })?;
            create(c, &MemoryInput { profile_id: Some("builtin:architect".into()), ..input("agent", Some("w2"), "architect in w2") })?;
            create(c, &MemoryInput { profile_id: Some("builtin:architect".into()), ..input("agent", None, "architect everywhere") })?;
            let archived = create(c, &input("workspace", Some("w1"), "archived"))?;
            set_status(c, &archived.id, "archived")?;
            let titles = |v: Vec<Memory>| v.into_iter().map(|m| m.title).collect::<Vec<_>>();
            let w1 = titles(applicable(c, Some("w1"), Some("m1"), Some("builtin:architect"))?);
            assert_eq!(w1, ["architect everywhere", "mission note", "w1 only", "global rule"]);
            let w2 = titles(applicable(c, Some("w2"), None, None)?);
            assert_eq!(w2, ["w2 only", "global rule"]);
            let w2a = titles(applicable(c, Some("w2"), None, Some("builtin:architect"))?);
            assert!(w2a.contains(&"architect in w2".to_string()) && !w2a.contains(&"w1 only".to_string()));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn compose_orders_pins_respects_budget_and_escapes() {
        let db = setup();
        db.with(|c| {
            let a = create(c, &input("workspace", Some("w1"), "a \"quoted\" <t>"))?;
            create(c, &input("workspace", Some("w1"), "b"))?;
            set_pinned(c, &a.id, true)?;
            let big = "x".repeat(MAX_BODY - 10);
            for i in 0..3 {
                create(c, &MemoryInput { body: big.clone(), ..input("global", None, &format!("big{i}")) })?;
            }
            let comp = compose(c, Some("w1"), None, None)?;
            assert!(comp.text.contains("<memory>"));
            assert!(comp.text.contains("title=\"a &quot;quoted&quot; &lt;t&gt;\""));
            assert_eq!(comp.labels[0], "convention: a \"quoted\" <t>");
            assert!(!comp.dropped.is_empty());
            assert!(comp.text.len() < MAX_INJECTED + 2000);
            assert!(comp.text.contains("<forge-memory"));
            let none = compose(c, None, None, None)?;
            assert!(none.text.contains("<memory>") && !none.text.contains("forge-memory kind"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn parses_suggestions_and_ignores_placeholders() {
        let text = r#"Done.
<forge-memory kind="convention" title="Use pnpm">The repo uses pnpm workspaces; never run npm install.</forge-memory>
<forge-memory kind="decision|convention|context|history|error|preference|doc" title="short title">one or two sentences</forge-memory>
<forge-memory kind="rumor" title="x">y</forge-memory>
<forge-memory kind="ERROR" title="Flaky test">auth.spec fails when run in parallel; use --runInBand. token=ghp_abcdefghijklmnopqrstuvwxyz0123456789</forge-memory>
<forge-memory kind="convention" title="use PNPM">dup</forge-memory>
<forge-memory kind="doc" title="a">1</forge-memory><forge-memory kind="doc" title="b">2</forge-memory>"#;
        let s = parse_suggestions(text);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].title, "Use pnpm");
        assert_eq!(s[1].kind, "error");
        assert!(!s[1].body.contains("ghp_abcdef"), "{}", s[1].body);
        assert_eq!(s[2].title, "a");
        assert!(parse_suggestions("no tags here").is_empty());
    }

    #[test]
    fn suggestions_are_inactive_until_approved_and_deduplicated() {
        let db = setup();
        db.with(|c| {
            let text = "<forge-memory kind=\"decision\" title=\"Use SQLite\">Local-first storage.</forge-memory>";
            let made = record_suggestions(c, "no-run", Some("w1"), None, text)?;
            assert_eq!(made.len(), 1);
            assert_eq!((made[0].status.as_str(), made[0].source.as_str(), made[0].scope.as_str()), ("suggested", "agent", "workspace"));
            assert!(applicable(c, Some("w1"), None, None)?.is_empty(), "suggestions must not be injected");
            assert!(record_suggestions(c, "no-run", Some("w1"), None, text)?.is_empty(), "duplicate");
            // Mission runs scope to the mission; a mission of another workspace is ignored.
            let m = record_suggestions(c, "r", Some("w1"), Some("m1"), "<forge-memory kind=\"error\" title=\"E\">fix</forge-memory>")?;
            assert_eq!((m[0].scope.as_str(), m[0].mission_id.as_deref()), ("mission", Some("m1")));
            let other = record_suggestions(c, "r", Some("w2"), Some("m1"), "<forge-memory kind=\"error\" title=\"E2\">fix</forge-memory>")?;
            assert_eq!(other[0].scope, "workspace");
            assert!(record_suggestions(c, "r", None, None, text)?.is_empty());
            let ok = approve(c, &made[0].id)?;
            assert_eq!(ok.status, "active");
            assert!(approve(c, &made[0].id).is_err());
            assert_eq!(applicable(c, Some("w1"), None, None)?.len(), 1);
            let counts = counts(c, Some("w1"))?;
            assert_eq!((counts.active, counts.suggested), (1, 1));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn list_filters_and_search_and_cascade() {
        let db = setup();
        db.with(|c| {
            create(c, &MemoryInput { tags: vec!["auth".into()], ..input("workspace", Some("w1"), "Login flow") })?;
            create(c, &MemoryInput { kind: "error".into(), ..input("workspace", Some("w1"), "100% CPU") })?;
            create(c, &input("workspace", Some("w2"), "Other"))?;
            let f = |q: &str| MemoryFilter { query: Some(q.into()), ..Default::default() };
            assert_eq!(list(c, &f("login"))?.len(), 1);
            assert_eq!(list(c, &f("AUTH"))?.len(), 1);
            assert_eq!(list(c, &f("100%"))?.len(), 1);
            assert_eq!(list(c, &f("_"))?.len(), 0);
            assert_eq!(list(c, &MemoryFilter { workspace_id: Some("w1".into()), kind: Some("error".into()), ..Default::default() })?.len(), 1);
            c.execute("DELETE FROM workspaces WHERE id = 'w1'", [])?;
            assert_eq!(list(c, &MemoryFilter::default())?.len(), 1, "workspace memories go with the workspace");
            Ok(())
        })
        .unwrap();
    }
}
