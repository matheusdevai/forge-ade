//! Agent run lifecycle: spawn the provider CLI headless, stream and persist
//! normalized events, support attach/detach, cancellation and timeouts.
//!
//! Permission model (least privilege by default):
//! - read: never edits files (Claude `plan`, Codex `read-only` sandbox, Gemini `plan`)
//! - edit: may edit files in the run directory (Claude `acceptEdits`,
//!   Codex `workspace-write`, Gemini `auto_edit`)
//! - full: unrestricted tools (Claude `bypassPermissions`, Codex
//!   `danger-full-access`, Gemini `yolo`) — requires explicit confirmation.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::runs::{self, Run, StoredEvent, MAX_EVENTS_PER_RUN};
use super::stream::{AgentEvent, StreamParser};
use crate::db::{new_id, now_ms};
use crate::error::{AppError, AppResult};
use crate::{environment, git, providers};

pub const MAX_CONCURRENT_RUNS: usize = 8;
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const MAX_PROMPT_CHARS: usize = 100_000;

pub type Subscriber = Arc<dyn Fn(Value) + Send + Sync>;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRequest {
    pub workspace_id: String,
    pub profile_id: Option<String>,
    pub parent_run_id: Option<String>,
    pub provider_id: String,
    pub model: Option<String>,
    pub permission: String,
    pub instructions: Option<String>,
    pub max_turns: Option<i64>,
    pub prompt: String,
    /// Continue this provider session (from a previous run).
    pub resume_session: Option<String>,
    /// Run inside a new git worktree (own branch) instead of the workspace.
    pub use_worktree: bool,
    /// Required for `permission = "full"`.
    #[serde(default)]
    pub confirm_full: bool,
    /// Run directory override (used when continuing a run inside its worktree).
    #[serde(skip)]
    pub cwd_override: Option<PathBuf>,
    /// Branch of `cwd_override` when it is a worktree prepared by the caller.
    #[serde(skip)]
    pub cwd_branch: Option<String>,
    /// Labels of skills already composed into `instructions` (recorded on the run).
    #[serde(skip)]
    pub skill_labels: Vec<String>,
    /// MCP servers to attach (Claude Code). The config file is deleted when the run ends.
    #[serde(skip)]
    pub mcp: Option<crate::mcp::repo::McpAttach>,
    /// Skills chosen for this run only (Harness), on top of the profile's.
    #[serde(default)]
    pub extra_skill_ids: Vec<String>,
    /// MCP servers chosen for this run only (Harness), on top of the profile's.
    #[serde(default)]
    pub extra_mcp_ids: Vec<String>,
    /// Mission this run belongs to (memory scope + where suggestions go).
    #[serde(skip)]
    pub mission_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachResult {
    pub run: Run,
    pub events: Vec<StoredEvent>,
    pub token: Option<u64>,
    pub live: bool,
}

struct LiveRun {
    /// Serializes "persist + emit" against "snapshot + subscribe".
    lock: Mutex<()>,
    subscriber: Mutex<Option<(u64, Subscriber)>>,
    pid: Mutex<Option<u32>>,
    cancelled: AtomicBool,
}

impl LiveRun {
    fn emit(&self, msg: Value) {
        let sub = self.subscriber.lock().ok().and_then(|s| s.as_ref().map(|(_, f)| f.clone()));
        if let Some(f) = sub {
            f(msg);
        }
    }
}

/// Cheap to clone: all state is shared.
#[derive(Default, Clone)]
pub struct RunManager {
    live: Arc<Mutex<HashMap<String, Arc<LiveRun>>>>,
    tokens: Arc<AtomicU64>,
}

/// Called once with the final run record when a run ends (any status).
pub type OnFinish = Box<dyn FnOnce(Run) + Send + 'static>;

fn open_db(path: &Path) -> AppResult<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")?;
    Ok(conn)
}

fn map_permission(provider: &str, permission: &str) -> AppResult<Vec<&'static str>> {
    Ok(match (provider, permission) {
        ("claude", "read") => vec!["--permission-mode", "plan"],
        ("claude", "edit") => vec!["--permission-mode", "acceptEdits"],
        ("claude", "full") => vec!["--permission-mode", "bypassPermissions"],
        ("codex", "read") => vec!["--sandbox", "read-only"],
        ("codex", "edit") => vec!["--sandbox", "workspace-write"],
        ("codex", "full") => vec!["--sandbox", "danger-full-access"],
        ("gemini", "read") => vec!["--approval-mode", "plan"],
        ("gemini", "edit") => vec!["--approval-mode", "auto_edit"],
        ("gemini", "full") => vec!["--approval-mode", "yolo"],
        _ => return Err(AppError::Invalid(format!("permissão inválida: {permission}"))),
    })
}

/// Builds the CLI arguments for a headless streaming run. Returns
/// `(args, stdin_prompt)`: Codex reads the prompt from stdin (`-`) so it can
/// never be parsed as a flag.
pub fn build_args(req: &RunRequest) -> AppResult<(Vec<String>, Option<String>)> {
    let def = providers::def(&req.provider_id)?;
    let perm = map_permission(def.id, &req.permission)?;
    let instructions = req.instructions.as_deref().map(str::trim).filter(|s| !s.is_empty());
    // Providers without a system-prompt flag receive instructions as a prompt prefix.
    let prefixed = |p: &str| match instructions {
        Some(i) => format!("<instructions>\n{i}\n</instructions>\n\n{p}"),
        None => p.to_string(),
    };
    let model = req.model.as_deref().filter(|m| !m.is_empty());
    if let Some(m) = model {
        providers::interactive_args(def, "agent", Some(m))?; // validates
    }
    let mut a: Vec<String> = Vec::new();
    let mut stdin = None;
    match def.id {
        "claude" => {
            a.extend(["-p".into(), req.prompt.clone(), "--output-format".into(), "stream-json".into(), "--verbose".into(), "--include-partial-messages".into()]);
            a.extend(perm.iter().map(|s| s.to_string()));
            if let Some(i) = instructions {
                a.extend(["--append-system-prompt".into(), i.to_string()]);
            }
            if let Some(t) = req.max_turns {
                a.extend(["--max-turns".into(), t.to_string()]);
            }
            if let Some(sid) = &req.resume_session {
                a.extend(["--resume".into(), sid.clone()]);
            }
            if let Some(m) = &req.mcp {
                a.extend(["--mcp-config".into(), m.config_file.display().to_string()]);
                if m.strict {
                    a.push("--strict-mcp-config".into());
                }
                if !m.allowed_tools.is_empty() {
                    a.push("--allowedTools".into());
                    a.extend(m.allowed_tools.iter().cloned());
                }
            }
        }
        "codex" => {
            a.extend(["exec".into(), "--json".into(), "--skip-git-repo-check".into()]);
            a.extend(perm.iter().map(|s| s.to_string()));
            if let Some(m) = model {
                a.extend(["-m".into(), m.into()]);
            }
            if let Some(sid) = &req.resume_session {
                a.extend(["resume".into(), sid.clone()]);
            }
            a.push("-".into());
            stdin = Some(prefixed(&req.prompt));
        }
        "gemini" => {
            // `--skip-trust`: trust the workspace the user opened, for this session only;
            // without it Gemini silently downgrades the approval mode in untrusted folders.
            a.extend(["-p".into(), prefixed(&req.prompt), "-o".into(), "stream-json".into(), "--skip-trust".into()]);
            a.extend(perm.iter().map(|s| s.to_string()));
            if req.resume_session.is_some() {
                return Err(AppError::Invalid("o Gemini CLI não permite retomar uma sessão específica por ID; inicie uma nova conversa".into()));
            }
        }
        _ => unreachable!(),
    }
    if def.id != "codex" {
        if let Some(m) = model {
            a.extend([def.model_flag.to_string(), m.to_string()]);
        }
    }
    Ok((a, stdin))
}

fn validate(req: &RunRequest) -> AppResult<()> {
    let n = req.prompt.trim().chars().count();
    if n == 0 || n > MAX_PROMPT_CHARS {
        return Err(AppError::Invalid("o prompt deve ter de 1 a 100.000 caracteres".into()));
    }
    if req.permission == "full" && !req.confirm_full {
        return Err(AppError::Forbidden("a permissão total exige confirmação explícita".into()));
    }
    if let Some(t) = req.max_turns {
        if !(1..=500).contains(&t) {
            return Err(AppError::Invalid("limite de turnos inválido".into()));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn kill_group(pid: u32, signal: &str) {
    let _ = std::process::Command::new("kill").args([signal, &format!("-{pid}")]).status();
}
#[cfg(not(unix))]
fn kill_group(pid: u32, _signal: &str) {
    let _ = std::process::Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).status();
}

impl RunManager {
    pub fn live_count(&self) -> usize {
        self.live.lock().map(|m| m.len()).unwrap_or(0)
    }

    /// (run id, process id) of runs currently executing.
    pub fn live_pids(&self) -> Vec<(String, Option<u32>)> {
        self.live
            .lock()
            .map(|m| m.iter().map(|(id, l)| (id.clone(), l.pid.lock().ok().and_then(|p| *p))).collect())
            .unwrap_or_default()
    }

    /// Starts a run. `root` is the workspace directory; `worktrees_root` the
    /// managed folder for worktree runs.
    pub fn start(&self, db_path: &Path, root: &Path, worktrees_root: &Path, req: RunRequest) -> AppResult<Run> {
        self.start_with_program(db_path, root, worktrees_root, req, None, None)
    }

    /// `program` overrides the provider binary (tests); `on_finish` runs after the run is finalized.
    pub fn start_with_program(
        &self,
        db_path: &Path,
        root: &Path,
        worktrees_root: &Path,
        mut req: RunRequest,
        program: Option<PathBuf>,
        on_finish: Option<OnFinish>,
    ) -> AppResult<Run> {
        validate(&req)?;
        // Shared memory (M14): composed here so every caller gets the same rules.
        let memory = {
            let conn = open_db(db_path)?;
            let ws = Some(req.workspace_id.as_str()).filter(|w| !w.is_empty());
            crate::memory::compose(&conn, ws, req.mission_id.as_deref(), req.profile_id.as_deref())?
        };
        req.instructions = crate::memory::with_memory(req.instructions.take(), &memory);
        if self.live_count() >= MAX_CONCURRENT_RUNS {
            return Err(AppError::Invalid(format!("limite de {MAX_CONCURRENT_RUNS} agentes simultâneos atingido")));
        }
        let def = providers::def(&req.provider_id)?;
        let bin = match program {
            Some(p) => p,
            None => environment::which(def.bin).ok_or_else(|| AppError::NotFound(format!("{} não está instalado", def.name)))?,
        };
        let (args, stdin_prompt) = build_args(&req)?;
        let id = new_id();

        // Working directory: workspace, existing worktree (continuation) or a new worktree.
        let (cwd, branch) = if let Some(dir) = &req.cwd_override {
            (dir.clone(), req.cwd_branch.clone())
        } else if req.use_worktree {
            let branch = format!("forge/agent-{}", &id[..8]);
            let wt = git::worktree_add(root, worktrees_root, &branch, true, None)?;
            (PathBuf::from(wt.path), Some(branch))
        } else {
            (std::fs::canonicalize(root)?, None)
        };
        let base_commit = git::head_commit(&cwd);

        let mut run = Run {
            id: id.clone(),
            workspace_id: Some(req.workspace_id.clone()),
            profile_id: req.profile_id.clone(),
            parent_run_id: req.parent_run_id.clone(),
            provider_id: def.id.into(),
            model: req.model.clone().filter(|m| !m.is_empty()),
            permission: req.permission.clone(),
            prompt: crate::redact::sanitize(&req.prompt, MAX_PROMPT_CHARS * 4),
            status: "running".into(),
            session_id: req.resume_session.clone(),
            cwd: cwd.display().to_string(),
            worktree_branch: branch,
            base_commit,
            result_text: None,
            error: None,
            input_tokens: None,
            output_tokens: None,
            cache_read_tokens: None,
            cost_usd: None,
            duration_ms: None,
            started_at: now_ms(),
            ended_at: None,
            skills: req.skill_labels.clone(),
            mcp: if def.id == "claude" { req.mcp.as_ref().map(|m| m.names.clone()).unwrap_or_default() } else { vec![] },
            memory: memory.labels.clone(),
        };
        // The MCP config file (with resolved secrets) must not outlive the run.
        let mcp_file = req.mcp.as_ref().map(|m| m.config_file.clone());
        let conn = open_db(db_path)?;
        runs::insert(&conn, &run)?;

        let mut cmd = providers::provider_command(&bin);
        cmd.args(&args).current_dir(&cwd).stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.stdin(if stdin_prompt.is_some() { Stdio::piped() } else { Stdio::null() });
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0); // so cancellation also stops the agent's subprocesses
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                run.status = "failed".into();
                run.error = Some(format!("falha ao iniciar {}: {e}", def.name));
                run.ended_at = Some(now_ms());
                runs::finish(&conn, &run)?;
                if let Some(f) = &mcp_file {
                    let _ = std::fs::remove_file(f);
                }
                if let Some(cb) = on_finish {
                    let r = run.clone();
                    std::thread::spawn(move || cb(r));
                }
                return Ok(run);
            }
        };
        if let (Some(text), Some(mut sin)) = (stdin_prompt, child.stdin.take()) {
            std::thread::spawn(move || {
                let _ = sin.write_all(text.as_bytes());
            });
        }
        let live = Arc::new(LiveRun {
            lock: Mutex::new(()),
            subscriber: Mutex::new(None),
            pid: Mutex::new(child.id().into()),
            cancelled: AtomicBool::new(false),
        });
        self.live.lock().map_err(|_| AppError::Internal("lock".into()))?.insert(id.clone(), live.clone());

        let db_path = db_path.to_path_buf();
        let started = Instant::now();
        let run_for_thread = run.clone();
        // Watchdog: cancel after the timeout.
        {
            let live = live.clone();
            std::thread::spawn(move || {
                let deadline = Instant::now() + DEFAULT_TIMEOUT;
                while Instant::now() < deadline {
                    std::thread::sleep(Duration::from_secs(1));
                    if live.pid.lock().map(|p| p.is_none()).unwrap_or(true) {
                        return;
                    }
                }
                live.cancelled.store(true, Ordering::SeqCst);
                if let Some(pid) = *live.pid.lock().unwrap_or_else(|e| e.into_inner()) {
                    kill_group(pid, "-TERM");
                }
            });
        }
        let live_map = self.live.clone();
        let run_id = id.clone();
        let mission_id = req.mission_id.clone();
        std::thread::Builder::new()
            .name(format!("forge-agent-{}", &id[..8]))
            .spawn(move || {
                let result = pump(db_path.clone(), run_for_thread, child, live.clone(), started);
                if let Some(f) = &mcp_file {
                    let _ = std::fs::remove_file(f);
                }
                if let Err(e) = &result {
                    eprintln!("agent run pump error: {e}");
                }
                // Memory proposals from the final answer (inactive until approved).
                if let Ok(c) = open_db(&db_path) {
                    if let Ok(r) = runs::get(&c, &run_id) {
                        if let (Some(text), true) = (r.result_text.as_deref(), r.status == "completed") {
                            if let Ok(made) = crate::memory::record_suggestions(&c, &run_id, r.workspace_id.as_deref(), mission_id.as_deref(), text) {
                                if !made.is_empty() {
                                    let _ = crate::db::activity::log(
                                        &c,
                                        "memory.suggested",
                                        &format!("{} sugestão(ões) de memória aguardando revisão", made.len()),
                                        r.workspace_id.as_deref(),
                                        None,
                                    );
                                }
                            }
                        }
                    }
                }
                if let Ok(mut m) = live_map.lock() {
                    m.remove(&run_id);
                }
                if let Some(cb) = on_finish {
                    let finished = open_db(&db_path).and_then(|c| runs::get(&c, &run_id));
                    if let Ok(r) = finished {
                        cb(r);
                    }
                }
            })
            .map_err(AppError::Io)?;
        Ok(run)
    }

    pub fn attach(&self, db_path: &Path, id: &str, subscriber: Subscriber) -> AppResult<AttachResult> {
        let live = self.live.lock().ok().and_then(|m| m.get(id).cloned());
        let conn = open_db(db_path)?;
        match live {
            Some(l) => {
                let _g = l.lock.lock().map_err(|_| AppError::Internal("lock".into()))?;
                let token = self.tokens.fetch_add(1, Ordering::Relaxed) + 1;
                *l.subscriber.lock().map_err(|_| AppError::Internal("lock".into()))? = Some((token, subscriber));
                Ok(AttachResult { run: runs::get(&conn, id)?, events: runs::events(&conn, id)?, token: Some(token), live: true })
            }
            None => Ok(AttachResult { run: runs::get(&conn, id)?, events: runs::events(&conn, id)?, token: None, live: false }),
        }
    }

    pub fn detach(&self, id: &str, token: u64) {
        if let Some(l) = self.live.lock().ok().and_then(|m| m.get(id).cloned()) {
            if let Ok(mut s) = l.subscriber.lock() {
                if s.as_ref().is_some_and(|(t, _)| *t == token) {
                    *s = None;
                }
            }
        }
    }

    pub fn cancel(&self, id: &str) -> AppResult<()> {
        let l = self
            .live
            .lock()
            .ok()
            .and_then(|m| m.get(id).cloned())
            .ok_or_else(|| AppError::Invalid("a execução não está em andamento".into()))?;
        l.cancelled.store(true, Ordering::SeqCst);
        if let Some(pid) = *l.pid.lock().map_err(|_| AppError::Internal("lock".into()))? {
            kill_group(pid, "-TERM");
            let l2 = l.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(3));
                if let Some(pid) = *l2.pid.lock().unwrap_or_else(|e| e.into_inner()) {
                    kill_group(pid, "-KILL");
                }
            });
        }
        Ok(())
    }

    pub fn cancel_all(&self) {
        let ids: Vec<String> = self.live.lock().map(|m| m.keys().cloned().collect()).unwrap_or_default();
        for id in ids {
            let _ = self.cancel(&id);
        }
    }
}

fn pump(db_path: PathBuf, mut run: Run, mut child: Child, live: Arc<LiveRun>, started: Instant) -> AppResult<()> {
    let conn = open_db(&db_path)?;
    let stdout = child.stdout.take().ok_or_else(|| AppError::Internal("stdout".into()))?;
    let stderr = child.stderr.take();
    let stderr_t = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut s) = stderr {
            let _ = (&mut s).take(256 * 1024).read_to_end(&mut buf);
        }
        crate::redact::sanitize(&String::from_utf8_lossy(&buf), 16 * 1024)
    });

    let mut parser = StreamParser::new(&run.provider_id);
    let mut seq: i64 = 0;
    let mut persisted: i64 = 0;
    let handle = |events: Vec<AgentEvent>, run: &mut Run, seq: &mut i64, persisted: &mut i64| {
        let _g = live.lock.lock();
        for ev in events {
            *seq += 1;
            let payload = serde_json::to_value(&ev).unwrap_or(Value::Null);
            if let AgentEvent::Session { session_id: Some(sid), .. } = &ev {
                if run.session_id.as_deref() != Some(sid) {
                    run.session_id = Some(sid.clone());
                    let _ = runs::set_session(&conn, &run.id, sid);
                }
            }
            if ev.persisted() && *persisted < MAX_EVENTS_PER_RUN {
                *persisted += 1;
                let _ = runs::insert_event(&conn, &run.id, *seq, ev.kind(), &payload);
            }
            live.emit(json!({ "type": "event", "seq": *seq, "event": payload }));
        }
    };

    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        let events = parser.feed(&line);
        if !events.is_empty() {
            handle(events, &mut run, &mut seq, &mut persisted);
        }
    }
    let tail = parser.finish();
    if !tail.is_empty() {
        handle(tail, &mut run, &mut seq, &mut persisted);
    }
    let status = child.wait().ok();
    if let Ok(mut pid) = live.pid.lock() {
        *pid = None;
    }
    let stderr_text = stderr_t.join().unwrap_or_default();

    let s = &parser.summary;
    let cancelled = live.cancelled.load(Ordering::SeqCst);
    run.status = if cancelled {
        "cancelled".into()
    } else if s.ok == Some(true) {
        "completed".into()
    } else {
        "failed".into()
    };
    run.result_text = s.text.clone();
    run.error = match run.status.as_str() {
        "cancelled" => Some("cancelada".into()),
        "failed" => s
            .error
            .clone()
            .or_else(|| stderr_text.lines().rev().find(|l| !l.trim().is_empty()).map(str::to_string))
            .or_else(|| status.and_then(|st| st.code()).map(|c| format!("o processo terminou com código {c}"))),
        _ => None,
    };
    if s.session_id.is_some() {
        run.session_id = s.session_id.clone();
    }
    run.model = s.model.clone().or(run.model);
    run.input_tokens = s.input_tokens;
    run.output_tokens = s.output_tokens;
    run.cache_read_tokens = s.cache_read_tokens;
    run.cost_usd = s.cost_usd;
    run.duration_ms = Some(started.elapsed().as_millis() as i64);
    run.ended_at = Some(now_ms());
    runs::finish(&conn, &run)?;
    let _g = live.lock.lock();
    live.emit(json!({ "type": "status", "run": run }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(provider: &str, permission: &str) -> RunRequest {
        RunRequest {
            workspace_id: "w".into(),
            profile_id: None,
            parent_run_id: None,
            provider_id: provider.into(),
            model: None,
            permission: permission.into(),
            instructions: Some("Be terse.".into()),
            max_turns: Some(5),
            prompt: "--help me".into(),
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
        }
    }

    #[test]
    fn maps_permissions_and_flags_per_provider() {
        let (a, stdin) = build_args(&req("claude", "read")).unwrap();
        assert!(stdin.is_none());
        let joined = a.join(" ");
        assert!(joined.contains("--permission-mode plan"));
        assert!(joined.contains("--append-system-prompt Be terse."));
        assert!(joined.contains("--max-turns 5"));
        assert_eq!(a[0..2], ["-p".to_string(), "--help me".to_string()], "prompt is the value of -p, never a flag");

        let (a, stdin) = build_args(&req("codex", "edit")).unwrap();
        assert_eq!(a.last().map(String::as_str), Some("-"), "codex reads the prompt from stdin");
        assert!(a.join(" ").contains("--sandbox workspace-write"));
        assert!(stdin.unwrap().starts_with("<instructions>\nBe terse."));

        let (a, _) = build_args(&req("gemini", "read")).unwrap();
        let joined = a.join(" ");
        assert!(joined.contains("--approval-mode plan") && joined.contains("--skip-trust") && joined.contains("stream-json"));

        let mut r = req("codex", "read");
        r.resume_session = Some("abc".into());
        let (a, _) = build_args(&r).unwrap();
        assert!(a.join(" ").ends_with("resume abc -"));
        let mut r = req("gemini", "read");
        r.resume_session = Some("abc".into());
        assert!(build_args(&r).is_err());
        let mut r = req("claude", "read");
        r.model = Some("--evil".into());
        assert!(build_args(&r).is_err());
    }

    #[test]
    fn full_permission_requires_confirmation() {
        assert!(matches!(validate(&req("claude", "full")), Err(AppError::Forbidden(_))));
        let mut r = req("claude", "full");
        r.confirm_full = true;
        assert!(validate(&r).is_ok());
        let mut r = req("claude", "read");
        r.prompt = "   ".into();
        assert!(validate(&r).is_err());
    }

    fn test_db() -> (tempfile::TempDir, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("t.db");
        let db = crate::db::Db::open(&p).unwrap();
        db.with(|c| {
            c.execute("INSERT INTO workspaces (id, name, path, created_at) VALUES ('w', 'w', '/tmp/forge-test-w', 1)", [])?;
            Ok(())
        })
        .unwrap();
        drop(db);
        (d, p)
    }

    /// A fake "claude" that replays the real recorded stream, so the full
    /// spawn → parse → persist → emit → finish pipeline runs for real.
    #[cfg(unix)]
    fn fake_cli(dir: &Path, script: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("fake-cli.sh");
        std::fs::write(&p, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    fn wait_for(mgr: &RunManager, id: &str) {
        let start = Instant::now();
        while mgr.live.lock().unwrap().contains_key(id) && start.elapsed() < Duration::from_secs(15) {
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Memory reaches the agent's instructions, is recorded on the run, and
    /// `<forge-memory>` proposals in the answer become pending suggestions.
    #[cfg(unix)]
    #[test]
    fn memory_is_injected_and_suggestions_recorded() {
        let (dir, db) = test_db();
        {
            let d = crate::db::Db::open(&db).unwrap();
            d.with(|c| {
                c.execute("INSERT INTO workspaces (id, name, path, created_at) VALUES ('other', 'o', '/tmp/forge-test-o', 1)", [])?;
                let mk = |scope: &str, ws: Option<&str>, title: &str| crate::memory::MemoryInput {
                    scope: scope.into(),
                    workspace_id: ws.map(String::from),
                    kind: "convention".into(),
                    title: title.into(),
                    body: format!("{title} body"),
                    ..Default::default()
                };
                crate::memory::create(c, &mk("workspace", Some("w"), "Use pnpm"))?;
                crate::memory::create(c, &mk("workspace", Some("other"), "SECRET-PROJECT-NOTE"))?;
                Ok(())
            })
            .unwrap();
        }
        let answer = serde_json::json!({
            "type": "result", "subtype": "success", "is_error": false, "duration_ms": 5, "num_turns": 1,
            "result": "Done. <forge-memory kind=\"error\" title=\"Port 3000 busy\">Dev server falls back to 3001.</forge-memory>"
        });
        let out = dir.path().join("result.jsonl");
        std::fs::write(&out, format!("{answer}\n")).unwrap();
        let args_file = dir.path().join("args.txt");
        let cli = fake_cli(dir.path(), &format!("printf '%s\\n' \"$@\" > '{}'; cat '{}'", args_file.display(), out.display()));
        let ws = tempfile::tempdir().unwrap();
        let mgr = RunManager::default();
        let run = mgr.start_with_program(&db, ws.path(), dir.path(), req("claude", "read"), Some(cli), None).unwrap();
        wait_for(&mgr, &run.id);
        let args = std::fs::read_to_string(&args_file).unwrap();
        assert!(args.contains("<memory>") && args.contains("Use pnpm body"), "{args}");
        assert!(!args.contains("SECRET-PROJECT-NOTE"), "other workspace leaked");
        let conn = open_db(&db).unwrap();
        let done = runs::get(&conn, &run.id).unwrap();
        assert_eq!(done.status, "completed", "{:?}", done.error);
        assert_eq!(done.memory, vec!["convention: Use pnpm".to_string()]);
        let pending = crate::memory::list(&conn, &crate::memory::MemoryFilter { status: Some("suggested".into()), ..Default::default() }).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].title, "Port 3000 busy");
        assert_eq!(pending[0].source_run_id.as_deref(), Some(run.id.as_str()));
        assert_eq!(pending[0].workspace_id.as_deref(), Some("w"));
    }

    #[cfg(unix)]
    #[test]
    fn end_to_end_run_with_replayed_stream() {
        let (dir, db) = test_db();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/agents/fixtures/claude_tools.jsonl");
        let cli = fake_cli(dir.path(), &format!("cat '{}'", fixture.display()));
        let ws = tempfile::tempdir().unwrap();
        let mgr = RunManager::default();
        let received = Arc::new(Mutex::new(Vec::<Value>::new()));
        let run = mgr.start_with_program(&db, ws.path(), dir.path(), req("claude", "read"), Some(cli), None).unwrap();
        let r2 = received.clone();
        // Attach immediately; events may already be persisted — snapshot + live must cover all.
        let att = mgr.attach(&db, &run.id, Arc::new(move |v| r2.lock().unwrap().push(v))).unwrap();
        wait_for(&mgr, &run.id);
        let conn = open_db(&db).unwrap();
        let done = runs::get(&conn, &run.id).unwrap();
        assert_eq!(done.status, "completed", "{:?}", done.error);
        assert_eq!(done.result_text.as_deref(), Some("a.txt, b.md"));
        assert_eq!(done.session_id.as_deref(), Some("00000000-0000-0000-0000-000000000000"));
        assert!(done.cost_usd.is_some() && done.duration_ms.is_some());
        let stored = runs::events(&conn, &run.id).unwrap();
        assert!(stored.iter().any(|e| e.event["kind"] == "tool_use"));
        assert!(stored.iter().all(|e| e.event["kind"] != "text_delta"), "deltas are not persisted");
        // Every persisted event was delivered either in the snapshot or live.
        let live_seqs: Vec<i64> = received.lock().unwrap().iter().filter_map(|m| m["seq"].as_i64()).collect();
        let snap_seqs: Vec<i64> = att.events.iter().map(|e| e.seq).collect();
        for e in &stored {
            assert!(live_seqs.contains(&e.seq) || snap_seqs.contains(&e.seq), "missing seq {}", e.seq);
        }
        assert!(received.lock().unwrap().iter().any(|m| m["type"] == "status"));
        // Finished runs can still be attached (from the database).
        let after = mgr.attach(&db, &run.id, Arc::new(|_| {})).unwrap();
        assert!(!after.live && after.token.is_none());
        assert_eq!(after.events.len(), stored.len());
    }

    #[cfg(unix)]
    #[test]
    fn cancel_kills_the_process_group() {
        let (dir, db) = test_db();
        // Prints a session event, then hangs with a child process.
        let cli = fake_cli(dir.path(), "echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"s\",\"model\":\"m\"}'; sleep 30 & wait");
        let ws = tempfile::tempdir().unwrap();
        let mgr = RunManager::default();
        let run = mgr.start_with_program(&db, ws.path(), dir.path(), req("claude", "read"), Some(cli), None).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let start = Instant::now();
        mgr.cancel(&run.id).unwrap();
        wait_for(&mgr, &run.id);
        assert!(start.elapsed() < Duration::from_secs(6));
        let conn = open_db(&db).unwrap();
        let done = runs::get(&conn, &run.id).unwrap();
        assert_eq!(done.status, "cancelled");
        assert_eq!(done.session_id.as_deref(), Some("s"));
        assert!(mgr.cancel(&run.id).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn failing_cli_reports_error() {
        let (dir, db) = test_db();
        let cli = fake_cli(dir.path(), "echo 'fatal: something broke' >&2; exit 3");
        let ws = tempfile::tempdir().unwrap();
        let mgr = RunManager::default();
        let run = mgr.start_with_program(&db, ws.path(), dir.path(), req("claude", "read"), Some(cli), None).unwrap();
        wait_for(&mgr, &run.id);
        let done = runs::get(&open_db(&db).unwrap(), &run.id).unwrap();
        assert_eq!(done.status, "failed");
        assert!(done.error.unwrap().contains("something broke"));
    }

    #[test]
    fn orphaned_runs_are_marked_failed() {
        let (_d, db) = test_db();
        let conn = open_db(&db).unwrap();
        conn.execute(
            "INSERT INTO agent_runs (id, provider_id, permission, prompt, status, cwd, started_at) VALUES ('o', 'claude', 'read', 'p', 'running', '/tmp', 1)",
            [],
        )
        .unwrap();
        assert_eq!(runs::mark_orphans(&conn).unwrap(), 1);
        assert_eq!(runs::get(&conn, "o").unwrap().status, "failed");
    }

    /// Real Claude Code run + continuation through the full pipeline.
    /// `cargo test agents::runner::tests::live -- --ignored --nocapture`
    #[cfg(unix)]
    #[test]
    #[ignore]
    fn live_claude_run_and_resume() {
        let (dir, db) = test_db();
        let ws = tempfile::tempdir().unwrap();
        std::fs::write(ws.path().join("notes.txt"), "the secret word is PINEAPPLE\n").unwrap();
        let mgr = RunManager::default();
        let mut r = req("claude", "read");
        r.model = Some("haiku".into());
        r.prompt = "Read notes.txt and reply with only the secret word.".into();
        let first = mgr.start(&db, ws.path(), dir.path(), r.clone()).unwrap();
        wait_for(&mgr, &first.id);
        let conn = open_db(&db).unwrap();
        let done = runs::get(&conn, &first.id).unwrap();
        eprintln!("first: status={} text={:?} err={:?} model={:?} tokens={:?}/{:?} cost={:?}", done.status, done.result_text, done.error, done.model, done.input_tokens, done.output_tokens, done.cost_usd);
        let kinds: Vec<String> = runs::events(&conn, &first.id).unwrap().iter().map(|e| e.event["kind"].as_str().unwrap_or("").to_string()).collect();
        eprintln!("events: {kinds:?}");
        assert_eq!(done.status, "completed");
        assert!(done.result_text.unwrap_or_default().contains("PINEAPPLE"));
        assert!(kinds.contains(&"tool_use".to_string()));

        let mut r2 = r;
        r2.prompt = "What was the secret word you just told me? Reply with only the word, lowercase.".into();
        r2.resume_session = done.session_id.clone();
        r2.parent_run_id = Some(first.id.clone());
        let second = mgr.start(&db, ws.path(), dir.path(), r2).unwrap();
        wait_for(&mgr, &second.id);
        let done2 = runs::get(&conn, &second.id).unwrap();
        eprintln!("second: status={} text={:?} err={:?}", done2.status, done2.result_text, done2.error);
        assert_eq!(done2.status, "completed");
        assert!(done2.result_text.unwrap_or_default().contains("pineapple"));
    }

    /// A linked skill changes the behavior of a real Claude Code run.
    /// `cargo test agents::runner::tests::live_skill -- --ignored --nocapture`
    /// Real Claude: an active workspace memory changes the answer, and a
    /// requested proposal becomes a pending suggestion.
    #[cfg(unix)]
    #[test]
    #[ignore]
    fn live_memory_is_used_and_suggested() {
        let (dir, db) = test_db();
        {
            let d = crate::db::Db::open(&db).unwrap();
            d.with(|c| {
                crate::memory::create(
                    c,
                    &crate::memory::MemoryInput {
                        scope: "workspace".into(),
                        workspace_id: Some("w".into()),
                        kind: "context".into(),
                        title: "Project codename".into(),
                        body: "The internal codename of this project is ZEPHYR-42.".into(),
                        ..Default::default()
                    },
                )?;
                Ok(())
            })
            .unwrap();
        }
        let ws = tempfile::tempdir().unwrap();
        let mgr = RunManager::default();
        let mut r = req("claude", "read");
        r.model = Some("haiku".into());
        r.prompt = "What is this project's internal codename? Answer in one line. Then propose exactly one project memory of kind convention titled \"Answer style\" saying answers should be one line.".into();
        let run = mgr.start(&db, ws.path(), dir.path(), r).unwrap();
        wait_for(&mgr, &run.id);
        let conn = open_db(&db).unwrap();
        let done = runs::get(&conn, &run.id).unwrap();
        eprintln!("status={} memory={:?} text={:?}", done.status, done.memory, done.result_text);
        assert_eq!(done.memory, vec!["context: Project codename".to_string()]);
        assert!(done.result_text.unwrap_or_default().contains("ZEPHYR-42"));
        let pending = crate::memory::list(&conn, &crate::memory::MemoryFilter { status: Some("suggested".into()), ..Default::default() }).unwrap();
        eprintln!("suggestions={:?}", pending.iter().map(|m| (&m.kind, &m.title, &m.body)).collect::<Vec<_>>());
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].kind, "convention");
    }

    #[cfg(unix)]
    #[test]
    #[ignore]
    fn live_skill_is_followed() {
        let (dir, db) = test_db();
        let composed = {
            let d = crate::db::Db::open(&db).unwrap();
            d.with(|c| {
                crate::agents::profiles::ensure_builtins(c, "claude")?;
                let skill = crate::skills::repo::create(
                    c,
                    &crate::skills::format::SkillDoc { name: "Signature".into(), description: "Signs answers".into(), body: "End every answer with the exact token FORGE-SKILL-OK on its own line.".into() },
                    "local",
                    None,
                    &[],
                )?;
                crate::skills::repo::set_link(c, &skill.id, "profile", "builtin:architect", true)?;
                crate::skills::repo::compose(c, Some("builtin:architect"), None, None)
            })
            .unwrap()
        };
        let ws = tempfile::tempdir().unwrap();
        let mgr = RunManager::default();
        let mut r = req("claude", "read");
        r.model = Some("haiku".into());
        r.prompt = "What is 2 + 2? Answer briefly.".into();
        r.instructions = crate::skills::repo::with_skills(None, &composed);
        r.skill_labels = composed.labels.clone();
        let run = mgr.start(&db, ws.path(), dir.path(), r).unwrap();
        wait_for(&mgr, &run.id);
        let done = runs::get(&open_db(&db).unwrap(), &run.id).unwrap();
        eprintln!("status={} skills={:?} text={:?}", done.status, done.skills, done.result_text);
        assert_eq!(done.skills, vec!["signature@v1".to_string()]);
        assert!(done.result_text.unwrap_or_default().contains("FORGE-SKILL-OK"));
    }

    /// Real Claude Code run using an attached remote MCP server (Cloudflare Docs).
    /// `cargo test agents::runner::tests::live_mcp -- --ignored --nocapture`
    #[cfg(unix)]
    #[test]
    #[ignore]
    fn live_mcp_tool_is_used() {
        let (dir, db) = test_db();
        let store = crate::mcp::secrets::MemoryStore::default();
        let (attach, notes) = {
            let d = crate::db::Db::open(&db).unwrap();
            d.with(|c| {
                crate::agents::profiles::ensure_builtins(c, "claude")?;
                let s = crate::mcp::repo::create_from_template(c, &store, "cloudflare-docs", None, &Default::default())?;
                crate::mcp::repo::set_link(c, &s.id, "profile", "builtin:researcher", true)?;
                crate::mcp::repo::prepare_run(c, &store, &dir.path().join("mcp-run"), "claude", "read", Some("builtin:researcher"), None, None)
            })
            .unwrap()
        };
        eprintln!("notes={notes:?}");
        let attach = attach.expect("mcp attached");
        let cfg_file = attach.config_file.clone();
        let ws = tempfile::tempdir().unwrap();
        let mgr = RunManager::default();
        let mut r = req("claude", "read");
        r.model = Some("haiku".into());
        r.instructions = None;
        r.prompt = "Use the cloudflare-docs MCP search tool to look up 'Workers KV'. Reply with MCP_OK followed by the title of one documentation page you found.".into();
        r.mcp = Some(attach);
        let run = mgr.start(&db, ws.path(), dir.path(), r).unwrap();
        wait_for(&mgr, &run.id);
        let conn = open_db(&db).unwrap();
        let done = runs::get(&conn, &run.id).unwrap();
        let tools: Vec<String> = runs::events(&conn, &run.id).unwrap().iter().filter(|e| e.event["kind"] == "tool_use").map(|e| e.event["name"].as_str().unwrap_or("").to_string()).collect();
        eprintln!("status={} mcp={:?} tools={tools:?} text={:?} err={:?}", done.status, done.mcp, done.result_text, done.error);
        assert_eq!(done.mcp, vec!["cloudflare-docs".to_string()]);
        assert!(tools.iter().any(|t| t.starts_with("mcp__cloudflare-docs__")), "an MCP tool was called");
        assert!(done.result_text.unwrap_or_default().contains("MCP_OK"));
        assert!(!cfg_file.exists(), "temporary MCP config removed");
    }
}
