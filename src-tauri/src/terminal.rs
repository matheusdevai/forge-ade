//! Native terminal sessions backed by a real PTY (portable-pty).
//!
//! - Each session runs the user's shell inside a workspace directory.
//! - Output is decoded as UTF-8 (handling sequences split across reads),
//!   kept in an in-memory scrollback ring and streamed to the current
//!   subscriber (a Tauri channel in the app, a closure in tests).
//! - Sessions outlive their pane/tab while the app runs; a pane re-attaching
//!   receives the scrollback first.
//! - Input and output are NEVER logged or persisted to disk.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;

use crate::db::now_ms;
use crate::error::{AppError, AppResult};

pub const MAX_TERMINALS: usize = 32;
pub const SCROLLBACK_BYTES: usize = 512 * 1024;
const MAX_WRITE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TermEvent {
    Data { data: String },
    Exit { code: Option<i32> },
}

pub type Subscriber = Arc<dyn Fn(TermEvent) + Send + Sync>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    pub id: String,
    pub workspace_id: String,
    pub shell: String,
    pub cwd: String,
    pub pid: Option<u32>,
    pub cols: u16,
    pub rows: u16,
    pub created_at: i64,
    pub exited: bool,
    pub exit_code: Option<i32>,
    /// Agent provider running in this PTY (None = plain shell).
    pub provider_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachResult {
    pub info: TerminalInfo,
    pub scrollback: String,
    pub token: u64,
}

// ── UTF-8 streaming decoder ───────────────────────────────────────────────

/// Decodes a byte stream into valid UTF-8 strings, carrying incomplete
/// multi-byte sequences over to the next chunk.
#[derive(Default)]
pub struct Utf8Decoder {
    pending: Vec<u8>,
}

impl Utf8Decoder {
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut out = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(s) => {
                    out.push_str(s);
                    self.pending.clear();
                    return out;
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    out.push_str(std::str::from_utf8(&self.pending[..valid]).unwrap_or_default());
                    match e.error_len() {
                        // Incomplete sequence at the end: keep it for the next chunk.
                        None => {
                            self.pending.drain(..valid);
                            return out;
                        }
                        // Invalid bytes: replace and continue.
                        Some(n) => {
                            out.push('\u{FFFD}');
                            self.pending.drain(..valid + n);
                        }
                    }
                }
            }
        }
    }
}

// ── Scrollback ring ───────────────────────────────────────────────────────

pub struct Scrollback {
    buf: String,
    cap: usize,
}

impl Scrollback {
    pub fn new(cap: usize) -> Self {
        Self { buf: String::new(), cap }
    }

    pub fn push(&mut self, s: &str) {
        self.buf.push_str(s);
        if self.buf.len() > self.cap {
            let mut cut = self.buf.len() - self.cap;
            while !self.buf.is_char_boundary(cut) {
                cut += 1;
            }
            // Prefer cutting at a line break to avoid half-rendered escape sequences.
            if let Some(nl) = self.buf[cut..].find('\n') {
                if nl < 4096 {
                    cut += nl + 1;
                }
            }
            self.buf.drain(..cut);
        }
    }

    pub fn snapshot(&self) -> String {
        self.buf.clone()
    }
}

// ── Sessions ──────────────────────────────────────────────────────────────

struct Session {
    info: Mutex<TerminalInfo>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    scrollback: Mutex<Scrollback>,
    subscriber: Mutex<Option<(u64, Subscriber)>>,
}

impl Session {
    fn emit(&self, ev: TermEvent) {
        let sub = self.subscriber.lock().ok().and_then(|s| s.as_ref().map(|(_, f)| f.clone()));
        if let Some(f) = sub {
            f(ev);
        }
    }
}

/// How to launch the process inside the PTY.
pub struct SpawnSpec {
    pub workspace_id: String,
    pub cwd: PathBuf,
    pub cols: u16,
    pub rows: u16,
    /// `None` = the user's default shell as a login shell.
    pub program: Option<(String, Vec<String>)>,
    pub path_env: Option<std::ffi::OsString>,
    /// Extra environment (e.g. the user's login-shell env for agent CLIs).
    pub extra_env: Option<HashMap<String, String>>,
    pub provider_id: Option<String>,
}

#[derive(Default)]
pub struct TerminalManager {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    tokens: AtomicU64,
}

pub fn default_shell() -> (String, Vec<String>) {
    #[cfg(windows)]
    {
        let shell = std::env::var("COMSPEC").unwrap_or_else(|_| "powershell.exe".into());
        (shell, vec![])
    }
    #[cfg(not(windows))]
    {
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|s| Path::new(s).exists())
            .unwrap_or_else(|| if cfg!(target_os = "macos") { "/bin/zsh".into() } else { "/bin/bash".into() });
        (shell, vec!["-l".into()])
    }
}

fn clamp_size(cols: u16, rows: u16) -> (u16, u16) {
    (cols.clamp(10, 1000), rows.clamp(2, 500))
}

fn pty_err(e: impl std::fmt::Display) -> AppError {
    AppError::Internal(format!("pty: {e}"))
}

impl TerminalManager {
    fn get(&self, id: &str) -> AppResult<Arc<Session>> {
        self.sessions
            .lock()
            .map_err(|_| AppError::Internal("terminal lock poisoned".into()))?
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("terminal not found: {id}")))
    }

    pub fn spawn(&self, spec: SpawnSpec) -> AppResult<TerminalInfo> {
        {
            let sessions = self.sessions.lock().map_err(|_| AppError::Internal("terminal lock poisoned".into()))?;
            let live = sessions.values().filter(|s| s.info.lock().map(|i| !i.exited).unwrap_or(false)).count();
            if live >= MAX_TERMINALS {
                return Err(AppError::Invalid(format!("limit of {MAX_TERMINALS} open terminals reached")));
            }
        }
        if !spec.cwd.is_dir() {
            return Err(AppError::NotFound(format!("directory not found: {}", spec.cwd.display())));
        }
        let (cols, rows) = clamp_size(spec.cols, spec.rows);
        let pair = native_pty_system()
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(pty_err)?;

        let (program, args) = spec.program.unwrap_or_else(default_shell);
        let mut cmd = CommandBuilder::new(&program);
        cmd.args(&args);
        cmd.cwd(&spec.cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "FORGE");
        cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
        if std::env::var_os("LANG").is_none() {
            cmd.env("LANG", "en_US.UTF-8");
        }
        if let Some(env) = &spec.extra_env {
            for (k, v) in env {
                cmd.env(k, v);
            }
        }
        if let Some(p) = &spec.path_env {
            cmd.env("PATH", p);
        }

        let child = pair.slave.spawn_command(cmd).map_err(pty_err)?;
        drop(pair.slave);
        let reader = pair.master.try_clone_reader().map_err(pty_err)?;
        let writer = pair.master.take_writer().map_err(pty_err)?;
        let killer = child.clone_killer();

        let id = crate::db::new_id();
        let info = TerminalInfo {
            id: id.clone(),
            workspace_id: spec.workspace_id,
            shell: program,
            cwd: spec.cwd.display().to_string(),
            pid: child.process_id(),
            cols,
            rows,
            created_at: now_ms(),
            exited: false,
            exit_code: None,
            provider_id: spec.provider_id.clone(),
        };
        let session = Arc::new(Session {
            info: Mutex::new(info.clone()),
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            killer: Mutex::new(killer),
            scrollback: Mutex::new(Scrollback::new(SCROLLBACK_BYTES)),
            subscriber: Mutex::new(None),
        });
        self.sessions
            .lock()
            .map_err(|_| AppError::Internal("terminal lock poisoned".into()))?
            .insert(id.clone(), session.clone());

        std::thread::Builder::new()
            .name(format!("forge-pty-{}", &id[..8]))
            .spawn(move || pump(session, reader, child))
            .map_err(AppError::Io)?;
        Ok(info)
    }

    /// Sets the (single) subscriber and returns the scrollback so far.
    /// A newer attach replaces the previous subscriber.
    pub fn attach(&self, id: &str, subscriber: Subscriber) -> AppResult<AttachResult> {
        let s = self.get(id)?;
        let token = self.tokens.fetch_add(1, Ordering::Relaxed) + 1;
        // Hold the scrollback lock while swapping the subscriber so no chunk
        // is lost or duplicated between snapshot and live stream.
        let sb = s.scrollback.lock().map_err(|_| AppError::Internal("lock poisoned".into()))?;
        *s.subscriber.lock().map_err(|_| AppError::Internal("lock poisoned".into()))? = Some((token, subscriber));
        let scrollback = sb.snapshot();
        drop(sb);
        let info = s.info.lock().map_err(|_| AppError::Internal("lock poisoned".into()))?.clone();
        Ok(AttachResult { info, scrollback, token })
    }

    /// Clears the subscriber only if `token` still owns it.
    pub fn detach(&self, id: &str, token: u64) -> AppResult<()> {
        let s = self.get(id)?;
        let mut sub = s.subscriber.lock().map_err(|_| AppError::Internal("lock poisoned".into()))?;
        if sub.as_ref().is_some_and(|(t, _)| *t == token) {
            *sub = None;
        }
        Ok(())
    }

    pub fn write(&self, id: &str, data: &str) -> AppResult<()> {
        if data.len() > MAX_WRITE_BYTES {
            return Err(AppError::Invalid("input chunk too large".into()));
        }
        let s = self.get(id)?;
        if s.info.lock().map(|i| i.exited).unwrap_or(true) {
            return Err(AppError::Invalid("terminal has exited".into()));
        }
        let mut w = s.writer.lock().map_err(|_| AppError::Internal("lock poisoned".into()))?;
        w.write_all(data.as_bytes())?;
        w.flush()?;
        Ok(())
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> AppResult<()> {
        let s = self.get(id)?;
        let (cols, rows) = clamp_size(cols, rows);
        s.master
            .lock()
            .map_err(|_| AppError::Internal("lock poisoned".into()))?
            .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(pty_err)?;
        if let Ok(mut i) = s.info.lock() {
            i.cols = cols;
            i.rows = rows;
        }
        Ok(())
    }

    /// Terminates the process (if running) and forgets the session.
    pub fn kill(&self, id: &str) -> AppResult<()> {
        let s = self
            .sessions
            .lock()
            .map_err(|_| AppError::Internal("terminal lock poisoned".into()))?
            .remove(id)
            .ok_or_else(|| AppError::NotFound(format!("terminal not found: {id}")))?;
        if !s.info.lock().map(|i| i.exited).unwrap_or(true) {
            let _ = s.killer.lock().map(|mut k| k.kill());
        }
        if let Ok(mut sub) = s.subscriber.lock() {
            *sub = None;
        }
        Ok(())
    }

    pub fn list(&self, workspace_id: Option<&str>) -> Vec<TerminalInfo> {
        let Ok(sessions) = self.sessions.lock() else { return vec![] };
        let mut out: Vec<TerminalInfo> = sessions
            .values()
            .filter_map(|s| s.info.lock().ok().map(|i| i.clone()))
            .filter(|i| workspace_id.is_none_or(|w| i.workspace_id == w))
            .collect();
        out.sort_by_key(|i| i.created_at);
        out
    }

    pub fn kill_workspace(&self, workspace_id: &str) {
        for t in self.list(Some(workspace_id)) {
            let _ = self.kill(&t.id);
        }
    }

    pub fn kill_all(&self) {
        for t in self.list(None) {
            let _ = self.kill(&t.id);
        }
    }
}

/// Reads PTY output until EOF, then records the exit status.
fn pump(session: Arc<Session>, mut reader: Box<dyn Read + Send>, mut child: Box<dyn portable_pty::Child + Send + Sync>) {
    let mut decoder = Utf8Decoder::default();
    let mut buf = [0u8; 16 * 1024];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let text = decoder.push(&buf[..n]);
                if text.is_empty() {
                    continue;
                }
                // Append + emit under the scrollback lock (see `attach`).
                if let Ok(mut sb) = session.scrollback.lock() {
                    sb.push(&text);
                    session.emit(TermEvent::Data { data: text });
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    let code = child.wait().ok().map(|s| s.exit_code() as i32);
    if let Ok(mut i) = session.info.lock() {
        i.exited = true;
        i.exit_code = code;
    }
    session.emit(TermEvent::Exit { code });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn decoder_handles_split_multibyte_and_invalid_bytes() {
        let mut d = Utf8Decoder::default();
        let s = "olá ✓ 🚀".as_bytes();
        let mut out = String::new();
        for b in s {
            out.push_str(&d.push(std::slice::from_ref(b)));
        }
        assert_eq!(out, "olá ✓ 🚀");
        assert_eq!(d.push(&[b'a', 0xFF, b'b']), "a\u{FFFD}b");
    }

    #[test]
    fn scrollback_is_bounded_and_char_safe() {
        let mut sb = Scrollback::new(64);
        for i in 0..100 {
            sb.push(&format!("linha {i} ✓\n"));
        }
        let snap = sb.snapshot();
        assert!(snap.len() <= 64);
        assert!(snap.ends_with("linha 99 ✓\n"));
        assert!(snap.starts_with("linha"), "cut at a line boundary: {snap:?}");
    }

    fn wait_until(timeout: Duration, mut f: impl FnMut() -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if f() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[cfg(unix)]
    #[test]
    fn real_pty_roundtrip_resize_reattach_and_exit() {
        let mgr = TerminalManager::default();
        let dir = tempfile::tempdir().unwrap();
        let info = mgr
            .spawn(SpawnSpec {
                workspace_id: "w".into(),
                cwd: dir.path().to_path_buf(),
                cols: 80,
                rows: 24,
                program: Some(("/bin/sh".into(), vec![])),
                path_env: None,
                extra_env: None,
                provider_id: None,
            })
            .unwrap();
        assert!(info.pid.is_some());

        let out = Arc::new(Mutex::new(String::new()));
        let exits = Arc::new(Mutex::new(Vec::new()));
        let (o, e) = (out.clone(), exits.clone());
        let a = mgr
            .attach(&info.id, Arc::new(move |ev| match ev {
                TermEvent::Data { data } => o.lock().unwrap().push_str(&data),
                TermEvent::Exit { code } => e.lock().unwrap().push(code),
            }))
            .unwrap();

        mgr.write(&info.id, "echo forge_$((40+2)); pwd\n").unwrap();
        assert!(wait_until(Duration::from_secs(5), || out.lock().unwrap().contains("forge_42")));
        let canonical = std::fs::canonicalize(dir.path()).unwrap();
        assert!(wait_until(Duration::from_secs(5), || {
            let o = out.lock().unwrap();
            o.contains(&*canonical.to_string_lossy()) || o.contains(&*dir.path().to_string_lossy())
        }));

        mgr.resize(&info.id, 120, 40).unwrap();
        mgr.write(&info.id, "stty size\n").unwrap();
        assert!(wait_until(Duration::from_secs(5), || out.lock().unwrap().contains("40 120")));

        // Re-attach: the scrollback contains everything printed so far.
        let b = mgr.attach(&info.id, Arc::new(|_| {})).unwrap();
        assert!(b.scrollback.contains("forge_42"));
        assert_ne!(a.token, b.token);
        mgr.detach(&info.id, a.token).unwrap(); // stale token: no effect
        assert_eq!(mgr.list(Some("w")).len(), 1);
        assert!(mgr.list(Some("other")).is_empty());

        // Exit is reported to the current subscriber.
        let e2 = exits.clone();
        mgr.attach(&info.id, Arc::new(move |ev| {
            if let TermEvent::Exit { code } = ev {
                e2.lock().unwrap().push(code);
            }
        }))
        .unwrap();
        mgr.write(&info.id, "exit 3\n").unwrap();
        assert!(wait_until(Duration::from_secs(5), || !exits.lock().unwrap().is_empty()));
        assert_eq!(exits.lock().unwrap()[0], Some(3));
        let listed = &mgr.list(None)[0];
        assert!(listed.exited);
        assert!(mgr.write(&info.id, "x").is_err());
        mgr.kill(&info.id).unwrap();
        assert!(mgr.list(None).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn kill_terminates_running_process() {
        let mgr = TerminalManager::default();
        let dir = tempfile::tempdir().unwrap();
        let info = mgr
            .spawn(SpawnSpec {
                workspace_id: "w".into(),
                cwd: dir.path().to_path_buf(),
                cols: 80,
                rows: 24,
                program: Some(("/bin/sh".into(), vec!["-c".into(), "sleep 30".into()])),
                path_env: None,
                extra_env: None,
                provider_id: None,
            })
            .unwrap();
        let pid = info.pid.unwrap() as i32;
        mgr.kill_workspace("w");
        assert!(mgr.list(None).is_empty());
        // The process must be gone (kill(pid, 0) fails once it is reaped or dead).
        let gone = wait_until(Duration::from_secs(5), || {
            let status = std::process::Command::new("kill").args(["-0", &pid.to_string()]).status();
            status.map(|s| !s.success()).unwrap_or(true)
        });
        assert!(gone, "process {pid} still alive");
    }

    #[test]
    fn rejects_missing_cwd() {
        let mgr = TerminalManager::default();
        let r = mgr.spawn(SpawnSpec {
            workspace_id: "w".into(),
            cwd: PathBuf::from("/definitely/not/here"),
            cols: 80,
            rows: 24,
            program: None,
            path_env: None,
            extra_env: None,
            provider_id: None,
        });
        assert!(matches!(r, Err(AppError::NotFound(_))));
    }
}
