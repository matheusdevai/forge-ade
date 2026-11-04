//! Read-only filesystem access scoped to a workspace root.
//!
//! Every path coming from the frontend is relative to the workspace root and
//! is resolved + canonicalized here; anything that escapes the root (via
//! `..`, absolute paths or symlinks) is rejected.

use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;

use crate::error::{AppError, AppResult};

pub const MAX_DIR_ENTRIES: usize = 5000;
pub const MAX_TEXT_BYTES: u64 = 1024 * 1024;
/// Hard cap for files opened in the editor / written back.
pub const MAX_EDIT_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    pub rel_path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirListing {
    pub rel_path: String,
    pub entries: Vec<DirEntry>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContent {
    pub rel_path: String,
    pub size: u64,
    pub mtime_ms: i64,
    pub binary: bool,
    pub truncated: bool,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitInfo {
    pub is_repo: bool,
    pub branch: Option<String>,
    pub detached_head: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMeta {
    pub rel_path: String,
    pub size: u64,
    pub mtime_ms: i64,
    pub is_dir: bool,
}

fn mtime_ms(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Canonicalizes a directory chosen by the user so it can be registered.
pub fn canonical_dir(path: &str) -> AppResult<PathBuf> {
    let p = fs::canonicalize(path)
        .map_err(|_| AppError::NotFound(format!("directory not found: {path}")))?;
    if !p.is_dir() {
        return Err(AppError::Invalid(format!("not a directory: {path}")));
    }
    Ok(p)
}

/// Resolves `rel` inside `root`, rejecting traversal and symlink escapes.
pub fn resolve(root: &Path, rel: &str) -> AppResult<PathBuf> {
    let rel_path = Path::new(rel);
    for c in rel_path.components() {
        match c {
            Component::Normal(_) | Component::CurDir => {}
            _ => return Err(AppError::Forbidden("path must be relative to the workspace".into())),
        }
    }
    let root = fs::canonicalize(root)?;
    let target = fs::canonicalize(root.join(rel_path))
        .map_err(|_| AppError::NotFound(format!("path not found: {rel}")))?;
    if !target.starts_with(&root) {
        return Err(AppError::Forbidden("path escapes the workspace".into()));
    }
    Ok(target)
}

fn rel_string(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn list_dir(root: &Path, rel: &str) -> AppResult<DirListing> {
    let root = fs::canonicalize(root)?;
    let dir = resolve(&root, rel)?;
    if !dir.is_dir() {
        return Err(AppError::Invalid(format!("not a directory: {rel}")));
    }
    let mut entries = Vec::new();
    let mut truncated = false;
    for entry in fs::read_dir(&dir)? {
        if entries.len() >= MAX_DIR_ENTRIES {
            truncated = true;
            break;
        }
        let Ok(entry) = entry else { continue };
        let Ok(ft) = entry.file_type() else { continue };
        let path = entry.path();
        let is_symlink = ft.is_symlink();
        // Follow symlinks only to classify them; listing them never escapes
        // because `resolve` is applied again when they are opened.
        let meta = fs::metadata(&path).ok();
        let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
        entries.push(DirEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            rel_path: rel_string(&root, &path),
            is_dir,
            is_symlink,
            size: meta.filter(|m| m.is_file()).map(|m| m.len()),
        });
    }
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(DirListing { rel_path: rel_string(&root, &dir), entries, truncated })
}

pub fn read_text(root: &Path, rel: &str, max_bytes: u64) -> AppResult<FileContent> {
    let max_bytes = max_bytes.clamp(1, MAX_EDIT_BYTES);
    let path = resolve(root, rel)?;
    let meta = fs::metadata(&path)?;
    if !meta.is_file() {
        return Err(AppError::Invalid(format!("not a file: {rel}")));
    }
    let size = meta.len();
    let mut buf = Vec::with_capacity(size.min(max_bytes) as usize);
    fs::File::open(&path)?.take(max_bytes).read_to_end(&mut buf)?;
    let sniff = &buf[..buf.len().min(8192)];
    let binary = sniff.contains(&0) || std::str::from_utf8(sniff).is_err() && String::from_utf8_lossy(sniff).matches('\u{FFFD}').count() > 8;
    Ok(FileContent {
        rel_path: rel.to_string(),
        size,
        mtime_ms: mtime_ms(&meta),
        binary,
        truncated: size > max_bytes,
        content: if binary { None } else { Some(String::from_utf8_lossy(&buf).into_owned()) },
    })
}

// ── Writes ────────────────────────────────────────────────────────────────

fn check_relative(rel: &str) -> AppResult<&Path> {
    let p = Path::new(rel);
    if rel.trim().is_empty() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(AppError::Forbidden("path must be relative to the workspace".into()));
    }
    for c in p.components() {
        let name = c.as_os_str().to_string_lossy();
        if name.contains('\0') || name.len() > 255 {
            return Err(AppError::Invalid(format!("invalid file name: {name}")));
        }
    }
    Ok(p)
}

/// Resolves a path that may not exist yet: its parent must exist inside the
/// root (after canonicalization) and the final component must be a plain name.
pub fn resolve_new(root: &Path, rel: &str) -> AppResult<PathBuf> {
    let p = check_relative(rel)?;
    let name = p.file_name().ok_or_else(|| AppError::Invalid("missing file name".into()))?;
    let root = fs::canonicalize(root)?;
    let parent_rel = p.parent().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
    let parent = resolve(&root, &parent_rel)?;
    if !parent.is_dir() {
        return Err(AppError::Invalid(format!("not a directory: {parent_rel}")));
    }
    let target = parent.join(name);
    // An existing symlink at the target must not point outside the root.
    if fs::symlink_metadata(&target).is_ok() {
        if let Ok(real) = fs::canonicalize(&target) {
            if !real.starts_with(&root) {
                return Err(AppError::Forbidden("path escapes the workspace".into()));
            }
        }
    }
    Ok(target)
}

pub fn stat(root: &Path, rel: &str) -> AppResult<FileMeta> {
    let path = resolve(root, rel)?;
    let meta = fs::metadata(&path)?;
    Ok(FileMeta { rel_path: rel.to_string(), size: meta.len(), mtime_ms: mtime_ms(&meta), is_dir: meta.is_dir() })
}

/// Atomically writes a text file inside the workspace.
///
/// - `expected_mtime_ms`: optimistic concurrency. If the file changed on disk
///   since it was read, fails with `Conflict` instead of overwriting.
/// - Writes to a temp file in the same directory, fsyncs, keeps the original
///   permissions, then renames over the target (symlinks are followed to
///   their real file, which must be inside the workspace).
pub fn write_text(root: &Path, rel: &str, content: &str, expected_mtime_ms: Option<i64>) -> AppResult<FileMeta> {
    if content.len() as u64 > MAX_EDIT_BYTES {
        return Err(AppError::Invalid("file too large to save from the editor".into()));
    }
    let target = match resolve(root, rel) {
        Ok(p) => p,
        Err(AppError::NotFound(_)) => resolve_new(root, rel)?,
        Err(e) => return Err(e),
    };
    let existing = fs::metadata(&target).ok();
    if existing.as_ref().is_some_and(|m| m.is_dir()) {
        return Err(AppError::Invalid(format!("is a directory: {rel}")));
    }
    if let (Some(expected), Some(meta)) = (expected_mtime_ms, existing.as_ref()) {
        if mtime_ms(meta) != expected {
            return Err(AppError::Conflict(format!("file changed on disk: {rel}")));
        }
    }
    let dir = target.parent().ok_or_else(|| AppError::Invalid("missing parent".into()))?;
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.forge-{}.tmp", &crate::db::new_id()[..8]));
    let result = (|| -> AppResult<()> {
        let mut f = fs::File::create(&tmp)?;
        std::io::Write::write_all(&mut f, content.as_bytes())?;
        f.sync_all()?;
        drop(f);
        if let Some(meta) = &existing {
            fs::set_permissions(&tmp, meta.permissions())?;
        }
        fs::rename(&tmp, &target)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result?;
    let meta = fs::metadata(&target)?;
    Ok(FileMeta { rel_path: rel.to_string(), size: meta.len(), mtime_ms: mtime_ms(&meta), is_dir: false })
}

pub fn create_file(root: &Path, rel: &str) -> AppResult<FileMeta> {
    let target = resolve_new(root, rel)?;
    fs::OpenOptions::new().write(true).create_new(true).open(&target).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => AppError::Invalid(format!("already exists: {rel}")),
        _ => AppError::Io(e),
    })?;
    stat(root, rel)
}

pub fn create_dir(root: &Path, rel: &str) -> AppResult<FileMeta> {
    let target = resolve_new(root, rel)?;
    fs::create_dir(&target).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => AppError::Invalid(format!("already exists: {rel}")),
        _ => AppError::Io(e),
    })?;
    stat(root, rel)
}

pub fn rename(root: &Path, from: &str, to: &str) -> AppResult<FileMeta> {
    check_relative(from)?;
    // Operate on the link itself, not its target: resolve the parent only.
    let src = resolve_new(root, from)?;
    if fs::symlink_metadata(&src).is_err() {
        return Err(AppError::NotFound(format!("path not found: {from}")));
    }
    let dst = resolve_new(root, to)?;
    if fs::symlink_metadata(&dst).is_ok() {
        return Err(AppError::Invalid(format!("already exists: {to}")));
    }
    let root_c = fs::canonicalize(root)?;
    if src == root_c {
        return Err(AppError::Forbidden("cannot rename the workspace root".into()));
    }
    if dst.starts_with(&src) {
        return Err(AppError::Invalid("cannot move a folder into itself".into()));
    }
    fs::rename(&src, &dst)?;
    stat(root, to)
}

/// Moves a file or folder to the OS trash (recoverable). Never the root.
pub fn trash(root: &Path, rel: &str) -> AppResult<()> {
    check_relative(rel)?;
    let target = resolve_new(root, rel)?;
    if fs::symlink_metadata(&target).is_err() {
        return Err(AppError::NotFound(format!("path not found: {rel}")));
    }
    if target == fs::canonicalize(root)? {
        return Err(AppError::Forbidden("cannot delete the workspace root".into()));
    }
    trash::delete(&target).map_err(|e| AppError::Internal(format!("trash: {e}")))
}

/// Reads branch info directly from `.git` (no git binary needed). Supports
/// regular repos and linked worktrees (`.git` file with `gitdir:`).
pub fn git_info(root: &Path) -> GitInfo {
    let none = GitInfo { is_repo: false, branch: None, detached_head: None };
    let dot_git = root.join(".git");
    let git_dir = if dot_git.is_dir() {
        dot_git
    } else if dot_git.is_file() {
        match fs::read_to_string(&dot_git) {
            Ok(s) => match s.trim().strip_prefix("gitdir:") {
                Some(p) => {
                    let p = PathBuf::from(p.trim());
                    if p.is_absolute() { p } else { root.join(p) }
                }
                None => return none,
            },
            Err(_) => return none,
        }
    } else {
        return none;
    };
    let Ok(head) = fs::read_to_string(git_dir.join("HEAD")) else {
        return none;
    };
    let head = head.trim();
    match head.strip_prefix("ref: refs/heads/") {
        Some(branch) => GitInfo { is_repo: true, branch: Some(branch.to_string()), detached_head: None },
        None => GitInfo {
            is_repo: true,
            branch: None,
            detached_head: Some(head.chars().take(12).collect()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join("src/nested")).unwrap();
        fs::write(d.path().join("src/main.ts"), "console.log(1)\n").unwrap();
        fs::write(d.path().join("README.md"), "# hi").unwrap();
        fs::write(d.path().join("blob.bin"), [0u8, 1, 2, 3]).unwrap();
        d
    }

    #[test]
    fn lists_dirs_first_with_relative_paths() {
        let d = fixture();
        let l = list_dir(d.path(), "").unwrap();
        assert_eq!(l.entries[0].name, "src");
        assert!(l.entries[0].is_dir);
        let nested = list_dir(d.path(), "src").unwrap();
        assert_eq!(nested.entries[0].rel_path, "src/nested");
        assert_eq!(nested.entries[1].rel_path, "src/main.ts");
    }

    #[test]
    fn rejects_traversal_and_absolute_paths() {
        let d = fixture();
        assert!(matches!(resolve(d.path(), "../"), Err(AppError::Forbidden(_))));
        assert!(matches!(resolve(d.path(), "/etc"), Err(AppError::Forbidden(_))));
        assert!(matches!(resolve(d.path(), "src/../../x"), Err(AppError::Forbidden(_))));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        let d = fixture();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret"), "s").unwrap();
        std::os::unix::fs::symlink(outside.path(), d.path().join("link")).unwrap();
        assert!(matches!(read_text(d.path(), "link/secret", MAX_TEXT_BYTES), Err(AppError::Forbidden(_))));
        assert!(matches!(write_text(d.path(), "link/secret", "x", None), Err(AppError::Forbidden(_))));
        assert!(matches!(create_file(d.path(), "link/new"), Err(AppError::Forbidden(_))));
        assert_eq!(fs::read_to_string(outside.path().join("secret")).unwrap(), "s");
    }

    #[test]
    fn reads_text_and_detects_binary() {
        let d = fixture();
        let t = read_text(d.path(), "README.md", MAX_TEXT_BYTES).unwrap();
        assert_eq!(t.content.as_deref(), Some("# hi"));
        assert!(!t.binary);
        let b = read_text(d.path(), "blob.bin", MAX_TEXT_BYTES).unwrap();
        assert!(b.binary);
        assert!(b.content.is_none());
    }

    #[test]
    fn write_is_atomic_with_conflict_detection() {
        let d = fixture();
        let before = read_text(d.path(), "README.md", MAX_TEXT_BYTES).unwrap();
        let meta = write_text(d.path(), "README.md", "# novo ✓", Some(before.mtime_ms)).unwrap();
        assert_eq!(fs::read_to_string(d.path().join("README.md")).unwrap(), "# novo ✓");
        // Stale mtime → conflict, file untouched.
        let stale = meta.mtime_ms - 10_000;
        assert!(matches!(write_text(d.path(), "README.md", "lost", Some(stale)), Err(AppError::Conflict(_))));
        assert_eq!(fs::read_to_string(d.path().join("README.md")).unwrap(), "# novo ✓");
        // No temp files left behind.
        assert!(fs::read_dir(d.path()).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().contains("forge-")));
        // Creating a new file through write works too.
        write_text(d.path(), "src/new.ts", "export {}", None).unwrap();
        assert!(d.path().join("src/new.ts").is_file());
        assert!(matches!(write_text(d.path(), "../evil", "x", None), Err(AppError::Forbidden(_))));
        assert!(matches!(write_text(d.path(), "src", "x", None), Err(AppError::Invalid(_))));
    }

    #[cfg(unix)]
    #[test]
    fn write_preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let d = fixture();
        let p = d.path().join("run.sh");
        fs::write(&p, "#!/bin/sh").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        write_text(d.path(), "run.sh", "#!/bin/sh\necho hi", None).unwrap();
        assert_eq!(fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o755);
    }

    #[test]
    fn create_rename_and_guards() {
        let d = fixture();
        create_file(d.path(), "src/a.ts").unwrap();
        assert!(create_file(d.path(), "src/a.ts").is_err(), "no overwrite");
        create_dir(d.path(), "src/lib").unwrap();
        assert!(create_file(d.path(), "missing/dir/x.ts").is_err());
        rename(d.path(), "src/a.ts", "src/lib/b.ts").unwrap();
        assert!(d.path().join("src/lib/b.ts").is_file());
        assert!(rename(d.path(), "src/lib/b.ts", "README.md").is_err(), "no clobber");
        assert!(rename(d.path(), "src", "src/lib/inner").is_err(), "no move into itself");
        assert!(matches!(rename(d.path(), "src", "../out"), Err(AppError::Forbidden(_))));
        assert!(matches!(trash(d.path(), ""), Err(AppError::Forbidden(_))));
        assert!(matches!(trash(d.path(), ".."), Err(AppError::Forbidden(_))));
        assert!(stat(d.path(), "src/lib").unwrap().is_dir);
    }

    #[test]
    fn git_info_reads_head() {
        let d = fixture();
        assert!(!git_info(d.path()).is_repo);
        fs::create_dir(d.path().join(".git")).unwrap();
        fs::write(d.path().join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        let g = git_info(d.path());
        assert!(g.is_repo);
        assert_eq!(g.branch.as_deref(), Some("main"));
    }
}
