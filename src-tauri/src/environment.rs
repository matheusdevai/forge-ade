//! Real detection of developer tools installed on this machine.
//!
//! GUI apps on macOS do not inherit the user's shell PATH, so we resolve the
//! login-shell PATH once and merge it with well-known install locations.
//! Nothing here is simulated: a tool is "installed" only if its binary is
//! found on PATH *and* answers its version command.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::Serialize;

pub struct ToolSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub bin: &'static str,
    pub args: &'static [&'static str],
    pub category: &'static str,
    pub required: bool,
    pub hint: &'static str,
}

pub const TOOLS: &[ToolSpec] = &[
    ToolSpec { id: "git", name: "Git", bin: "git", args: &["--version"], category: "core", required: true, hint: "https://git-scm.com/downloads" },
    ToolSpec { id: "node", name: "Node.js", bin: "node", args: &["--version"], category: "core", required: true, hint: "https://nodejs.org" },
    ToolSpec { id: "npm", name: "npm", bin: "npm", args: &["--version"], category: "core", required: false, hint: "Ships with Node.js" },
    ToolSpec { id: "rustc", name: "Rust", bin: "rustc", args: &["--version"], category: "core", required: false, hint: "https://rustup.rs" },
    ToolSpec { id: "cargo", name: "Cargo", bin: "cargo", args: &["--version"], category: "core", required: false, hint: "Ships with rustup" },
    ToolSpec { id: "claude", name: "Claude Code", bin: "claude", args: &["--version"], category: "agent", required: false, hint: "npm i -g @anthropic-ai/claude-code" },
    ToolSpec { id: "codex", name: "OpenAI Codex CLI", bin: "codex", args: &["--version"], category: "agent", required: false, hint: "npm i -g @openai/codex" },
    ToolSpec { id: "gemini", name: "Gemini CLI", bin: "gemini", args: &["--version"], category: "agent", required: false, hint: "npm i -g @google/gemini-cli" },
    ToolSpec { id: "opencode", name: "OpenCode", bin: "opencode", args: &["--version"], category: "agent", required: false, hint: "https://opencode.ai" },
    ToolSpec { id: "ollama", name: "Ollama", bin: "ollama", args: &["--version"], category: "local", required: false, hint: "https://ollama.com" },
    ToolSpec { id: "lms", name: "LM Studio CLI", bin: "lms", args: &["version"], category: "local", required: false, hint: "https://lmstudio.ai" },
    ToolSpec { id: "docker", name: "Docker", bin: "docker", args: &["--version"], category: "runtime", required: false, hint: "https://docs.docker.com/get-docker/" },
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub id: String,
    pub name: String,
    pub category: String,
    pub required: bool,
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub error: Option<String>,
    pub hint: String,
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Login-shell PATH merged with the process PATH and common install dirs.
pub fn search_path() -> &'static OsString {
    SEARCH_PATH.get_or_init(|| {
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(shell_path) = login_shell_path() {
            dirs.extend(std::env::split_paths(&shell_path));
        }
        if let Some(p) = std::env::var_os("PATH") {
            dirs.extend(std::env::split_paths(&p));
        }
        if let Some(home) = home_dir() {
            for rel in [".cargo/bin", ".local/bin", ".local/node/bin", ".bun/bin", ".volta/bin", ".npm-global/bin"] {
                dirs.push(home.join(rel));
            }
        }
        for d in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
            dirs.push(PathBuf::from(d));
        }
        let mut seen = HashSet::new();
        dirs.retain(|d| !d.as_os_str().is_empty() && seen.insert(d.clone()));
        std::env::join_paths(dirs).unwrap_or_default()
    })
}

#[cfg(unix)]
fn login_shell_path() -> Option<OsString> {
    const MARK: &str = "__FORGE_PATH__";
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let mut cmd = Command::new(shell);
    cmd.args(["-ilc", &format!("printf '{MARK}%s{MARK}' \"$PATH\"")]);
    let out = run_with_timeout(cmd, Duration::from_secs(4)).ok()??;
    let text = String::from_utf8_lossy(&out.stdout);
    let start = text.find(MARK)? + MARK.len();
    let end = start + text[start..].find(MARK)?;
    Some(OsString::from(&text[start..end]))
}

#[cfg(not(unix))]
fn login_shell_path() -> Option<OsString> {
    None
}

/// Full environment of the user's interactive login shell (e.g. API keys
/// exported in ~/.zshrc), so agent CLIs behave as in the user's terminal.
/// Held in memory only; never logged, persisted or sent to the frontend.
pub fn login_env() -> &'static HashMap<String, String> {
    static ENV: OnceLock<HashMap<String, String>> = OnceLock::new();
    ENV.get_or_init(|| {
        let mut env = capture_login_env().unwrap_or_default();
        if let Some(p) = search_path().to_str() {
            env.insert("PATH".into(), p.to_string());
        }
        env
    })
}

#[cfg(unix)]
fn capture_login_env() -> Option<HashMap<String, String>> {
    const MARK: &str = "__FORGE_ENV__";
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let mut cmd = Command::new(shell);
    cmd.args(["-ilc", &format!("printf '{MARK}'; env -0")]);
    let out = run_capture(cmd, Duration::from_secs(5), 1024 * 1024, 64 * 1024).ok()??;
    let raw = String::from_utf8_lossy(&out.stdout);
    let body = &raw[raw.find(MARK)? + MARK.len()..];
    let map: HashMap<String, String> = body
        .split('\0')
        .filter_map(|kv| kv.split_once('='))
        .filter(|(k, _)| !k.is_empty() && !k.contains('\n') && !matches!(*k, "PWD" | "OLDPWD" | "SHLVL" | "_"))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    (!map.is_empty()).then_some(map)
}

#[cfg(not(unix))]
fn capture_login_env() -> Option<HashMap<String, String>> {
    None
}

pub fn which(bin: &str) -> Option<PathBuf> {
    let exts: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ".bat", ""] } else { &[""] };
    for dir in std::env::split_paths(search_path()) {
        for ext in exts {
            let candidate = dir.join(format!("{bin}{ext}"));
            if is_executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata().map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(p: &Path) -> bool {
    p.is_file()
}

pub struct Output {
    pub status_ok: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Runs a command with stdin closed, killing it after `timeout`.
/// Returns `Ok(None)` on timeout.
pub fn run_with_timeout(cmd: Command, timeout: Duration) -> std::io::Result<Option<Output>> {
    run_capture(cmd, timeout, 64 * 1024, 64 * 1024)
}

/// Like `run_with_timeout`, with explicit output caps.
pub fn run_capture(mut cmd: Command, timeout: Duration, out_cap: u64, err_cap: u64) -> std::io::Result<Option<Output>> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if cmd.get_envs().all(|(k, _)| k != "PATH") {
        cmd.env("PATH", effective_path());
    }
    let mut child = cmd.spawn()?;
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let out_t = std::thread::spawn(move || {
        let mut b = Vec::new();
        if let Some(s) = stdout.as_mut() {
            let _ = s.take(out_cap).read_to_end(&mut b);
        }
        b
    });
    let err_t = std::thread::spawn(move || {
        let mut b = Vec::new();
        if let Some(s) = stderr.as_mut() {
            let _ = s.take(err_cap).read_to_end(&mut b);
        }
        b
    });
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let stdout = out_t.join().unwrap_or_default();
    let stderr = err_t.join().unwrap_or_default();
    Ok(status.map(|s| Output { status_ok: s.success(), stdout, stderr }))
}

static SEARCH_PATH: OnceLock<OsString> = OnceLock::new();

/// PATH handed to child processes. While the login-shell PATH is still being
/// resolved (the resolver itself spawns a shell) we fall back to the process PATH.
fn effective_path() -> OsString {
    SEARCH_PATH
        .get()
        .cloned()
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default()
}

/// Extracts the first `x.y[.z…]` version token from tool output.
pub fn parse_version(text: &str) -> Option<String> {
    text.split(|c: char| c.is_whitespace() || c == ',' || c == '(' || c == ')')
        .map(|t| t.trim_start_matches('v'))
        .find(|t| {
            let mut parts = t.split('.');
            let first_ok = parts.next().is_some_and(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
            first_ok && parts.next().is_some_and(|p| p.chars().next().is_some_and(|c| c.is_ascii_digit()))
        })
        .map(|t| t.trim_end_matches(|c: char| !c.is_ascii_alphanumeric()).to_string())
}

pub fn detect(spec: &ToolSpec) -> ToolStatus {
    let mut status = ToolStatus {
        id: spec.id.into(),
        name: spec.name.into(),
        category: spec.category.into(),
        required: spec.required,
        installed: false,
        version: None,
        path: None,
        error: None,
        hint: spec.hint.into(),
    };
    let Some(path) = which(spec.bin) else {
        return status;
    };
    status.path = Some(path.display().to_string());
    let mut cmd = Command::new(&path);
    cmd.args(spec.args);
    match run_with_timeout(cmd, Duration::from_secs(8)) {
        Ok(Some(out)) => {
            let text = format!(
                "{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            status.version = parse_version(&text);
            status.installed = out.status_ok || status.version.is_some();
            if !status.installed {
                status.error = Some("version command failed".into());
            }
        }
        Ok(None) => status.error = Some("version command timed out".into()),
        Err(e) => status.error = Some(format!("failed to run: {e}")),
    }
    status
}

pub fn detect_all() -> Vec<ToolStatus> {
    let _ = search_path();
    std::thread::scope(|s| {
        let handles: Vec<_> = TOOLS.iter().map(|t| s.spawn(move || detect(t))).collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_version_formats() {
        assert_eq!(parse_version("git version 2.54.0 (Apple Git-157)").as_deref(), Some("2.54.0"));
        assert_eq!(parse_version("v24.22.0\n").as_deref(), Some("24.22.0"));
        assert_eq!(parse_version("2.1.317 (Claude Code)").as_deref(), Some("2.1.317"));
        assert_eq!(parse_version("rustc 1.99.1 (58cf8f9c3 2026-10-13)").as_deref(), Some("1.99.1"));
        assert_eq!(parse_version("Docker version 27.3.1, build ce12230").as_deref(), Some("27.3.1"));
        assert_eq!(parse_version("codex-cli 0.48.0").as_deref(), Some("0.48.0"));
        assert_eq!(parse_version("no version here"), None);
    }

    #[test]
    fn which_finds_sh_and_misses_garbage() {
        #[cfg(unix)]
        assert!(which("sh").is_some());
        assert!(which("definitely-not-a-real-binary-forge").is_none());
    }

    #[test]
    fn detect_all_reports_every_tool() {
        let all = detect_all();
        assert_eq!(all.len(), TOOLS.len());
        for t in &all {
            eprintln!("{:<18} installed={:<5} version={:?}", t.name, t.installed, t.version);
        }
    }

    #[cfg(unix)]
    #[test]
    fn timeout_kills_long_running_process() {
        let mut cmd = Command::new("sleep");
        cmd.arg("5");
        let start = Instant::now();
        let out = run_with_timeout(cmd, Duration::from_millis(200)).unwrap();
        assert!(out.is_none());
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
