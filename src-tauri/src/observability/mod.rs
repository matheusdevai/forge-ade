//! Observability (M19): metrics, live processes, unified timeline (History /
//! Redline), audit trail and run replay.
//!
//! Real data only: tokens and costs are summed exclusively from runs where
//! the CLI reported them, and every aggregate says how many runs reported vs
//! did not. Text leaving this module is passed through `redact_secrets` again.

use std::collections::HashMap;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AppResult;
use crate::redact::redact_secrets;

pub const DAY_MS: i64 = 24 * 3600 * 1000;

// ── Metrics ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Reported<T> {
    /// Sum over runs that reported the value (None when none did).
    pub value: Option<T>,
    pub reported_runs: i64,
    pub unreported_runs: i64,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderMetrics {
    pub provider_id: String,
    pub model: Option<String>,
    pub runs: i64,
    pub completed: i64,
    pub failed: i64,
    pub cancelled: i64,
    pub avg_duration_ms: Option<i64>,
    pub input_tokens: Reported<i64>,
    pub output_tokens: Reported<i64>,
    pub cost_usd: Reported<f64>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DayPoint {
    /// Local day start (ms).
    pub day: i64,
    pub runs: i64,
    pub failed: i64,
    pub cost_usd: Option<f64>,
    pub cost_runs: i64,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    pub since: i64,
    pub runs: i64,
    pub running: i64,
    pub completed: i64,
    pub failed: i64,
    pub cancelled: i64,
    pub p50_duration_ms: Option<i64>,
    pub p95_duration_ms: Option<i64>,
    pub input_tokens: Reported<i64>,
    pub output_tokens: Reported<i64>,
    pub cache_read_tokens: Reported<i64>,
    pub cost_usd: Reported<f64>,
    pub providers: Vec<ProviderMetrics>,
    pub missions: HashMap<String, i64>,
    pub tasks: HashMap<String, i64>,
    pub routine_runs: HashMap<String, i64>,
    pub routines_disabled: i64,
    pub media: HashMap<String, i64>,
    pub tool_uses: i64,
    pub days: Vec<DayPoint>,
}

fn reported_i64(conn: &Connection, col: &str, where_: &str, args: &[&dyn rusqlite::ToSql]) -> AppResult<Reported<i64>> {
    let sql = format!("SELECT SUM({col}), COUNT({col}), COUNT(*) - COUNT({col}) FROM agent_runs WHERE {where_} AND status != 'running'");
    Ok(conn.query_row(&sql, args, |r| Ok(Reported { value: r.get(0)?, reported_runs: r.get(1)?, unreported_runs: r.get(2)? }))?)
}

fn percentile(sorted: &[i64], p: f64) -> Option<i64> {
    if sorted.is_empty() {
        return None;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted.get(idx).copied()
}

fn count_by(conn: &Connection, sql: &str, args: &[&dyn rusqlite::ToSql]) -> AppResult<HashMap<String, i64>> {
    let mut st = conn.prepare(sql)?;
    let rows = st.query_map(args, |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    let mut m = HashMap::new();
    for row in rows {
        let (k, v) = row?;
        m.insert(k, v);
    }
    Ok(m)
}

/// `day_starts`: local midnights covering the window (oldest first), so the
/// daily buckets follow the user's calendar.
pub fn metrics(conn: &Connection, since: i64, workspace_id: Option<&str>, day_starts: &[i64]) -> AppResult<Metrics> {
    let ws = workspace_id;
    let w = "started_at >= ?1 AND (?2 IS NULL OR workspace_id = ?2)";
    let a: [&dyn rusqlite::ToSql; 2] = [&since, &ws];
    let mut m = Metrics { since, ..Default::default() };
    let by_status = count_by(conn, &format!("SELECT status, COUNT(*) FROM agent_runs WHERE {w} GROUP BY status"), &a)?;
    m.running = *by_status.get("running").unwrap_or(&0);
    m.completed = *by_status.get("completed").unwrap_or(&0);
    m.failed = *by_status.get("failed").unwrap_or(&0);
    m.cancelled = *by_status.get("cancelled").unwrap_or(&0);
    m.runs = by_status.values().sum();
    let mut durations: Vec<i64> = {
        let mut st = conn.prepare(&format!("SELECT duration_ms FROM agent_runs WHERE {w} AND duration_ms IS NOT NULL"))?;
        let v = st.query_map(a, |r| r.get::<_, i64>(0))?.collect::<Result<Vec<_>, _>>()?;
        v
    };
    durations.sort_unstable();
    m.p50_duration_ms = percentile(&durations, 0.5);
    m.p95_duration_ms = percentile(&durations, 0.95);
    m.input_tokens = reported_i64(conn, "input_tokens", w, &a)?;
    m.output_tokens = reported_i64(conn, "output_tokens", w, &a)?;
    m.cache_read_tokens = reported_i64(conn, "cache_read_tokens", w, &a)?;
    m.cost_usd = conn.query_row(
        &format!("SELECT SUM(cost_usd), COUNT(cost_usd), COUNT(*) - COUNT(cost_usd) FROM agent_runs WHERE {w} AND status != 'running'"),
        a,
        |r| Ok(Reported { value: r.get(0)?, reported_runs: r.get(1)?, unreported_runs: r.get(2)? }),
    )?;
    {
        let mut st = conn.prepare(&format!(
            "SELECT provider_id, model, COUNT(*),
               SUM(status='completed'), SUM(status='failed'), SUM(status='cancelled'),
               CAST(AVG(duration_ms) AS INTEGER),
               SUM(input_tokens), COUNT(input_tokens), SUM(output_tokens), COUNT(output_tokens),
               SUM(cost_usd), COUNT(cost_usd), SUM(status != 'running')
             FROM agent_runs WHERE {w} GROUP BY provider_id, model ORDER BY COUNT(*) DESC"
        ))?;
        m.providers = st
            .query_map(a, |r| {
                let finished: i64 = r.get(13)?;
                let rep = |v, n: i64| Reported { value: v, reported_runs: n, unreported_runs: finished - n };
                Ok(ProviderMetrics {
                    provider_id: r.get(0)?,
                    model: r.get(1)?,
                    runs: r.get(2)?,
                    completed: r.get(3)?,
                    failed: r.get(4)?,
                    cancelled: r.get(5)?,
                    avg_duration_ms: r.get(6)?,
                    input_tokens: rep(r.get(7)?, r.get(8)?),
                    output_tokens: rep(r.get(9)?, r.get(10)?),
                    cost_usd: Reported { value: r.get(11)?, reported_runs: r.get(12)?, unreported_runs: finished - r.get::<_, i64>(12)? },
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
    }
    m.missions = count_by(conn, "SELECT status, COUNT(*) FROM missions WHERE updated_at >= ?1 AND (?2 IS NULL OR workspace_id = ?2) GROUP BY status", &a)?;
    m.tasks = count_by(
        conn,
        "SELECT t.status, COUNT(*) FROM mission_tasks t JOIN missions mi ON mi.id = t.mission_id
         WHERE mi.updated_at >= ?1 AND (?2 IS NULL OR mi.workspace_id = ?2) GROUP BY t.status",
        &a,
    )?;
    m.routine_runs = count_by(
        conn,
        "SELECT rr.status, COUNT(*) FROM routine_runs rr JOIN routines r ON r.id = rr.routine_id
         WHERE rr.started_at >= ?1 AND (?2 IS NULL OR r.workspace_id = ?2) GROUP BY rr.status",
        &a,
    )?;
    m.routines_disabled = conn.query_row(
        "SELECT COUNT(*) FROM routines WHERE enabled = 0 AND disabled_reason IS NOT NULL AND (?1 IS NULL OR workspace_id = ?1)",
        [ws],
        |r| r.get(0),
    )?;
    m.media = count_by(conn, "SELECT status, COUNT(*) FROM media_assets WHERE created_at >= ?1 AND (?2 IS NULL OR workspace_id = ?2) GROUP BY status", &a)?;
    m.tool_uses = conn.query_row(
        "SELECT COUNT(*) FROM agent_events e JOIN agent_runs r ON r.id = e.run_id
         WHERE e.kind = 'tool_use' AND e.created_at >= ?1 AND (?2 IS NULL OR r.workspace_id = ?2)",
        a,
        |r| r.get(0),
    )?;
    for (i, start) in day_starts.iter().enumerate() {
        let end = day_starts.get(i + 1).copied().unwrap_or(i64::MAX);
        let p = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(status='failed'),0), SUM(cost_usd), COUNT(cost_usd) FROM agent_runs
             WHERE started_at >= ?1 AND started_at < ?2 AND (?3 IS NULL OR workspace_id = ?3)",
            params![start, end, ws],
            |r| Ok(DayPoint { day: *start, runs: r.get(0)?, failed: r.get(1)?, cost_usd: r.get(2)?, cost_runs: r.get(3)? }),
        )?;
        m.days.push(p);
    }
    Ok(m)
}

// ── Timeline (History / Redline) ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TimelineItem {
    pub ts: i64,
    /// activity | agent | tool | mission | routine | media
    pub source: String,
    pub kind: String,
    /// info | success | warning | error
    pub level: String,
    pub title: String,
    pub detail: Option<String>,
    pub workspace_id: Option<String>,
    pub run_id: Option<String>,
    pub mission_id: Option<String>,
    pub routine_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TimelineQuery {
    pub query: Option<String>,
    pub sources: Option<Vec<String>>,
    pub workspace_id: Option<String>,
    pub level: Option<String>,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub limit: Option<i64>,
    /// Only audit-relevant events.
    #[serde(default)]
    pub audit: bool,
}

/// Activity kinds that matter for the security audit.
pub const AUDIT_KINDS: &[&str] = &[
    "workspace.removed",
    "voice.pause_all",
    "skill.install",
    "library.import",
    "media.key",
    "media.key.clear",
    "mcp.secret.set",
    "mcp.secret.clear",
    "git.push",
    "git.discard",
    "git.branch.delete",
    "git.merge",
    "git.worktree.remove",
    "fs.trashed",
    "mission.deleted",
    "mission.cancelled",
    "autopilot.created",
    "autopilot.deleted",
    "memory.deleted",
    "provider.verify",
];

fn clip(s: &str, n: usize) -> String {
    let s = redact_secrets(s);
    if s.chars().count() <= n {
        return s;
    }
    format!("{}…", s.chars().take(n).collect::<String>())
}

fn like(q: &str) -> String {
    format!("%{}%", q.to_lowercase().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"))
}

/// Human title for a tool call (e.g. `Bash: npm test`).
pub fn tool_title(payload: &Value) -> String {
    let name = payload.get("name").and_then(Value::as_str).unwrap_or("ferramenta");
    let input = payload.get("input").and_then(Value::as_str).unwrap_or_default();
    let parsed: Option<Value> = serde_json::from_str(input).ok();
    let arg = parsed
        .as_ref()
        .and_then(|v| ["command", "file_path", "path", "pattern", "url", "query", "prompt"].iter().find_map(|k| v.get(*k).and_then(Value::as_str)))
        .map(String::from)
        .unwrap_or_else(|| input.to_string());
    let arg = arg.lines().next().unwrap_or_default();
    if arg.is_empty() {
        name.to_string()
    } else {
        format!("{name}: {}", clip(arg, 160))
    }
}

pub fn timeline(conn: &Connection, q: &TimelineQuery) -> AppResult<Vec<TimelineItem>> {
    let since = q.since.unwrap_or(0);
    let until = q.until.unwrap_or(i64::MAX);
    let ws = q.workspace_id.as_deref().filter(|w| !w.is_empty());
    let text = q.query.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(like);
    let limit = q.limit.unwrap_or(300).clamp(1, 2000);
    let want = |s: &str| !q.audit && q.sources.as_ref().is_none_or(|v| v.iter().any(|x| x == s));
    let mut out: Vec<TimelineItem> = Vec::new();
    let tl = text.as_deref();

    // Activity log (also the audit source).
    if want("activity") || q.audit {
        let audit_list = AUDIT_KINDS.iter().map(|k| format!("'{k}'")).collect::<Vec<_>>().join(",");
        let audit_cond = if q.audit { format!("AND (kind IN ({audit_list}) OR (kind = 'agent.run' AND message LIKE '%(full)%'))") } else { String::new() };
        let mut st = conn.prepare(&format!(
            "SELECT created_at, kind, message, workspace_id, mission_id FROM activity_events
             WHERE created_at >= ?1 AND created_at <= ?2 AND (?3 IS NULL OR workspace_id = ?3)
               AND (?4 IS NULL OR lower(message) LIKE ?4 ESCAPE '\\' OR lower(kind) LIKE ?4 ESCAPE '\\') {audit_cond}
             ORDER BY created_at DESC LIMIT ?5"
        ))?;
        let rows = st.query_map(params![since, until, ws, tl, limit], |r| {
            let kind: String = r.get(1)?;
            let msg: String = r.get(2)?;
            let level = if kind.contains("deleted") || kind.contains("removed") || kind.contains("discard") || kind.contains("clear") || kind.contains("trashed") {
                "warning"
            } else if msg.contains("falh") {
                "error"
            } else {
                "info"
            };
            Ok(TimelineItem {
                ts: r.get(0)?,
                source: "activity".into(),
                kind,
                level: level.into(),
                title: clip(&msg, 300),
                detail: None,
                workspace_id: r.get(3)?,
                run_id: None,
                mission_id: r.get(4)?,
                routine_id: None,
            })
        })?;
        out.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }

    // Agent runs: prompt, result, error.
    if want("agent") || q.audit {
        let audit_cond = if q.audit { "AND permission = 'full'" } else { "" };
        let mut st = conn.prepare(&format!(
            "SELECT started_at, id, provider_id, model, permission, status, prompt, result_text, error, workspace_id, duration_ms FROM agent_runs
             WHERE started_at >= ?1 AND started_at <= ?2 AND (?3 IS NULL OR workspace_id = ?3)
               AND (?4 IS NULL OR lower(prompt) LIKE ?4 ESCAPE '\\' OR lower(COALESCE(result_text,'')) LIKE ?4 ESCAPE '\\' OR lower(COALESCE(error,'')) LIKE ?4 ESCAPE '\\') {audit_cond}
             ORDER BY started_at DESC LIMIT ?5"
        ))?;
        let rows = st.query_map(params![since, until, ws, tl, limit], |r| {
            let status: String = r.get(5)?;
            let provider: String = r.get(2)?;
            let model: Option<String> = r.get(3)?;
            let permission: String = r.get(4)?;
            let prompt: String = r.get(6)?;
            let result: Option<String> = r.get(7)?;
            let error: Option<String> = r.get(8)?;
            let dur: Option<i64> = r.get(10)?;
            let level = match status.as_str() {
                "failed" => "error",
                "cancelled" => "warning",
                "completed" => "success",
                _ => "info",
            };
            let head = format!(
                "{provider}{} · {permission} · {status}{}",
                model.map(|m| format!(" ({m})")).unwrap_or_default(),
                dur.map(|d| format!(" · {:.1}s", d as f64 / 1000.0)).unwrap_or_default()
            );
            Ok(TimelineItem {
                ts: r.get(0)?,
                source: "agent".into(),
                kind: format!("agent.{status}"),
                level: level.into(),
                title: clip(&prompt, 200),
                detail: Some(match (error, result) {
                    (Some(e), _) => format!("{head}\nErro: {}", clip(&e, 600)),
                    (None, Some(t)) => format!("{head}\n{}", clip(&t, 600)),
                    _ => head,
                }),
                workspace_id: r.get(9)?,
                run_id: Some(r.get(1)?),
                mission_id: None,
                routine_id: None,
            })
        })?;
        out.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }

    // Tool calls (commands, file edits…) made by agents.
    if want("tool") {
        let mut st = conn.prepare(
            "SELECT e.created_at, e.run_id, e.payload, r.workspace_id FROM agent_events e JOIN agent_runs r ON r.id = e.run_id
             WHERE e.kind = 'tool_use' AND e.created_at >= ?1 AND e.created_at <= ?2 AND (?3 IS NULL OR r.workspace_id = ?3)
               AND (?4 IS NULL OR lower(e.payload) LIKE ?4 ESCAPE '\\')
             ORDER BY e.created_at DESC LIMIT ?5",
        )?;
        let rows = st.query_map(params![since, until, ws, tl, limit], |r| {
            let payload: Value = serde_json::from_str(&r.get::<_, String>(2)?).unwrap_or(Value::Null);
            Ok(TimelineItem {
                ts: r.get(0)?,
                source: "tool".into(),
                kind: payload.get("name").and_then(Value::as_str).unwrap_or("tool").to_string(),
                level: "info".into(),
                title: tool_title(&payload),
                detail: None,
                workspace_id: r.get(3)?,
                run_id: Some(r.get(1)?),
                mission_id: None,
                routine_id: None,
            })
        })?;
        out.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }

    if want("mission") {
        let mut st = conn.prepare(
            "SELECT e.created_at, e.kind, e.message, e.mission_id, m.workspace_id, m.title FROM mission_events e JOIN missions m ON m.id = e.mission_id
             WHERE e.created_at >= ?1 AND e.created_at <= ?2 AND (?3 IS NULL OR m.workspace_id = ?3)
               AND (?4 IS NULL OR lower(e.message) LIKE ?4 ESCAPE '\\' OR lower(m.title) LIKE ?4 ESCAPE '\\')
             ORDER BY e.created_at DESC LIMIT ?5",
        )?;
        let rows = st.query_map(params![since, until, ws, tl, limit], |r| {
            let kind: String = r.get(1)?;
            let level = match kind.as_str() {
                "error" | "fail" | "failed" | "escalate" => "error",
                "complete" | "approve" | "integrate" => "success",
                "pause" | "reject" | "cancel" => "warning",
                _ => "info",
            };
            Ok(TimelineItem {
                ts: r.get(0)?,
                source: "mission".into(),
                kind,
                level: level.into(),
                title: clip(&r.get::<_, String>(2)?, 300),
                detail: Some(format!("Missão: {}", r.get::<_, String>(5)?)),
                workspace_id: r.get(4)?,
                run_id: None,
                mission_id: Some(r.get(3)?),
                routine_id: None,
            })
        })?;
        out.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }

    if want("routine") {
        let mut st = conn.prepare(
            "SELECT rr.started_at, rr.status, rr.trigger, rr.error, rr.output, r.name, r.workspace_id, r.id, rr.agent_run_id, rr.mission_id
             FROM routine_runs rr JOIN routines r ON r.id = rr.routine_id
             WHERE rr.started_at >= ?1 AND rr.started_at <= ?2 AND (?3 IS NULL OR r.workspace_id = ?3)
               AND (?4 IS NULL OR lower(r.name) LIKE ?4 ESCAPE '\\' OR lower(COALESCE(rr.output,'')) LIKE ?4 ESCAPE '\\' OR lower(COALESCE(rr.error,'')) LIKE ?4 ESCAPE '\\')
             ORDER BY rr.started_at DESC LIMIT ?5",
        )?;
        let rows = st.query_map(params![since, until, ws, tl, limit], |r| {
            let status: String = r.get(1)?;
            let level = match status.as_str() {
                "failed" => "error",
                "skipped" | "cancelled" => "warning",
                "succeeded" => "success",
                _ => "info",
            };
            let err: Option<String> = r.get(3)?;
            let output: Option<String> = r.get(4)?;
            Ok(TimelineItem {
                ts: r.get(0)?,
                source: "routine".into(),
                kind: format!("routine.{status}"),
                level: level.into(),
                title: format!("Rotina “{}” · {} · {status}", r.get::<_, String>(5)?, r.get::<_, String>(2)?),
                detail: err.or(output).map(|t| clip(&t, 600)),
                workspace_id: r.get(6)?,
                run_id: r.get(8)?,
                mission_id: r.get(9)?,
                routine_id: Some(r.get(7)?),
            })
        })?;
        out.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }

    if want("media") {
        let mut st = conn.prepare(
            "SELECT created_at, op, status, provider, model, prompt, error, workspace_id FROM media_assets
             WHERE created_at >= ?1 AND created_at <= ?2 AND (?3 IS NULL OR workspace_id = ?3)
               AND (?4 IS NULL OR lower(prompt) LIKE ?4 ESCAPE '\\' OR lower(COALESCE(result_text,'')) LIKE ?4 ESCAPE '\\')
             ORDER BY created_at DESC LIMIT ?5",
        )?;
        let rows = st.query_map(params![since, until, ws, tl, limit], |r| {
            let status: String = r.get(2)?;
            let model: Option<String> = r.get(4)?;
            Ok(TimelineItem {
                ts: r.get(0)?,
                source: "media".into(),
                kind: r.get(1)?,
                level: if status == "failed" { "error" } else if status == "done" { "success" } else { "info" }.into(),
                title: format!("{} · {}{}", r.get::<_, String>(1)?, r.get::<_, String>(3)?, model.map(|m| format!(" ({m})")).unwrap_or_default()),
                detail: r.get::<_, Option<String>>(6)?.or(Some(r.get::<_, String>(5)?)).map(|t| clip(&t, 400)),
                workspace_id: r.get(7)?,
                run_id: None,
                mission_id: None,
                routine_id: None,
            })
        })?;
        out.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }

    if let Some(l) = q.level.as_deref().filter(|l| !l.is_empty()) {
        out.retain(|i| i.level == l);
    }
    out.sort_by_key(|i| std::cmp::Reverse(i.ts));
    out.truncate(limit as usize);
    Ok(out)
}

// ── Replay ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReplayStep {
    pub seq: i64,
    /// ms since the run started (original pacing).
    pub offset_ms: i64,
    pub kind: String,
    pub text: String,
}

pub fn replay(conn: &Connection, run_id: &str) -> AppResult<Vec<ReplayStep>> {
    let run = crate::agents::runs::get(conn, run_id)?;
    let events = crate::agents::runs::events(conn, run_id)?;
    Ok(events
        .into_iter()
        .map(|e| {
            let p = &e.event;
            let kind = p.get("kind").and_then(Value::as_str).unwrap_or("event").to_string();
            let text = match kind.as_str() {
                "tool_use" => tool_title(p),
                "tool_result" => format!(
                    "{} {}",
                    if p.get("ok").and_then(Value::as_bool) == Some(false) { "✗" } else { "✓" },
                    p.get("output").and_then(Value::as_str).unwrap_or_default().lines().take(6).collect::<Vec<_>>().join("\n")
                ),
                "text" => p.get("text").and_then(Value::as_str).unwrap_or_default().to_string(),
                "error" | "warning" => p.get("message").and_then(Value::as_str).unwrap_or_default().to_string(),
                "result" => p.get("text").or_else(|| p.get("error")).and_then(Value::as_str).unwrap_or("fim").to_string(),
                "session" => format!("sessão iniciada{}", p.get("model").and_then(Value::as_str).map(|m| format!(" · {m}")).unwrap_or_default()),
                _ => p.to_string(),
            };
            ReplayStep { seq: e.seq, offset_ms: (e.created_at - run.started_at).max(0), kind, text: clip(&text, 2000) }
        })
        .collect())
}

// ── Live processes ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    /// terminal | agent | browser | voice | routine
    pub kind: String,
    pub label: String,
    pub workspace_id: Option<String>,
    pub pid: Option<u32>,
    pub started_at: Option<i64>,
    /// Measured with `ps` (None when the process is not visible to us).
    pub cpu_percent: Option<f64>,
    pub rss_kb: Option<i64>,
    pub ref_id: Option<String>,
}

/// CPU% and RSS (KB) per pid via `ps`, including all descendants' totals is
/// not attempted: values are for the listed pid itself.
pub fn ps_stats(pids: &[u32]) -> HashMap<u32, (f64, i64)> {
    let mut out = HashMap::new();
    if pids.is_empty() || !cfg!(unix) {
        return out;
    }
    // `ps -p` aborts on any invalid pid, so list everything and filter.
    let Ok(o) = std::process::Command::new("ps").args(["-A", "-o", "pid=,%cpu=,rss="]).output() else { return out };
    for line in String::from_utf8_lossy(&o.stdout).lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if let [pid, cpu, rss] = f.as_slice() {
            if let (Ok(p), Ok(c), Ok(r)) = (pid.parse::<u32>(), cpu.replace(',', ".").parse::<f64>(), rss.parse::<i64>()) {
                if pids.contains(&p) {
                    out.insert(p, (c, r));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn db() -> crate::db::Db {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|c| {
            c.execute_batch(
                "INSERT INTO workspaces (id, name, path, created_at) VALUES ('w1','w1','/tmp/w1',0), ('w2','w2','/tmp/w2',0);
                 INSERT INTO agent_runs (id, workspace_id, provider_id, model, permission, prompt, status, cwd, started_at, duration_ms, input_tokens, output_tokens, cost_usd, result_text, error) VALUES
                   ('a','w1','claude','haiku','read','Explique o login','completed','/tmp',1000,2000,100,50,0.01,'ok api_key=sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789',NULL),
                   ('b','w1','claude','haiku','edit','Corrija o bug','failed','/tmp',2000,4000,NULL,NULL,NULL,NULL,'boom'),
                   ('c','w1','codex',NULL,'full','Deploy','completed','/tmp',3000,6000,NULL,NULL,NULL,'done',NULL),
                   ('d','w2','claude','sonnet','read','Outro projeto','completed','/tmp',4000,8000,10,5,0.02,'x',NULL),
                   ('e','w1','claude','haiku','read','Rodando','running','/tmp',5000,NULL,NULL,NULL,NULL,NULL,NULL);
                 INSERT INTO agent_events (run_id, seq, kind, payload, created_at) VALUES
                   ('a',1,'session','{\"kind\":\"session\",\"model\":\"haiku\"}',1100),
                   ('a',2,'tool_use','{\"kind\":\"tool_use\",\"name\":\"Bash\",\"input\":\"{\\\"command\\\":\\\"npm test\\\"}\"}',1500),
                   ('a',3,'tool_result','{\"kind\":\"tool_result\",\"ok\":true,\"output\":\"3 passed\"}',2500),
                   ('a',4,'result','{\"kind\":\"result\",\"ok\":true,\"text\":\"ok\"}',3000);
                 INSERT INTO activity_events (kind, message, workspace_id, created_at) VALUES
                   ('git.push','Push de main','w1',1200), ('git.commit','Commit','w1',1300), ('mcp.secret.set','Segredo TOKEN definido',NULL,1400),
                   ('agent.run','Agente Codex iniciado (full)','w1',3000);",
            )?;
            Ok(())
        })
        .unwrap();
        db
    }

    #[test]
    fn metrics_separate_reported_from_unreported() {
        let db = db();
        let m = db.with(|c| metrics(c, 0, Some("w1"), &[0, 2500])).unwrap();
        assert_eq!((m.runs, m.completed, m.failed, m.running), (4, 2, 1, 1));
        assert_eq!(m.cost_usd, Reported { value: Some(0.01), reported_runs: 1, unreported_runs: 2 });
        assert_eq!(m.input_tokens, Reported { value: Some(100), reported_runs: 1, unreported_runs: 2 });
        assert_eq!(m.p50_duration_ms, Some(4000));
        let codex = m.providers.iter().find(|p| p.provider_id == "codex").unwrap();
        assert_eq!(codex.cost_usd, Reported { value: None, reported_runs: 0, unreported_runs: 1 }, "never invented");
        assert_eq!(m.tool_uses, 1);
        assert_eq!(m.days.len(), 2);
        assert_eq!((m.days[0].runs, m.days[0].failed, m.days[0].cost_runs), (2, 1, 1));
        assert_eq!(m.days[1].runs, 2);
        let all = db.with(|c| metrics(c, 0, None, &[])).unwrap();
        assert_eq!(all.runs, 5);
        assert!((all.cost_usd.value.unwrap() - 0.03).abs() < 1e-9);
    }

    #[test]
    fn timeline_merges_sources_filters_and_redacts() {
        let db = db();
        let items = db.with(|c| timeline(c, &TimelineQuery { workspace_id: Some("w1".into()), ..Default::default() })).unwrap();
        assert!(items.windows(2).all(|w| w[0].ts >= w[1].ts), "sorted newest first");
        assert!(items.iter().any(|i| i.source == "tool" && i.title == "Bash: npm test"));
        assert!(items.iter().all(|i| i.workspace_id.as_deref() != Some("w2")));
        let a = items.iter().find(|i| i.run_id.as_deref() == Some("a") && i.source == "agent").unwrap();
        assert!(!a.detail.as_ref().unwrap().contains("sk-ant-api03-abcdef"), "secrets redacted again on the way out");
        let found = db.with(|c| timeline(c, &TimelineQuery { query: Some("npm TEST".into()), ..Default::default() })).unwrap();
        assert_eq!(found.len(), 1);
        let errors = db.with(|c| timeline(c, &TimelineQuery { level: Some("error".into()), ..Default::default() })).unwrap();
        assert!(errors.iter().all(|i| i.level == "error") && errors.iter().any(|i| i.run_id.as_deref() == Some("b")));
        let only_agents = db.with(|c| timeline(c, &TimelineQuery { sources: Some(vec!["agent".into()]), ..Default::default() })).unwrap();
        assert!(only_agents.iter().all(|i| i.source == "agent"));
        assert!(db.with(|c| timeline(c, &TimelineQuery { query: Some("100%_x".into()), ..Default::default() })).unwrap().is_empty());
    }

    #[test]
    fn audit_lists_sensitive_actions_only() {
        let db = db();
        let audit = db.with(|c| timeline(c, &TimelineQuery { audit: true, ..Default::default() })).unwrap();
        let titles: Vec<&str> = audit.iter().map(|i| i.title.as_str()).collect();
        assert!(titles.contains(&"Push de main"));
        assert!(titles.contains(&"Segredo TOKEN definido"));
        assert!(titles.contains(&"Agente Codex iniciado (full)"));
        assert!(audit.iter().any(|i| i.source == "agent" && i.run_id.as_deref() == Some("c")), "full-permission runs");
        assert!(!titles.contains(&"Commit"), "routine actions are not audit noise");
        assert!(!audit.iter().any(|i| i.run_id.as_deref() == Some("a")));
    }

    #[test]
    fn replay_keeps_original_pacing() {
        let db = db();
        let steps = db.with(|c| replay(c, "a")).unwrap();
        assert_eq!(steps.iter().map(|s| s.offset_ms).collect::<Vec<_>>(), [100, 500, 1500, 2000]);
        assert_eq!(steps[1].text, "Bash: npm test");
        assert_eq!(steps[2].text, "✓ 3 passed");
        assert_eq!(steps[0].text, "sessão iniciada · haiku");
    }

    #[test]
    fn tool_titles_pick_the_meaningful_argument() {
        assert_eq!(tool_title(&json!({"name":"Read","input":"{\"file_path\":\"/x/a.rs\"}"})), "Read: /x/a.rs");
        assert_eq!(tool_title(&json!({"name":"Bash","input":"{\"command\":\"echo 1\\necho 2\"}"})), "Bash: echo 1");
        assert_eq!(tool_title(&json!({"name":"Web","input":"raw text"})), "Web: raw text");
        assert_eq!(tool_title(&json!({"name":"Noop","input":""})), "Noop");
    }

    #[test]
    fn ps_reads_real_process_stats() {
        let me = std::process::id();
        let s = ps_stats(&[me, 999_999_999]);
        let (_cpu, rss) = s.get(&me).copied().expect("own process visible");
        assert!(rss > 0);
        assert!(!s.contains_key(&999_999_999));
    }
}

#[cfg(test)]
mod live {
    use super::*;
    use std::time::{Duration, Instant};

    /// Generates real history with the installed CLIs (Claude succeeds with a
    /// tool call; Codex, logged out here, fails for real) and checks metrics,
    /// timeline, audit-free noise and replay on it.
    #[test]
    #[ignore = "uses the Claude Code and Codex CLIs"]
    fn live_metrics_on_real_runs() {
        let tmp = tempfile::tempdir().unwrap();
        let ws_dir = std::fs::canonicalize(tmp.path()).unwrap().join("proj");
        std::fs::create_dir(&ws_dir).unwrap();
        std::fs::write(ws_dir.join("hello.txt"), "OBS-4242").unwrap();
        let db_path = tmp.path().join("t.db");
        let db = crate::db::Db::open(&db_path).unwrap();
        let ws = db.with(|c| crate::db::workspaces::add(c, &ws_dir.display().to_string(), "proj")).unwrap();
        drop(db);
        let mgr = crate::agents::runner::RunManager::default();
        let mk = |provider: &str, prompt: &str| crate::agents::runner::RunRequest {
            workspace_id: ws.id.clone(),
            profile_id: None,
            parent_run_id: None,
            provider_id: provider.into(),
            model: if provider == "claude" { Some("haiku".into()) } else { None },
            permission: "read".into(),
            instructions: None,
            max_turns: Some(4),
            prompt: prompt.into(),
            resume_session: None,
            use_worktree: false,
            confirm_full: false,
            cwd_override: None,
            cwd_branch: None,
            skill_labels: vec![],
            mcp: None,
            mission_id: None,
            extra_skill_ids: vec![],
            extra_mcp_ids: vec![],
        };
        let wt = tmp.path().join("wt");
        let a = mgr.start(&db_path, &ws_dir, &wt, mk("claude", "Use the Read tool to read hello.txt and reply with its exact content only.")).unwrap();
        let b = mgr.start(&db_path, &ws_dir, &wt, mk("codex", "Say hi"));
        let start = Instant::now();
        while mgr.live_count() > 0 && start.elapsed() < Duration::from_secs(180) {
            std::thread::sleep(Duration::from_millis(300));
        }
        let c = crate::db::connect(&db_path).unwrap();
        let ra = crate::agents::runs::get(&c, &a.id).unwrap();
        eprintln!("claude: {} {:?} cost={:?}", ra.status, ra.result_text, ra.cost_usd);
        assert_eq!(ra.status, "completed");
        assert!(ra.result_text.unwrap_or_default().contains("OBS-4242"));
        if let Ok(b) = &b {
            eprintln!("codex: {} {:?}", crate::agents::runs::get(&c, &b.id).unwrap().status, crate::agents::runs::get(&c, &b.id).unwrap().error);
        }
        let m = metrics(&c, 0, Some(&ws.id), &[]).unwrap();
        eprintln!("runs={} ok={} failed={} cost={:?} tokens_in={:?} tools={}", m.runs, m.completed, m.failed, m.cost_usd, m.input_tokens, m.tool_uses);
        for p in &m.providers {
            eprintln!("  {} {:?}: {} runs, cost {:?}", p.provider_id, p.model, p.runs, p.cost_usd);
        }
        let claude = m.providers.iter().find(|p| p.provider_id == "claude").unwrap();
        assert_eq!(claude.cost_usd.reported_runs, 1, "Claude reports its cost");
        if let Some(codex) = m.providers.iter().find(|p| p.provider_id == "codex") {
            assert_eq!(codex.cost_usd.value, None, "no invented cost for a run that reported none");
        }
        assert!(m.tool_uses >= 1);
        let t = timeline(&c, &TimelineQuery::default()).unwrap();
        let tool = t.iter().find(|i| i.source == "tool").expect("tool call in history");
        eprintln!("tool item: {}", tool.title);
        assert!(tool.title.starts_with("Read"));
        let steps = replay(&c, &a.id).unwrap();
        eprintln!("replay: {:?}", steps.iter().map(|s| (s.offset_ms, s.kind.as_str())).collect::<Vec<_>>());
        assert!(steps.windows(2).all(|w| w[0].offset_ms <= w[1].offset_ms));
        assert!(steps.iter().any(|s| s.kind == "tool_use"));
    }
}
