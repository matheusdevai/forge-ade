//! Git integration through the user's own `git` binary.
//!
//! - Commands are spawned directly (no shell), always with `--` before paths.
//! - Never interactive: `GIT_TERMINAL_PROMPT=0`, SSH in BatchMode, no editor.
//!   Authentication comes from the user's existing git setup (credential
//!   helpers, keychain, SSH agent) — FORGE never sees or stores credentials.
//! - Error output is redacted (`https://user:token@host` → `https://***@host`).
//! - Every command runs against the repository top-level; paths in results are
//!   repo-relative, plus the workspace `prefix` to map them back.

use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use regex::Regex;
use serde::Serialize;

use crate::environment;
use crate::error::{AppError, AppResult};

const LOCAL_TIMEOUT: Duration = Duration::from_secs(60);
const NETWORK_TIMEOUT: Duration = Duration::from_secs(180);
const MAX_STDOUT: u64 = 32 * 1024 * 1024;
const MAX_STDERR: u64 = 256 * 1024;
pub const MAX_BLOB_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_STATUS_FILES: usize = 5000;

// ── Runner ────────────────────────────────────────────────────────────────

pub struct GitOutput {
    pub stdout: Vec<u8>,
    pub stderr: String,
    pub success: bool,
}

/// Masks credentials embedded in URLs.
pub fn redact(s: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?i)\b([a-z][a-z0-9+.\-]*://)[^/\s@]+@").unwrap());
    re.replace_all(s, "${1}***@").into_owned()
}

fn git_bin() -> AppResult<PathBuf> {
    environment::which("git").ok_or_else(|| AppError::NotFound("git não encontrado no PATH".into()))
}

pub fn run(cwd: &Path, args: &[&str], stdin: Option<&[u8]>, timeout: Duration) -> AppResult<GitOutput> {
    let mut cmd = Command::new(git_bin()?);
    cmd.current_dir(cwd)
        .args(["-c", "core.quotepath=false", "-c", "color.ui=false", "-c", "core.pager=cat"])
        .args(args)
        .env("PATH", environment::search_path())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .env("GIT_EDITOR", "true")
        .env("GIT_SEQUENCE_EDITOR", "true")
        .env("GIT_PAGER", "cat")
        .env("LC_ALL", "C")
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        cmd.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
    }
    let mut child = cmd.spawn()?;
    if let (Some(data), Some(mut sin)) = (stdin, child.stdin.take()) {
        let data = data.to_vec();
        std::thread::spawn(move || {
            let _ = sin.write_all(&data);
        });
    }
    let mut out = child.stdout.take().expect("piped");
    let mut err = child.stderr.take().expect("piped");
    let out_t = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = (&mut out).take(MAX_STDOUT).read_to_end(&mut b);
        b
    });
    let err_t = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = (&mut err).take(MAX_STDERR).read_to_end(&mut b);
        b
    });
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AppError::Internal(format!("git {} excedeu o tempo limite", args.first().unwrap_or(&""))));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = out_t.join().unwrap_or_default();
    let stderr = redact(&String::from_utf8_lossy(&err_t.join().unwrap_or_default()));
    Ok(GitOutput { stdout, stderr, success: status.success() })
}

fn git_error(out: &GitOutput) -> AppError {
    let msg = out.stderr.trim();
    let msg = if msg.is_empty() { redact(String::from_utf8_lossy(&out.stdout).trim()) } else { msg.to_string() };
    AppError::Git(msg.lines().take(20).collect::<Vec<_>>().join("\n"))
}

fn ok(cwd: &Path, args: &[&str]) -> AppResult<Vec<u8>> {
    ok_with(cwd, args, None, LOCAL_TIMEOUT)
}

fn ok_with(cwd: &Path, args: &[&str], stdin: Option<&[u8]>, timeout: Duration) -> AppResult<Vec<u8>> {
    let out = run(cwd, args, stdin, timeout)?;
    if out.success {
        Ok(out.stdout)
    } else {
        Err(git_error(&out))
    }
}

fn text(b: Vec<u8>) -> String {
    String::from_utf8_lossy(&b).into_owned()
}

// ── Validation ────────────────────────────────────────────────────────────

/// Repo-relative path: no absolute paths, no `..`.
pub fn check_path(p: &str) -> AppResult<()> {
    if p.is_empty() || p.contains('\0') || Path::new(p).components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(AppError::Forbidden(format!("caminho inválido: {p}")));
    }
    Ok(())
}

/// Revisions/refs coming from the UI: conservative charset, never an option.
pub fn check_rev(r: &str) -> AppResult<()> {
    let okc = |c: char| c.is_ascii_alphanumeric() || "_./~^@{}-+".contains(c);
    if r.is_empty() || r.len() > 255 || r.starts_with('-') || !r.chars().all(okc) || r.contains("..") {
        return Err(AppError::Invalid(format!("referência inválida: {r}")));
    }
    Ok(())
}

pub fn check_branch_name(repo: &Path, name: &str) -> AppResult<()> {
    if name.starts_with('-') {
        return Err(AppError::Invalid(format!("nome de branch inválido: {name}")));
    }
    let out = run(repo, &["check-ref-format", "--branch", name], None, LOCAL_TIMEOUT)?;
    if out.success {
        Ok(())
    } else {
        Err(AppError::Invalid(format!("nome de branch inválido: {name}")))
    }
}

// ── Repository discovery ──────────────────────────────────────────────────

pub struct Repo {
    pub top: PathBuf,
    /// Workspace dir relative to the repo top-level ("" or "sub/dir/").
    pub prefix: String,
}

pub fn discover(workspace: &Path) -> AppResult<Option<Repo>> {
    let out = run(workspace, &["rev-parse", "--show-toplevel", "--show-prefix"], None, LOCAL_TIMEOUT)?;
    if !out.success {
        return Ok(None);
    }
    let s = text(out.stdout);
    let mut lines = s.lines();
    let top = PathBuf::from(lines.next().unwrap_or_default().trim());
    let prefix = lines.next().unwrap_or_default().trim().to_string();
    if top.as_os_str().is_empty() {
        return Ok(None);
    }
    Ok(Some(Repo { top, prefix }))
}

pub fn require(workspace: &Path) -> AppResult<Repo> {
    discover(workspace)?.ok_or_else(|| AppError::Invalid("o workspace não é um repositório Git".into()))
}

/// HEAD commit of the repository containing `dir` (None if not a repo / no commits).
pub fn head_commit(dir: &Path) -> Option<String> {
    let out = run(dir, &["rev-parse", "--verify", "-q", "HEAD"], None, LOCAL_TIMEOUT).ok()?;
    out.success.then(|| text(out.stdout).trim().to_string()).filter(|s| !s.is_empty())
}

/// Commits made after `base` (a full or short SHA) on the current HEAD.
pub fn commits_since(dir: &Path, base: &str) -> AppResult<Vec<Commit>> {
    if base.len() < 7 || base.len() > 64 || !base.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::Invalid("commit base inválido".into()));
    }
    let repo = require(dir)?;
    let range = format!("{base}..HEAD");
    let s = text(ok(&repo.top, &["log", "--max-count=200", "--format=%H%x1f%h%x1f%an%x1f%ae%x1f%at%x1f%D%x1f%s%x1e", &range, "--"])?);
    Ok(parse_log(&s))
}

/// Stages everything and commits if the tree has changes. Returns the new SHA, if any.
pub fn commit_all_if_dirty(dir: &Path, message: &str) -> AppResult<Option<String>> {
    let repo = require(dir)?;
    ok(&repo.top, &["add", "-A"])?;
    let staged = run(&repo.top, &["diff", "--cached", "--quiet"], None, LOCAL_TIMEOUT)?;
    if staged.success {
        return Ok(None); // nothing to commit
    }
    ok_with(&repo.top, &["commit", "-F", "-"], Some(message.as_bytes()), Duration::from_secs(120))?;
    Ok(head_commit(&repo.top))
}

/// Current branch name (None when detached or not a repo).
pub fn current_branch(dir: &Path) -> Option<String> {
    let out = run(dir, &["symbolic-ref", "--short", "-q", "HEAD"], None, LOCAL_TIMEOUT).ok()?;
    out.success.then(|| text(out.stdout).trim().to_string()).filter(|s| !s.is_empty())
}

/// Commits reachable from `rev` but not from `base` (base is a SHA).
pub fn commits_between(dir: &Path, base: &str, rev: &str) -> AppResult<Vec<Commit>> {
    if base.len() < 7 || base.len() > 64 || !base.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::Invalid("commit base inválido".into()));
    }
    check_rev(rev)?;
    let repo = require(dir)?;
    let range = format!("{base}..{rev}");
    let s = text(ok(&repo.top, &["log", "--max-count=200", "--format=%H%x1f%h%x1f%an%x1f%ae%x1f%at%x1f%D%x1f%s%x1e", &range, "--"])?);
    Ok(parse_log(&s))
}

pub fn init(workspace: &Path) -> AppResult<()> {
    ok(workspace, &["init"]).map(|_| ())
}

// ── Status ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileStatus {
    pub path: String,
    pub orig_path: Option<String>,
    /// Index (staged) status letter: M A D R C T U or '.'.
    pub index: char,
    /// Worktree status letter, '?' for untracked.
    pub worktree: char,
    pub conflicted: bool,
    pub untracked: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub is_repo: bool,
    pub prefix: String,
    pub top: String,
    pub branch: Option<String>,
    pub oid: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub files: Vec<FileStatus>,
    pub truncated: bool,
    pub merging: bool,
    pub rebasing: bool,
    pub has_commits: bool,
}

pub fn parse_status_v2(raw: &[u8]) -> GitStatus {
    let mut st = GitStatus { is_repo: true, ..Default::default() };
    let s = String::from_utf8_lossy(raw);
    let mut parts = s.split('\0').filter(|p| !p.is_empty()).peekable();
    while let Some(rec) = parts.next() {
        if let Some(h) = rec.strip_prefix("# ") {
            let (k, v) = h.split_once(' ').unwrap_or((h, ""));
            match k {
                "branch.oid" => {
                    st.has_commits = v != "(initial)";
                    st.oid = st.has_commits.then(|| v.to_string());
                }
                "branch.head" => st.branch = (v != "(detached)").then(|| v.to_string()),
                "branch.upstream" => st.upstream = Some(v.to_string()),
                "branch.ab" => {
                    for t in v.split_whitespace() {
                        if let Some(n) = t.strip_prefix('+') {
                            st.ahead = n.parse().unwrap_or(0);
                        } else if let Some(n) = t.strip_prefix('-') {
                            st.behind = n.parse().unwrap_or(0);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        if st.files.len() >= MAX_STATUS_FILES {
            st.truncated = true;
            break;
        }
        let kind = rec.chars().next().unwrap_or(' ');
        match kind {
            '1' | '2' | 'u' => {
                let nfields = match kind {
                    '1' => 8,
                    '2' => 9,
                    _ => 10,
                };
                let fields: Vec<&str> = rec.splitn(nfields + 1, ' ').collect();
                let xy: Vec<char> = fields.get(1).unwrap_or(&"..").chars().collect();
                let path = fields.get(nfields).unwrap_or(&"").to_string();
                let orig_path = if kind == '2' { parts.next().map(str::to_string) } else { None };
                st.files.push(FileStatus {
                    path,
                    orig_path,
                    index: *xy.first().unwrap_or(&'.'),
                    worktree: *xy.get(1).unwrap_or(&'.'),
                    conflicted: kind == 'u',
                    untracked: false,
                });
            }
            '?' => st.files.push(FileStatus {
                path: rec[2..].to_string(),
                orig_path: None,
                index: '.',
                worktree: '?',
                conflicted: false,
                untracked: true,
            }),
            _ => {}
        }
    }
    st
}

pub fn status(workspace: &Path) -> AppResult<GitStatus> {
    let Some(repo) = discover(workspace)? else {
        return Ok(GitStatus::default());
    };
    let raw = ok_with(
        &repo.top,
        &["--no-optional-locks", "status", "--porcelain=v2", "--branch", "-z", "--untracked-files=all"],
        None,
        LOCAL_TIMEOUT,
    )?;
    let mut st = parse_status_v2(&raw);
    st.prefix = repo.prefix.clone();
    st.top = repo.top.display().to_string();
    let git_dir = PathBuf::from(text(ok(&repo.top, &["rev-parse", "--absolute-git-dir"])?).trim());
    st.merging = git_dir.join("MERGE_HEAD").exists();
    st.rebasing = git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists();
    Ok(st)
}

// ── File contents (for diffs) ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitBlob {
    pub exists: bool,
    pub binary: bool,
    pub too_large: bool,
    pub content: Option<String>,
}

fn blob_from(bytes: Vec<u8>) -> GitBlob {
    if bytes.len() > MAX_BLOB_BYTES {
        return GitBlob { exists: true, binary: false, too_large: true, content: None };
    }
    let binary = bytes[..bytes.len().min(8192)].contains(&0);
    let content = if binary { None } else { Some(String::from_utf8_lossy(&bytes).into_owned()) };
    GitBlob { exists: true, binary, too_large: false, content }
}

/// `rev`: "WORKTREE" (file on disk), "INDEX" (staged), or a commit-ish.
pub fn file_contents(workspace: &Path, rev: &str, path: &str) -> AppResult<GitBlob> {
    check_path(path)?;
    let repo = require(workspace)?;
    let missing = GitBlob { exists: false, binary: false, too_large: false, content: Some(String::new()) };
    if rev == "WORKTREE" {
        let full = match crate::workspace_fs::resolve(&repo.top, path) {
            Ok(p) => p,
            Err(AppError::NotFound(_)) => return Ok(missing),
            Err(e) => return Err(e),
        };
        if full.is_dir() {
            return Ok(missing);
        }
        return Ok(blob_from(std::fs::read(full)?));
    }
    let spec = if rev == "INDEX" {
        format!(":{path}")
    } else {
        check_rev(rev)?;
        format!("{rev}:{path}")
    };
    let out = run(&repo.top, &["cat-file", "blob", &spec], None, LOCAL_TIMEOUT)?;
    if !out.success {
        return Ok(missing);
    }
    Ok(blob_from(out.stdout))
}

// ── Staging ───────────────────────────────────────────────────────────────

fn with_paths<'a>(base: &[&'a str], paths: &'a [String]) -> AppResult<Vec<&'a str>> {
    for p in paths {
        check_path(p)?;
    }
    let mut args: Vec<&str> = base.to_vec();
    args.push("--");
    args.extend(paths.iter().map(String::as_str));
    Ok(args)
}

pub fn stage(workspace: &Path, paths: &[String]) -> AppResult<()> {
    let repo = require(workspace)?;
    ok(&repo.top, &with_paths(&["add", "-A"], paths)?).map(|_| ())
}

pub fn unstage(workspace: &Path, paths: &[String]) -> AppResult<()> {
    let repo = require(workspace)?;
    let has_head = run(&repo.top, &["rev-parse", "--verify", "-q", "HEAD"], None, LOCAL_TIMEOUT)?.success;
    let args = if has_head {
        with_paths(&["restore", "--staged"], paths)?
    } else {
        with_paths(&["rm", "--cached", "-r", "-q"], paths)?
    };
    ok(&repo.top, &args).map(|_| ())
}

/// Discards worktree changes. Tracked files are restored from the index;
/// untracked files are moved to the OS trash (recoverable).
pub fn discard(workspace: &Path, paths: &[String]) -> AppResult<()> {
    let repo = require(workspace)?;
    let st = status(workspace)?;
    let (untracked, tracked): (Vec<String>, Vec<String>) = paths
        .iter()
        .cloned()
        .partition(|p| st.files.iter().any(|f| &f.path == p && f.untracked));
    if !tracked.is_empty() {
        ok(&repo.top, &with_paths(&["restore", "--worktree"], &tracked)?)?;
    }
    for p in untracked {
        check_path(&p)?;
        let full = crate::workspace_fs::resolve_new(&repo.top, &p)?;
        trash::delete(&full).map_err(|e| AppError::Internal(format!("trash: {e}")))?;
    }
    Ok(())
}

/// Writes `content` as the staged (index) version of `path`, without touching
/// the working tree. Enables staging/unstaging selected lines.
pub fn stage_content(workspace: &Path, path: &str, content: &str) -> AppResult<()> {
    check_path(path)?;
    let repo = require(workspace)?;
    let ls = text(ok(&repo.top, &["ls-files", "-s", "--", path])?);
    let mode = match ls.split_whitespace().next() {
        Some(m) => m.to_string(),
        None => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let exec = std::fs::metadata(repo.top.join(path)).map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false);
                if exec { "100755".into() } else { "100644".into() }
            }
            #[cfg(not(unix))]
            {
                "100644".to_string()
            }
        }
    };
    let oid = text(ok_with(&repo.top, &["hash-object", "-w", "--stdin", "--path", path], Some(content.as_bytes()), LOCAL_TIMEOUT)?);
    let cacheinfo = format!("{mode},{},{path}", oid.trim());
    ok(&repo.top, &["update-index", "--add", "--cacheinfo", &cacheinfo]).map(|_| ())
}

// ── Commit & history ──────────────────────────────────────────────────────

pub fn commit(workspace: &Path, message: &str, amend: bool) -> AppResult<String> {
    let repo = require(workspace)?;
    if message.trim().is_empty() {
        return Err(AppError::Invalid("a mensagem do commit não pode ficar vazia".into()));
    }
    let mut args = vec!["commit", "--cleanup=strip", "-F", "-"];
    if amend {
        args.push("--amend");
    }
    ok_with(&repo.top, &args, Some(message.as_bytes()), Duration::from_secs(300))?;
    Ok(text(ok(&repo.top, &["rev-parse", "HEAD"])?).trim().to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub sha: String,
    pub short: String,
    pub author: String,
    pub email: String,
    pub time: i64,
    pub refs: Vec<String>,
    pub subject: String,
}

pub fn log(workspace: &Path, limit: u32, rev: Option<&str>) -> AppResult<Vec<Commit>> {
    let repo = require(workspace)?;
    if !run(&repo.top, &["rev-parse", "--verify", "-q", "HEAD"], None, LOCAL_TIMEOUT)?.success {
        return Ok(vec![]);
    }
    let n = format!("--max-count={}", limit.clamp(1, 1000));
    let mut args = vec!["log", &n, "--format=%H%x1f%h%x1f%an%x1f%ae%x1f%at%x1f%D%x1f%s%x1e"];
    if let Some(r) = rev {
        check_rev(r)?;
        args.push(r);
    }
    args.push("--");
    let s = text(ok(&repo.top, &args)?);
    Ok(parse_log(&s))
}

fn parse_log(s: &str) -> Vec<Commit> {
    s.split('\x1e')
        .filter_map(|rec| {
            let f: Vec<&str> = rec.trim_start_matches('\n').split('\x1f').collect();
            if f.len() < 7 {
                return None;
            }
            Some(Commit {
                sha: f[0].into(),
                short: f[1].into(),
                author: f[2].into(),
                email: f[3].into(),
                time: f[4].parse().unwrap_or(0),
                refs: f[5].split(", ").filter(|r| !r.is_empty()).map(str::to_string).collect(),
                subject: f[6].into(),
            })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitFile {
    pub status: char,
    pub path: String,
    pub orig_path: Option<String>,
}

pub fn commit_files(workspace: &Path, sha: &str) -> AppResult<Vec<CommitFile>> {
    check_rev(sha)?;
    let repo = require(workspace)?;
    let raw = text(ok(&repo.top, &["diff-tree", "--no-commit-id", "-r", "-z", "--name-status", "-M", "--root", sha])?);
    let mut it = raw.split('\0').filter(|s| !s.is_empty());
    let mut out = Vec::new();
    while let Some(st) = it.next() {
        let status = st.chars().next().unwrap_or('M');
        if status == 'R' || status == 'C' {
            let orig = it.next().unwrap_or_default().to_string();
            let path = it.next().unwrap_or_default().to_string();
            out.push(CommitFile { status, path, orig_path: Some(orig) });
        } else {
            out.push(CommitFile { status, path: it.next().unwrap_or_default().to_string(), orig_path: None });
        }
    }
    Ok(out)
}

// ── Branches ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub name: String,
    pub remote: bool,
    pub current: bool,
    pub short_oid: String,
    pub upstream: Option<String>,
    pub track: Option<String>,
    pub time: i64,
    pub subject: String,
}

pub fn branches(workspace: &Path) -> AppResult<Vec<Branch>> {
    let repo = require(workspace)?;
    let fmt = "--format=%(HEAD)%1f%(refname)%1f%(refname:short)%1f%(objectname:short)%1f%(upstream:short)%1f%(upstream:track)%1f%(committerdate:unix)%1f%(subject)";
    let s = text(ok(&repo.top, &["for-each-ref", fmt, "--sort=-committerdate", "refs/heads", "refs/remotes"])?);
    Ok(s.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\x1f').collect();
            if f.len() < 8 || f[1].ends_with("/HEAD") {
                return None;
            }
            Some(Branch {
                current: f[0] == "*",
                remote: f[1].starts_with("refs/remotes/"),
                name: f[2].into(),
                short_oid: f[3].into(),
                upstream: (!f[4].is_empty()).then(|| f[4].to_string()),
                track: (!f[5].is_empty()).then(|| f[5].to_string()),
                time: f[6].parse().unwrap_or(0),
                subject: f[7].into(),
            })
        })
        .collect())
}

pub fn switch(workspace: &Path, branch: &str, create: bool, start: Option<&str>) -> AppResult<()> {
    let repo = require(workspace)?;
    if create {
        check_branch_name(&repo.top, branch)?;
        let mut args = vec!["switch", "-c", branch];
        if let Some(s) = start {
            check_rev(s)?;
            args.push(s);
        }
        return ok(&repo.top, &args).map(|_| ());
    }
    check_rev(branch)?;
    // Remote branch (e.g. origin/feat) → create a local tracking branch.
    let is_remote = run(&repo.top, &["show-ref", "--verify", "-q", &format!("refs/remotes/{branch}")], None, LOCAL_TIMEOUT)?.success;
    let is_local = run(&repo.top, &["show-ref", "--verify", "-q", &format!("refs/heads/{branch}")], None, LOCAL_TIMEOUT)?.success;
    if is_remote && !is_local {
        ok(&repo.top, &["switch", "--track", branch]).map(|_| ())
    } else {
        ok(&repo.top, &["switch", branch]).map(|_| ())
    }
}

pub fn delete_branch(workspace: &Path, name: &str, force: bool) -> AppResult<()> {
    let repo = require(workspace)?;
    check_rev(name)?;
    ok(&repo.top, &["branch", if force { "-D" } else { "-d" }, "--", name]).map(|_| ())
}

pub fn merge(workspace: &Path, branch: &str) -> AppResult<String> {
    let repo = require(workspace)?;
    check_rev(branch)?;
    let out = run(&repo.top, &["merge", "--no-edit", branch], None, LOCAL_TIMEOUT)?;
    if out.success {
        Ok(text(out.stdout).trim().to_string())
    } else {
        Err(git_error(&out))
    }
}

pub fn merge_abort(workspace: &Path) -> AppResult<()> {
    let repo = require(workspace)?;
    ok(&repo.top, &["merge", "--abort"]).map(|_| ())
}

// ── Remotes ───────────────────────────────────────────────────────────────

pub fn fetch(workspace: &Path) -> AppResult<String> {
    let repo = require(workspace)?;
    let out = ok_with(&repo.top, &["fetch", "--all", "--prune"], None, NETWORK_TIMEOUT)?;
    Ok(text(out).trim().to_string())
}

/// Fast-forward only: never creates surprise merge commits.
pub fn pull(workspace: &Path) -> AppResult<String> {
    let repo = require(workspace)?;
    let out = run(&repo.top, &["pull", "--ff-only"], None, NETWORK_TIMEOUT)?;
    if out.success {
        Ok(text(out.stdout).trim().to_string())
    } else {
        Err(git_error(&out))
    }
}

/// Pushes the current branch. Without an upstream, publishes to the only/`origin` remote.
/// Never forces.
pub fn push(workspace: &Path) -> AppResult<String> {
    let repo = require(workspace)?;
    let st = status(workspace)?;
    let branch = st.branch.clone().ok_or_else(|| AppError::Invalid("HEAD destacado: crie uma branch antes de enviar".into()))?;
    let out = if st.upstream.is_some() {
        run(&repo.top, &["push"], None, NETWORK_TIMEOUT)?
    } else {
        let remotes = text(ok(&repo.top, &["remote"])?);
        let list: Vec<&str> = remotes.lines().collect();
        let remote = if list.contains(&"origin") {
            "origin"
        } else if list.len() == 1 {
            list[0]
        } else if list.is_empty() {
            return Err(AppError::Invalid("o repositório não tem remoto configurado".into()));
        } else {
            return Err(AppError::Invalid("vários remotos e nenhum upstream: configure o upstream da branch".into()));
        };
        run(&repo.top, &["push", "-u", remote, &branch], None, NETWORK_TIMEOUT)?
    };
    if out.success {
        Ok(out.stderr.trim().to_string())
    } else {
        Err(git_error(&out))
    }
}

// ── Stash ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StashEntry {
    pub index: u32,
    pub time: i64,
    pub message: String,
}

pub fn stash_list(workspace: &Path) -> AppResult<Vec<StashEntry>> {
    let repo = require(workspace)?;
    let s = text(ok(&repo.top, &["stash", "list", "--format=%gd%x1f%at%x1f%gs%x1e"])?);
    Ok(s.split('\x1e')
        .filter_map(|rec| {
            let f: Vec<&str> = rec.trim_start_matches('\n').split('\x1f').collect();
            if f.len() < 3 {
                return None;
            }
            let index = f[0].trim_start_matches("stash@{").trim_end_matches('}').parse().ok()?;
            Some(StashEntry { index, time: f[1].parse().unwrap_or(0), message: f[2].into() })
        })
        .collect())
}

pub fn stash_push(workspace: &Path, message: Option<&str>) -> AppResult<()> {
    let repo = require(workspace)?;
    let mut args = vec!["stash", "push", "--include-untracked"];
    if let Some(m) = message.filter(|m| !m.trim().is_empty()) {
        args.push("-m");
        args.push(m);
    }
    ok(&repo.top, &args).map(|_| ())
}

pub fn stash_action(workspace: &Path, action: &str, index: u32) -> AppResult<()> {
    let repo = require(workspace)?;
    if !matches!(action, "apply" | "pop" | "drop") {
        return Err(AppError::Invalid(format!("ação de stash inválida: {action}")));
    }
    let r = format!("stash@{{{index}}}");
    ok(&repo.top, &["stash", action, &r]).map(|_| ())
}

// ── Worktrees ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub path: String,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub main: bool,
    pub detached: bool,
    pub locked: bool,
    pub prunable: bool,
}

pub fn parse_worktrees(s: &str) -> Vec<Worktree> {
    let mut out = Vec::new();
    for (i, block) in s.split("\n\n").filter(|b| !b.trim().is_empty()).enumerate() {
        let mut wt = Worktree { path: String::new(), head: None, branch: None, main: i == 0, detached: false, locked: false, prunable: false };
        for line in block.lines() {
            let (k, v) = line.split_once(' ').unwrap_or((line, ""));
            match k {
                "worktree" => wt.path = v.into(),
                "HEAD" => wt.head = Some(v.into()),
                "branch" => wt.branch = Some(v.trim_start_matches("refs/heads/").into()),
                "detached" => wt.detached = true,
                "locked" => wt.locked = true,
                "prunable" => wt.prunable = true,
                _ => {}
            }
        }
        if !wt.path.is_empty() {
            out.push(wt);
        }
    }
    out
}

pub fn worktrees(workspace: &Path) -> AppResult<Vec<Worktree>> {
    let repo = require(workspace)?;
    Ok(parse_worktrees(&text(ok(&repo.top, &["worktree", "list", "--porcelain"])?)))
}

fn sanitize(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '-' }).collect();
    let s = s.trim_matches(|c| c == '-' || c == '.').to_string();
    if s.is_empty() { "worktree".into() } else { s.chars().take(80).collect() }
}

/// Creates a worktree under `managed_root/<repo>-<id>/<branch>`.
/// `new_branch`: create `branch` from `base` (default HEAD); otherwise check out existing `branch`.
pub fn worktree_add(workspace: &Path, managed_root: &Path, branch: &str, new_branch: bool, base: Option<&str>) -> AppResult<Worktree> {
    let repo = require(workspace)?;
    if new_branch {
        check_branch_name(&repo.top, branch)?;
    } else {
        check_rev(branch)?;
    }
    if let Some(b) = base {
        check_rev(b)?;
    }
    let repo_name = sanitize(&repo.top.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    // Stable per-repo folder: name + short hash of the top-level path.
    let mut h: u64 = 1469598103934665603;
    for b in repo.top.to_string_lossy().bytes() {
        h = (h ^ b as u64).wrapping_mul(1099511628211);
    }
    let dir = managed_root.join(format!("{repo_name}-{:08x}", h as u32));
    std::fs::create_dir_all(&dir)?;
    let mut target = dir.join(sanitize(branch));
    let mut n = 2;
    while target.exists() {
        target = dir.join(format!("{}-{n}", sanitize(branch)));
        n += 1;
    }
    let target_s = target.to_string_lossy().into_owned();
    let mut args = vec!["worktree", "add"];
    if new_branch {
        args.extend(["-b", branch, &target_s]);
        if let Some(b) = base {
            args.push(b);
        }
    } else {
        args.extend([target_s.as_str(), branch]);
    }
    ok(&repo.top, &args)?;
    let canonical = std::fs::canonicalize(&target)?.to_string_lossy().into_owned();
    worktrees(workspace)?
        .into_iter()
        .find(|w| std::fs::canonicalize(&w.path).map(|p| p.to_string_lossy() == canonical).unwrap_or(false))
        .ok_or_else(|| AppError::Internal("worktree criada, mas não encontrada na listagem".into()))
}

/// Removes a linked worktree of this repository (never the main one).
pub fn worktree_remove(workspace: &Path, path: &str, force: bool) -> AppResult<()> {
    let list = worktrees(workspace)?;
    let wt = list
        .iter()
        .find(|w| w.path == path)
        .ok_or_else(|| AppError::NotFound(format!("worktree não encontrada: {path}")))?;
    if wt.main {
        return Err(AppError::Forbidden("a worktree principal não pode ser removida".into()));
    }
    let repo = require(workspace)?;
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(path);
    ok(&repo.top, &args)?;
    let _ = ok(&repo.top, &["worktree", "prune"]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sh(dir: &Path, args: &[&str]) {
        let out = run(dir, args, None, LOCAL_TIMEOUT).unwrap();
        assert!(out.success, "git {args:?} failed: {}", out.stderr);
    }

    fn repo() -> Option<tempfile::TempDir> {
        environment::which("git")?;
        let d = tempfile::tempdir().unwrap();
        sh(d.path(), &["init", "-q", "-b", "main"]);
        for (k, v) in [("user.name", "Forge Test"), ("user.email", "test@forge.dev"), ("commit.gpgsign", "false"), ("core.hooksPath", "/dev/null")] {
            sh(d.path(), &["config", k, v]);
        }
        Some(d)
    }

    #[test]
    fn redacts_credentials() {
        assert_eq!(redact("fatal: https://user:ghp_secret@github.com/x.git"), "fatal: https://***@github.com/x.git");
        assert_eq!(redact("https://token@host/a"), "https://***@host/a");
        assert_eq!(redact("git@github.com:me/x.git"), "git@github.com:me/x.git");
    }

    #[test]
    fn validates_inputs() {
        assert!(check_path("../x").is_err());
        assert!(check_path("/etc/passwd").is_err());
        assert!(check_path("src/a.ts").is_ok());
        assert!(check_rev("--upload-pack=evil").is_err());
        assert!(check_rev("main..evil").is_err());
        assert!(check_rev("origin/feat-1").is_ok());
        assert!(check_rev("HEAD~2").is_ok());
    }

    #[test]
    fn parses_status_v2() {
        let raw = b"# branch.oid abc\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0\
1 .M N... 100644 100644 100644 h1 h2 src/a.ts\0\
2 R. N... 100644 100644 100644 h1 h2 R100 new name.ts\0old.ts\0\
u UU N... 100644 100644 100644 100644 h1 h2 h3 conflict.ts\0\
? untracked file.txt\0";
        let st = parse_status_v2(raw);
        assert_eq!(st.branch.as_deref(), Some("main"));
        assert_eq!((st.ahead, st.behind), (2, 1));
        assert_eq!(st.files.len(), 4);
        assert_eq!(st.files[0], FileStatus { path: "src/a.ts".into(), orig_path: None, index: '.', worktree: 'M', conflicted: false, untracked: false });
        assert_eq!(st.files[1].path, "new name.ts");
        assert_eq!(st.files[1].orig_path.as_deref(), Some("old.ts"));
        assert!(st.files[2].conflicted);
        assert!(st.files[3].untracked);
        assert_eq!(st.files[3].path, "untracked file.txt");
    }

    #[test]
    fn parses_worktree_list() {
        let s = "worktree /r\nHEAD abc\nbranch refs/heads/main\n\nworktree /w/feat\nHEAD def\nbranch refs/heads/feat/x\nlocked\n\nworktree /w/det\nHEAD 123\ndetached\nprunable gitdir file points to non-existent location\n";
        let w = parse_worktrees(s);
        assert_eq!(w.len(), 3);
        assert!(w[0].main && !w[1].main);
        assert_eq!(w[1].branch.as_deref(), Some("feat/x"));
        assert!(w[1].locked && w[2].detached && w[2].prunable);
    }

    #[test]
    fn full_flow_status_stage_commit_branch_merge_stash() {
        let Some(d) = repo() else { return };
        let p = d.path();
        assert!(status(p).unwrap().is_repo);
        assert!(!status(p).unwrap().has_commits);
        fs::write(p.join("a.txt"), "1\n2\n3\n").unwrap();
        let st = status(p).unwrap();
        assert!(st.files[0].untracked);
        stage(p, &["a.txt".into()]).unwrap();
        assert_eq!(status(p).unwrap().files[0].index, 'A');
        unstage(p, &["a.txt".into()]).unwrap(); // no HEAD yet → rm --cached
        assert!(status(p).unwrap().files[0].untracked);
        stage(p, &["a.txt".into()]).unwrap();
        let sha = commit(p, "primeiro commit", false).unwrap();
        assert_eq!(sha.len(), 40);
        assert!(commit(p, "   ", false).is_err());
        let st = status(p).unwrap();
        assert!(st.files.is_empty() && st.has_commits);
        assert_eq!(log(p, 10, None).unwrap()[0].subject, "primeiro commit");

        // Partial staging via index content.
        fs::write(p.join("a.txt"), "1\nDOIS\n3\nquatro\n").unwrap();
        stage_content(p, "a.txt", "1\nDOIS\n3\n").unwrap();
        assert_eq!(file_contents(p, "INDEX", "a.txt").unwrap().content.as_deref(), Some("1\nDOIS\n3\n"));
        assert_eq!(file_contents(p, "HEAD", "a.txt").unwrap().content.as_deref(), Some("1\n2\n3\n"));
        assert_eq!(file_contents(p, "WORKTREE", "a.txt").unwrap().content.as_deref(), Some("1\nDOIS\n3\nquatro\n"));
        let f = &status(p).unwrap().files[0];
        assert_eq!((f.index, f.worktree), ('M', 'M'));
        assert!(!file_contents(p, "HEAD", "nope.txt").unwrap().exists);
        commit(p, "dois", false).unwrap();
        discard(p, &["a.txt".into()]).unwrap();
        assert_eq!(fs::read_to_string(p.join("a.txt")).unwrap(), "1\nDOIS\n3\n");
        assert_eq!(commit_files(p, "HEAD").unwrap()[0].path, "a.txt");

        // Branch, merge with conflict, abort.
        switch(p, "feat/x", true, None).unwrap();
        assert!(switch(p, "bad name", true, None).is_err());
        fs::write(p.join("a.txt"), "feat\n").unwrap();
        stage(p, &["a.txt".into()]).unwrap();
        commit(p, "feat", false).unwrap();
        switch(p, "main", false, None).unwrap();
        fs::write(p.join("a.txt"), "main\n").unwrap();
        stage(p, &["a.txt".into()]).unwrap();
        commit(p, "main", false).unwrap();
        assert!(merge(p, "feat/x").is_err());
        let st = status(p).unwrap();
        assert!(st.merging && st.files[0].conflicted);
        merge_abort(p).unwrap();
        assert!(!status(p).unwrap().merging);
        let names: Vec<_> = branches(p).unwrap().into_iter().map(|b| (b.name, b.current)).collect();
        assert!(names.contains(&("main".into(), true)) && names.contains(&("feat/x".into(), false)));
        assert!(delete_branch(p, "feat/x", false).is_err(), "unmerged branch needs force");
        delete_branch(p, "feat/x", true).unwrap();

        // Stash.
        fs::write(p.join("a.txt"), "wip\n").unwrap();
        stash_push(p, Some("meu wip")).unwrap();
        assert!(status(p).unwrap().files.is_empty());
        let list = stash_list(p).unwrap();
        assert_eq!(list[0].index, 0);
        assert!(list[0].message.contains("meu wip"));
        stash_action(p, "pop", 0).unwrap();
        assert_eq!(fs::read_to_string(p.join("a.txt")).unwrap(), "wip\n");
        assert!(stash_action(p, "clear", 0).is_err());

        // Remote ops without remote fail cleanly.
        assert!(push(p).is_err());
    }

    #[test]
    fn worktree_lifecycle() {
        let Some(d) = repo() else { return };
        let p = d.path();
        fs::write(p.join("r.md"), "x").unwrap();
        stage(p, &["r.md".into()]).unwrap();
        commit(p, "init", false).unwrap();
        let managed = tempfile::tempdir().unwrap();
        let wt = worktree_add(p, managed.path(), "task/one", true, None).unwrap();
        assert_eq!(wt.branch.as_deref(), Some("task/one"));
        assert!(Path::new(&wt.path).join("r.md").is_file());
        assert_eq!(worktrees(p).unwrap().len(), 2);
        // Worktree of the main repo is protected.
        let main = worktrees(p).unwrap()[0].path.clone();
        assert!(matches!(worktree_remove(p, &main, true), Err(AppError::Forbidden(_))));
        // Dirty worktree needs force.
        fs::write(Path::new(&wt.path).join("dirty.txt"), "d").unwrap();
        assert!(worktree_remove(p, &wt.path, false).is_err());
        worktree_remove(p, &wt.path, true).unwrap();
        assert_eq!(worktrees(p).unwrap().len(), 1);
        assert!(!Path::new(&wt.path).exists());
    }

    #[test]
    fn non_repo_reports_cleanly() {
        if environment::which("git").is_none() {
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let st = status(d.path()).unwrap();
        assert!(!st.is_repo);
        assert!(stage(d.path(), &["x".into()]).is_err());
        init(d.path()).unwrap();
        assert!(status(d.path()).unwrap().is_repo);
    }
}
