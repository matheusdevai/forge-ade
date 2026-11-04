//! Autopilot (M18): scheduled routines.
//!
//! Safety rails against runaway or unauthorized execution:
//! - one execution at a time per routine (no overlap);
//! - daily run cap, per-run timeout (process group killed);
//! - auto-disable after N consecutive failures (user is notified);
//! - optional daily budget (only CLI-reported costs count);
//! - agents never get "full" permission; shell commands must be explicitly
//!   approved and any edit invalidates the approval (sha256 of the command);
//! - missed runs while FORGE was closed are not replayed.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{Datelike, Local, NaiveTime, TimeZone, Timelike};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::db::{now_ms, workspaces};
use crate::error::{AppError, AppResult};
use crate::redact::redact_secrets;

pub const MAX_OUTPUT: usize = 64 * 1024;
const TICK: Duration = Duration::from_secs(15);

// ── Types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ActionConfig {
    Agent {
        #[serde(rename = "profileId")]
        profile_id: String,
        prompt: String,
        permission: String,
        #[serde(default, rename = "useWorktree")]
        use_worktree: bool,
    },
    Script {
        script: String,
    },
    Command {
        command: String,
    },
    Mission {
        #[serde(rename = "recipeId")]
        recipe_id: String,
        title: String,
        #[serde(default)]
        values: HashMap<String, String>,
    },
    Verify {
        url: String,
    },
}

impl ActionConfig {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Agent { .. } => "agent",
            Self::Script { .. } => "script",
            Self::Command { .. } => "command",
            Self::Mission { .. } => "mission",
            Self::Verify { .. } => "verify",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Routine {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub workspace_id: String,
    pub action: ActionConfig,
    pub trigger: String,
    pub interval_min: Option<i64>,
    pub daily_time: Option<String>,
    pub days_mask: i64,
    pub max_runs_per_day: i64,
    pub timeout_min: i64,
    pub max_failures: i64,
    pub budget_usd_day: Option<f64>,
    /// Command routines: the current command is the approved one.
    pub command_approved: bool,
    pub consecutive_failures: i64,
    pub disabled_reason: Option<String>,
    pub next_run_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineInput {
    pub name: String,
    pub workspace_id: String,
    pub action: ActionConfig,
    pub trigger: String,
    pub interval_min: Option<i64>,
    pub daily_time: Option<String>,
    #[serde(default = "all_days")]
    pub days_mask: i64,
    #[serde(default = "d24")]
    pub max_runs_per_day: i64,
    #[serde(default = "d30")]
    pub timeout_min: i64,
    #[serde(default = "d3")]
    pub max_failures: i64,
    pub budget_usd_day: Option<f64>,
    /// The user confirmed the exact shell command (command routines).
    #[serde(default)]
    pub approve_command: bool,
}
fn all_days() -> i64 {
    127
}
fn d24() -> i64 {
    24
}
fn d30() -> i64 {
    30
}
fn d3() -> i64 {
    3
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RoutineRun {
    pub id: String,
    pub routine_id: String,
    pub trigger: String,
    pub status: String,
    pub output: Option<String>,
    pub error: Option<String>,
    pub agent_run_id: Option<String>,
    pub mission_id: Option<String>,
    pub cost_usd: Option<f64>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
}

pub fn sha256(s: &str) -> String {
    let d = Sha256::digest(s.as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect()
}

// ── Schedule (pure) ───────────────────────────────────────────────────────

pub fn parse_time(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s.trim(), "%H:%M").ok()
}

/// Next execution after `now` (local time), or None for manual/commit.
pub fn next_run<Tz: TimeZone>(trigger: &str, interval_min: Option<i64>, daily_time: Option<&str>, days_mask: i64, now: &chrono::DateTime<Tz>) -> Option<i64> {
    match trigger {
        "interval" => Some(now.timestamp_millis() + interval_min? * 60_000),
        "daily" => {
            let t = parse_time(daily_time?)?;
            let tz = now.timezone();
            for add in 0..8 {
                let day = now.date_naive() + chrono::Days::new(add);
                let wd = day.weekday().num_days_from_monday();
                if days_mask & (1 << wd) == 0 {
                    continue;
                }
                let local = day.and_hms_opt(t.hour(), t.minute(), 0)?;
                // DST gaps: take the earliest valid instant, skip nonexistent times.
                let Some(dt) = tz.from_local_datetime(&local).earliest() else { continue };
                if dt.timestamp_millis() > now.timestamp_millis() {
                    return Some(dt.timestamp_millis());
                }
            }
            None
        }
        _ => None,
    }
}

/// Start of the local day (for the daily caps).
fn day_start_ms() -> i64 {
    let now = Local::now();
    Local.from_local_datetime(&now.date_naive().and_hms_opt(0, 0, 0).unwrap()).earliest().map(|d| d.timestamp_millis()).unwrap_or(0)
}

// ── Repository ────────────────────────────────────────────────────────────

const COLS: &str = "id, name, enabled, workspace_id, action, config, trigger, interval_min, daily_time, days_mask, max_runs_per_day, timeout_min, max_failures, budget_usd_day, approved_hash, consecutive_failures, disabled_reason, next_run_at, created_at, updated_at";

fn map(r: &Row) -> rusqlite::Result<Routine> {
    let config: String = r.get(5)?;
    let action: ActionConfig = serde_json::from_str(&config).unwrap_or(ActionConfig::Command { command: String::new() });
    let approved: Option<String> = r.get(14)?;
    let command_approved = match &action {
        ActionConfig::Command { command } => approved.as_deref() == Some(sha256(command).as_str()),
        _ => true,
    };
    Ok(Routine {
        id: r.get(0)?,
        name: r.get(1)?,
        enabled: r.get::<_, i64>(2)? != 0,
        workspace_id: r.get(3)?,
        action,
        trigger: r.get(6)?,
        interval_min: r.get(7)?,
        daily_time: r.get(8)?,
        days_mask: r.get(9)?,
        max_runs_per_day: r.get(10)?,
        timeout_min: r.get(11)?,
        max_failures: r.get(12)?,
        budget_usd_day: r.get(13)?,
        command_approved,
        consecutive_failures: r.get(15)?,
        disabled_reason: r.get(16)?,
        next_run_at: r.get(17)?,
        created_at: r.get(18)?,
        updated_at: r.get(19)?,
    })
}

const RUN_COLS: &str = "id, routine_id, trigger, status, output, error, agent_run_id, mission_id, cost_usd, started_at, ended_at";

fn map_run(r: &Row) -> rusqlite::Result<RoutineRun> {
    Ok(RoutineRun {
        id: r.get(0)?,
        routine_id: r.get(1)?,
        trigger: r.get(2)?,
        status: r.get(3)?,
        output: r.get(4)?,
        error: r.get(5)?,
        agent_run_id: r.get(6)?,
        mission_id: r.get(7)?,
        cost_usd: r.get(8)?,
        started_at: r.get(9)?,
        ended_at: r.get(10)?,
    })
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Routine> {
    conn.query_row(&format!("SELECT {COLS} FROM routines WHERE id = ?1"), [id], map)
        .optional()?
        .ok_or_else(|| AppError::NotFound("rotina não encontrada".into()))
}

pub fn list(conn: &Connection, workspace_id: Option<&str>) -> AppResult<Vec<Routine>> {
    let mut st = conn.prepare(&format!("SELECT {COLS} FROM routines WHERE (?1 IS NULL OR workspace_id = ?1) ORDER BY created_at"))?;
    let rows = st.query_map([workspace_id], map)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn runs(conn: &Connection, routine_id: Option<&str>, limit: i64) -> AppResult<Vec<RoutineRun>> {
    let mut st = conn.prepare(&format!("SELECT {RUN_COLS} FROM routine_runs WHERE (?1 IS NULL OR routine_id = ?1) ORDER BY started_at DESC LIMIT ?2"))?;
    let rows = st.query_map(params![routine_id, limit.clamp(1, 500)], map_run)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get_run(conn: &Connection, id: &str) -> AppResult<RoutineRun> {
    conn.query_row(&format!("SELECT {RUN_COLS} FROM routine_runs WHERE id = ?1"), [id], map_run)
        .optional()?
        .ok_or_else(|| AppError::NotFound("execução não encontrada".into()))
}

fn validate(conn: &Connection, i: &RoutineInput, previous: Option<&Routine>) -> AppResult<Option<String>> {
    let name = i.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(AppError::Invalid("nome obrigatório (até 80 caracteres)".into()));
    }
    let ws = workspaces::get(conn, &i.workspace_id)?;
    match i.trigger.as_str() {
        "manual" | "commit" => {}
        "interval" => {
            let m = i.interval_min.ok_or_else(|| AppError::Invalid("informe o intervalo".into()))?;
            if !(5..=10_080).contains(&m) {
                return Err(AppError::Invalid("intervalo entre 5 minutos e 7 dias".into()));
            }
        }
        "daily" => {
            if i.daily_time.as_deref().and_then(parse_time).is_none() {
                return Err(AppError::Invalid("horário inválido (use HH:MM)".into()));
            }
            if !(1..=127).contains(&i.days_mask) {
                return Err(AppError::Invalid("escolha ao menos um dia da semana".into()));
            }
        }
        t => return Err(AppError::Invalid(format!("gatilho inválido: {t}"))),
    }
    if !(1..=288).contains(&i.max_runs_per_day) || !(1..=240).contains(&i.timeout_min) || !(1..=20).contains(&i.max_failures) {
        return Err(AppError::Invalid("limites fora do intervalo permitido".into()));
    }
    if i.budget_usd_day.is_some_and(|b| !(b > 0.0 && b < 10_000.0)) {
        return Err(AppError::Invalid("orçamento diário inválido".into()));
    }
    let mut approved = previous.and_then(|p| match &p.action {
        ActionConfig::Command { command } if p.command_approved => Some(sha256(command)),
        _ => None,
    });
    match &i.action {
        ActionConfig::Agent { profile_id, prompt, permission, .. } => {
            crate::agents::profiles::get(conn, profile_id)?;
            if prompt.trim().is_empty() || prompt.len() > 20_000 {
                return Err(AppError::Invalid("escreva o pedido do agente".into()));
            }
            if permission != "read" && permission != "edit" {
                return Err(AppError::Forbidden("rotinas não rodam agentes com acesso total — use leitura ou edição".into()));
            }
        }
        ActionConfig::Script { script } => {
            crate::browser::preview::command_for(Path::new(&ws.path), script)?;
        }
        ActionConfig::Command { command } => {
            let c = command.trim();
            if c.is_empty() || c.len() > 2000 || c.contains('\0') {
                return Err(AppError::Invalid("comando vazio ou longo demais".into()));
            }
            let hash = sha256(command);
            if i.approve_command {
                approved = Some(hash);
            } else if approved.as_deref() != Some(hash.as_str()) {
                return Err(AppError::Forbidden("aprove o comando exato antes de salvar a rotina".into()));
            }
        }
        ActionConfig::Mission { recipe_id, title, .. } => {
            crate::library::recipes::get(conn, recipe_id)?;
            if title.trim().is_empty() {
                return Err(AppError::Invalid("informe o título das missões criadas".into()));
            }
        }
        ActionConfig::Verify { url } => {
            crate::browser::normalize_url(url)?;
        }
    }
    Ok(approved)
}

fn schedule_next(conn: &Connection, id: &str) -> AppResult<()> {
    let r = get(conn, id)?;
    let next = if r.enabled { next_run(&r.trigger, r.interval_min, r.daily_time.as_deref(), r.days_mask, &Local::now()) } else { None };
    conn.execute("UPDATE routines SET next_run_at = ?2 WHERE id = ?1", params![id, next])?;
    Ok(())
}

pub fn create(conn: &Connection, i: &RoutineInput) -> AppResult<Routine> {
    let approved = validate(conn, i, None)?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_ms();
    conn.execute(
        &format!("INSERT INTO routines ({COLS}) VALUES (?1,?2,1,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,0,NULL,NULL,?15,?15)"),
        params![
            id, i.name.trim(), i.workspace_id, i.action.kind(), serde_json::to_string(&i.action)?, i.trigger, i.interval_min, i.daily_time, i.days_mask,
            i.max_runs_per_day, i.timeout_min, i.max_failures, i.budget_usd_day, approved, now
        ],
    )?;
    schedule_next(conn, &id)?;
    get(conn, &id)
}

pub fn update(conn: &Connection, id: &str, i: &RoutineInput) -> AppResult<Routine> {
    let prev = get(conn, id)?;
    let approved = validate(conn, i, Some(&prev))?;
    conn.execute(
        "UPDATE routines SET name=?2, workspace_id=?3, action=?4, config=?5, trigger=?6, interval_min=?7, daily_time=?8, days_mask=?9,
           max_runs_per_day=?10, timeout_min=?11, max_failures=?12, budget_usd_day=?13, approved_hash=?14, updated_at=?15 WHERE id=?1",
        params![
            id, i.name.trim(), i.workspace_id, i.action.kind(), serde_json::to_string(&i.action)?, i.trigger, i.interval_min, i.daily_time, i.days_mask,
            i.max_runs_per_day, i.timeout_min, i.max_failures, i.budget_usd_day, approved, now_ms()
        ],
    )?;
    schedule_next(conn, id)?;
    get(conn, id)
}

pub fn set_enabled(conn: &Connection, id: &str, enabled: bool) -> AppResult<Routine> {
    get(conn, id)?;
    conn.execute(
        "UPDATE routines SET enabled=?2, consecutive_failures = CASE WHEN ?2 THEN 0 ELSE consecutive_failures END,
           disabled_reason = CASE WHEN ?2 THEN NULL ELSE disabled_reason END, updated_at=?3 WHERE id=?1",
        params![id, enabled as i64, now_ms()],
    )?;
    schedule_next(conn, id)?;
    get(conn, id)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    get(conn, id)?;
    conn.execute("DELETE FROM routines WHERE id = ?1", [id])?;
    Ok(())
}

/// Reasons to not run now (None = allowed).
pub fn gate(conn: &Connection, r: &Routine) -> AppResult<Option<String>> {
    let running: i64 = conn.query_row("SELECT COUNT(*) FROM routine_runs WHERE routine_id = ?1 AND status = 'running'", [&r.id], |x| x.get(0))?;
    if running > 0 {
        return Ok(Some("a execução anterior ainda está em andamento".into()));
    }
    let today = day_start_ms();
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM routine_runs WHERE routine_id = ?1 AND started_at >= ?2 AND status != 'skipped'",
        params![r.id, today],
        |x| x.get(0),
    )?;
    if count >= r.max_runs_per_day {
        return Ok(Some(format!("limite diário de {} execuções atingido", r.max_runs_per_day)));
    }
    if let Some(b) = r.budget_usd_day {
        let spent: f64 = conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0) FROM routine_runs WHERE routine_id = ?1 AND started_at >= ?2",
            params![r.id, today],
            |x| x.get(0),
        )?;
        if spent >= b {
            return Ok(Some(format!("orçamento diário de US$ {b:.2} atingido (custo informado: US$ {spent:.4})")));
        }
    }
    if let ActionConfig::Command { .. } = r.action {
        if !r.command_approved {
            return Ok(Some("o comando mudou e precisa ser aprovado de novo".into()));
        }
    }
    Ok(None)
}

fn insert_run(conn: &Connection, routine_id: &str, trigger: &str, status: &str, error: Option<&str>) -> AppResult<RoutineRun> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_ms();
    let ended = (status != "running").then_some(now);
    conn.execute(
        "INSERT INTO routine_runs (id, routine_id, trigger, status, error, started_at, ended_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![id, routine_id, trigger, status, error, now, ended],
    )?;
    get_run(conn, &id)
}

/// Runs left "running" by a previous app session are closed as cancelled.
pub fn mark_orphans(conn: &Connection) -> AppResult<usize> {
    Ok(conn.execute(
        "UPDATE routine_runs SET status='cancelled', error='o FORGE foi fechado durante a execução', ended_at=?1 WHERE status='running'",
        [now_ms()],
    )?)
}

// ── Process capture ───────────────────────────────────────────────────────

/// Runs a process with a timeout (whole process group is killed), capturing
/// stdout+stderr (capped, secrets redacted). Returns (exit_ok, output).
pub fn run_process(mut cmd: Command, timeout: Duration, cancel: &AtomicBool) -> AppResult<(bool, String)> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn()?;
    let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let mut readers = Vec::new();
    for stream in [child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>), child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>)]
        .into_iter()
        .flatten()
    {
        let buf = buf.clone();
        readers.push(std::thread::spawn(move || {
            let mut s = stream;
            let mut chunk = [0u8; 8192];
            while let Ok(n) = s.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                let mut b = buf.lock().unwrap();
                b.extend_from_slice(&chunk[..n]);
                if b.len() > MAX_OUTPUT * 2 {
                    let cut = b.len() - MAX_OUTPUT;
                    b.drain(..cut);
                }
            }
        }));
    }
    let start = Instant::now();
    let pid = child.id();
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break Some(st);
        }
        if start.elapsed() > timeout || cancel.load(Ordering::SeqCst) {
            #[cfg(unix)]
            {
                let quiet = |sig: &str| {
                    let _ = Command::new("kill").args([sig, &format!("-{pid}")]).stdout(Stdio::null()).stderr(Stdio::null()).status();
                };
                quiet("-TERM");
                std::thread::sleep(Duration::from_millis(500));
                quiet("-KILL");
            }
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    for r in readers {
        let _ = r.join();
    }
    let raw = String::from_utf8_lossy(&buf.lock().unwrap()).to_string();
    let mut out = redact_secrets(&raw);
    if out.len() > MAX_OUTPUT {
        let mut cut = out.len() - MAX_OUTPUT;
        while !out.is_char_boundary(cut) {
            cut += 1;
        }
        out = format!("…{}", &out[cut..]);
    }
    match status {
        Some(st) => Ok((st.success(), if st.success() { out } else { format!("{out}\n[código de saída {}]", st.code().map(|c| c.to_string()).unwrap_or_else(|| "sinal".into())) })),
        None if cancel.load(Ordering::SeqCst) => Err(AppError::Conflict(format!("cancelada\n{out}"))),
        None => Err(AppError::Internal(format!("tempo esgotado ({} min) — processo encerrado\n{out}", timeout.as_secs() / 60))),
    }
}

// ── Engine ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutopilotEvent {
    pub routine_id: String,
    pub routine_name: String,
    pub run_id: String,
    pub status: String,
    pub message: Option<String>,
    /// The routine was disabled after too many consecutive failures.
    pub auto_disabled: bool,
}

pub type Notify = Arc<dyn Fn(AutopilotEvent) + Send + Sync>;

#[derive(Clone)]
pub struct Autopilot {
    inner: Arc<Inner>,
}

struct Inner {
    db_path: PathBuf,
    worktrees: PathBuf,
    runs: crate::agents::runner::RunManager,
    missions: Option<crate::missions::engine::MissionEngine>,
    browser: Option<Arc<crate::browser::manager::BrowserManager>>,
    notify: Notify,
    agent_program: Option<PathBuf>,
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
    stopped: AtomicBool,
}

impl Autopilot {
    pub fn new(
        db_path: PathBuf,
        worktrees: PathBuf,
        runs: crate::agents::runner::RunManager,
        missions: Option<crate::missions::engine::MissionEngine>,
        browser: Option<Arc<crate::browser::manager::BrowserManager>>,
        notify: Notify,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                db_path,
                worktrees,
                runs,
                missions,
                browser,
                notify,
                agent_program: None,
                cancels: Mutex::default(),
                stopped: AtomicBool::new(false),
            }),
        }
    }

    /// Tests: replace the agent CLI with a fake.
    pub fn with_agent_program(self, p: PathBuf) -> Self {
        let inner = Arc::try_unwrap(self.inner).unwrap_or_else(|_| panic!("configure before cloning"));
        Self { inner: Arc::new(Inner { agent_program: Some(p), ..inner }) }
    }

    fn conn(&self) -> AppResult<Connection> {
        crate::db::connect(&self.inner.db_path)
    }

    /// Starts an execution in the background; returns its record (which may
    /// be "skipped" with the reason when a limit blocks it).
    pub fn trigger(&self, routine_id: &str, trigger: &str) -> AppResult<RoutineRun> {
        let c = self.conn()?;
        let r = get(&c, routine_id)?;
        if let Some(reason) = gate(&c, &r)? {
            let run = insert_run(&c, &r.id, trigger, "skipped", Some(&reason))?;
            (self.inner.notify)(AutopilotEvent { routine_id: r.id.clone(), routine_name: r.name.clone(), run_id: run.id.clone(), status: "skipped".into(), message: Some(reason), auto_disabled: false });
            return Ok(run);
        }
        let run = insert_run(&c, &r.id, trigger, "running", None)?;
        let cancel = Arc::new(AtomicBool::new(false));
        self.inner.cancels.lock().unwrap().insert(run.id.clone(), cancel.clone());
        (self.inner.notify)(AutopilotEvent { routine_id: r.id.clone(), routine_name: r.name.clone(), run_id: run.id.clone(), status: "running".into(), message: None, auto_disabled: false });
        let me = self.clone();
        let run_id = run.id.clone();
        std::thread::Builder::new().name("forge-autopilot".into()).spawn(move || me.execute(r, run_id, cancel))?;
        Ok(run)
    }

    pub fn cancel(&self, run_id: &str) {
        if let Some(c) = self.inner.cancels.lock().unwrap().get(run_id) {
            c.store(true, Ordering::SeqCst);
        }
    }

    fn execute(&self, r: Routine, run_id: String, cancel: Arc<AtomicBool>) {
        let timeout = Duration::from_secs(r.timeout_min as u64 * 60);
        let result = self.perform(&r, timeout, &cancel);
        self.inner.cancels.lock().unwrap().remove(&run_id);
        let Ok(c) = self.conn() else { return };
        let (status, output, error, agent_run, mission, cost) = match result {
            Ok(o) => ("succeeded", o.output, None, o.agent_run_id, o.mission_id, o.cost_usd),
            Err(Failure { message, output, agent_run_id, cost_usd }) => {
                let st = if cancel.load(Ordering::SeqCst) { "cancelled" } else { "failed" };
                (st, output, Some(message), agent_run_id, None, cost_usd)
            }
        };
        let _ = c.execute(
            "UPDATE routine_runs SET status=?2, output=?3, error=?4, agent_run_id=?5, mission_id=?6, cost_usd=?7, ended_at=?8 WHERE id=?1",
            params![run_id, status, output, error, agent_run, mission, cost, now_ms()],
        );
        let mut auto_disabled = false;
        if status == "failed" {
            let _ = c.execute("UPDATE routines SET consecutive_failures = consecutive_failures + 1 WHERE id = ?1", [&r.id]);
            if let Ok(cur) = get(&c, &r.id) {
                if cur.enabled && cur.consecutive_failures >= cur.max_failures {
                    let reason = format!("desativada após {} falhas seguidas", cur.consecutive_failures);
                    let _ = c.execute("UPDATE routines SET enabled = 0, disabled_reason = ?2, next_run_at = NULL WHERE id = ?1", params![r.id, reason]);
                    auto_disabled = true;
                }
            }
        } else if status == "succeeded" {
            let _ = c.execute("UPDATE routines SET consecutive_failures = 0 WHERE id = ?1", [&r.id]);
        }
        let msg = match status {
            "succeeded" => format!("Rotina “{}” concluída", r.name),
            "cancelled" => format!("Rotina “{}” cancelada", r.name),
            _ => format!("Rotina “{}” falhou{}", r.name, if auto_disabled { " e foi desativada" } else { "" }),
        };
        let _ = crate::db::activity::log(&c, "autopilot.run", &msg, Some(&r.workspace_id), None);
        (self.inner.notify)(AutopilotEvent { routine_id: r.id.clone(), routine_name: r.name.clone(), run_id, status: status.into(), message: error, auto_disabled });
    }

    fn perform(&self, r: &Routine, timeout: Duration, cancel: &AtomicBool) -> Result<Outcome, Failure> {
        let c = self.conn().map_err(Failure::from)?;
        let ws = workspaces::get(&c, &r.workspace_id).map_err(Failure::from)?;
        let root = PathBuf::from(&ws.path);
        if !root.is_dir() {
            return Err(Failure::msg("a pasta do workspace não existe mais"));
        }
        match &r.action {
            ActionConfig::Command { command } => {
                if !r.command_approved {
                    return Err(Failure::msg("comando não aprovado"));
                }
                let (shell, _) = crate::terminal::default_shell();
                let mut cmd = Command::new(shell);
                cmd.args(["-lc", command]).current_dir(&root);
                proc_outcome(run_process(cmd, timeout, cancel))
            }
            ActionConfig::Script { script } => {
                let (pm, args) = crate::browser::preview::command_for(&root, script).map_err(Failure::from)?;
                let bin = crate::environment::which(&pm).ok_or_else(|| Failure::msg(&format!("{pm} não está instalado")))?;
                let mut cmd = crate::providers::provider_command(&bin);
                cmd.args(&args).current_dir(&root);
                proc_outcome(run_process(cmd, timeout, cancel))
            }
            ActionConfig::Agent { profile_id, prompt, permission, use_worktree } => {
                let p = crate::agents::profiles::get(&c, profile_id).map_err(Failure::from)?;
                let req = crate::agents::runner::RunRequest {
                    workspace_id: ws.id.clone(),
                    profile_id: Some(p.id.clone()),
                    parent_run_id: None,
                    provider_id: p.provider_id.clone(),
                    model: p.model.clone(),
                    permission: permission.clone(),
                    instructions: crate::skills::repo::with_skills(Some(p.instructions.clone()), &crate::skills::repo::compose(&c, Some(&p.id), None, None).map_err(Failure::from)?),
                    max_turns: p.max_turns,
                    prompt: prompt.clone(),
                    resume_session: None,
                    use_worktree: *use_worktree,
                    confirm_full: false,
                    cwd_override: None,
                    cwd_branch: None,
                    skill_labels: vec![],
                    mcp: None,
                    mission_id: None,
                    extra_skill_ids: vec![],
                    extra_mcp_ids: vec![],
                };
                drop(c);
                let run = self
                    .inner
                    .runs
                    .start_with_program(&self.inner.db_path, &root, &self.inner.worktrees, req, self.inner.agent_program.clone(), None)
                    .map_err(Failure::from)?;
                let start = Instant::now();
                loop {
                    std::thread::sleep(Duration::from_millis(250));
                    let c = self.conn().map_err(Failure::from)?;
                    let cur = crate::agents::runs::get(&c, &run.id).map_err(Failure::from)?;
                    if cur.status != "running" {
                        let out = cur.result_text.clone().map(|t| redact_secrets(&t));
                        return if cur.status == "completed" {
                            Ok(Outcome { output: out, agent_run_id: Some(run.id), mission_id: None, cost_usd: cur.cost_usd })
                        } else {
                            Err(Failure { message: cur.error.unwrap_or_else(|| format!("agente terminou como {}", cur.status)), output: out, agent_run_id: Some(run.id), cost_usd: cur.cost_usd })
                        };
                    }
                    if start.elapsed() > timeout || cancel.load(Ordering::SeqCst) {
                        let _ = self.inner.runs.cancel(&run.id);
                        let msg = if cancel.load(Ordering::SeqCst) { "cancelada".to_string() } else { format!("tempo esgotado ({} min) — agente interrompido", timeout.as_secs() / 60) };
                        return Err(Failure { message: msg, output: None, agent_run_id: Some(run.id), cost_usd: None });
                    }
                }
            }
            ActionConfig::Mission { recipe_id, title, values } => {
                let engine = self.inner.missions.as_ref().ok_or_else(|| Failure::msg("motor de missões indisponível"))?;
                let stamped = format!("{} — {}", title.trim(), Local::now().format("%d/%m %H:%M"));
                let m = crate::library::recipes::instantiate(&c, recipe_id, Some(&ws.id), &stamped, values).map_err(Failure::from)?;
                drop(c);
                engine.plan(&m.id).map_err(Failure::from)?;
                Ok(Outcome {
                    output: Some(format!("Missão “{stamped}” criada e em planejamento. Iniciar a execução continua dependendo da sua aprovação.")),
                    agent_run_id: None,
                    mission_id: Some(m.id),
                    cost_usd: None,
                })
            }
            ActionConfig::Verify { url } => {
                let b = self.inner.browser.as_ref().ok_or_else(|| Failure::msg("navegador indisponível"))?;
                let tab = b.open(&ws.id, Some(url), 1280, 800, false).map_err(Failure::from)?;
                std::thread::sleep(Duration::from_millis(500));
                let rep = b.verify(&tab.id, Duration::from_millis(1500));
                let _ = b.close(&tab.id);
                let rep = rep.map_err(Failure::from)?;
                if rep.ok {
                    Ok(Outcome { output: Some(rep.summary), agent_run_id: None, mission_id: None, cost_usd: None })
                } else {
                    Err(Failure { message: format!("{} erro(s) na página", rep.errors.len()), output: Some(rep.summary), agent_run_id: None, cost_usd: None })
                }
            }
        }
    }

    /// One scheduler pass: due schedules and new commits.
    pub fn tick(&self) -> AppResult<Vec<RoutineRun>> {
        let c = self.conn()?;
        let now = now_ms();
        let mut fired = Vec::new();
        for r in list(&c, None)?.into_iter().filter(|r| r.enabled) {
            match r.trigger.as_str() {
                "interval" | "daily" => {
                    if r.next_run_at.is_some_and(|t| t <= now) {
                        // Reschedule first: a slow run never causes a burst.
                        schedule_next(&c, &r.id)?;
                        fired.push(self.trigger(&r.id, "schedule")?);
                    } else if r.next_run_at.is_none() {
                        schedule_next(&c, &r.id)?;
                    }
                }
                "commit" => {
                    let Ok(ws) = workspaces::get(&c, &r.workspace_id) else { continue };
                    let head = crate::git::head_commit(Path::new(&ws.path));
                    let last: Option<String> = c.query_row("SELECT last_head FROM routines WHERE id = ?1", [&r.id], |x| x.get(0))?;
                    if head.is_some() && head != last {
                        c.execute("UPDATE routines SET last_head = ?2 WHERE id = ?1", params![r.id, head])?;
                        // The first observation only records the baseline.
                        if last.is_some() {
                            fired.push(self.trigger(&r.id, "commit")?);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(fired)
    }

    /// Background scheduler. Missed schedules (app closed) are not replayed.
    pub fn start(&self) {
        if let Ok(c) = self.conn() {
            let _ = mark_orphans(&c);
            if let Ok(all) = list(&c, None) {
                for r in all {
                    let _ = schedule_next(&c, &r.id);
                }
            }
        }
        let me = self.clone();
        let _ = std::thread::Builder::new().name("forge-autopilot-scheduler".into()).spawn(move || {
            while !me.inner.stopped.load(Ordering::SeqCst) {
                if let Err(e) = me.tick() {
                    eprintln!("autopilot tick: {e}");
                }
                std::thread::sleep(TICK);
            }
        });
    }

    pub fn stop(&self) {
        self.inner.stopped.store(true, Ordering::SeqCst);
        for c in self.inner.cancels.lock().unwrap().values() {
            c.store(true, Ordering::SeqCst);
        }
    }
}

struct Outcome {
    output: Option<String>,
    agent_run_id: Option<String>,
    mission_id: Option<String>,
    cost_usd: Option<f64>,
}

struct Failure {
    message: String,
    output: Option<String>,
    agent_run_id: Option<String>,
    cost_usd: Option<f64>,
}

impl Failure {
    fn msg(m: &str) -> Self {
        Self { message: m.into(), output: None, agent_run_id: None, cost_usd: None }
    }
}

impl From<AppError> for Failure {
    fn from(e: AppError) -> Self {
        Self::msg(&e.to_string())
    }
}

fn proc_outcome(r: AppResult<(bool, String)>) -> Result<Outcome, Failure> {
    match r {
        Ok((true, out)) => Ok(Outcome { output: Some(out), agent_run_id: None, mission_id: None, cost_usd: None }),
        Ok((false, out)) => Err(Failure { message: "o processo terminou com erro".into(), output: Some(out), agent_run_id: None, cost_usd: None }),
        Err(e) => {
            let s = e.to_string();
            let (head, rest) = s.split_once('\n').unwrap_or((&s, ""));
            Err(Failure { message: head.to_string(), output: (!rest.is_empty()).then(|| rest.to_string()), agent_run_id: None, cost_usd: None })
        }
    }
}

#[cfg(test)]
mod tests;
