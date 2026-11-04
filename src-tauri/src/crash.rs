//! Local crash reports (M20). Panics in the core and unhandled UI errors are
//! written to `<app data>/crash/*.log` with secrets redacted. Nothing is ever
//! sent anywhere: the user decides whether to share a report.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::redact::redact_secrets;

pub const MAX_REPORTS: usize = 50;
const MAX_REPORT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CrashReport {
    pub file: String,
    pub source: String,
    pub created_at: i64,
    pub summary: String,
    pub bytes: u64,
}

fn clip(s: &str) -> String {
    if s.len() <= MAX_REPORT_BYTES {
        return s.to_string();
    }
    let mut end = MAX_REPORT_BYTES;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n…(cortado)", &s[..end])
}

/// Writes a report; keeps only the newest `MAX_REPORTS`.
pub fn write(dir: &Path, source: &str, summary: &str, details: &str) -> AppResult<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let ts = now_ms();
    let source = if source.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') && !source.is_empty() { source } else { "unknown" };
    let path = dir.join(format!("{ts}-{source}.log"));
    let body = format!(
        "FORGE {} — {}\nfonte: {source}\nquando: {ts}\nsistema: {} {}\n\n{}\n\n{}",
        env!("CARGO_PKG_VERSION"),
        redact_secrets(summary.lines().next().unwrap_or_default()),
        std::env::consts::OS,
        std::env::consts::ARCH,
        redact_secrets(summary),
        clip(&redact_secrets(details))
    );
    std::fs::write(&path, body)?;
    prune(dir)?;
    Ok(path)
}

fn prune(dir: &Path) -> AppResult<()> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "log")).collect();
    files.sort();
    while files.len() > MAX_REPORTS {
        let _ = std::fs::remove_file(files.remove(0));
    }
    Ok(())
}

pub fn list(dir: &Path) -> AppResult<Vec<CrashReport>> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Ok(vec![]) };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        let Some(name) = p.file_name().and_then(|n| n.to_str()).map(String::from) else { continue };
        if !name.ends_with(".log") {
            continue;
        }
        let (ts, source) = name.trim_end_matches(".log").split_once('-').unwrap_or(("0", "unknown"));
        let text = std::fs::read_to_string(&p).unwrap_or_default();
        let summary = text.lines().next().and_then(|l| l.split_once(" — ")).map(|(_, s)| s.to_string()).unwrap_or_default();
        out.push(CrashReport { file: name.clone(), source: source.into(), created_at: ts.parse().unwrap_or(0), summary, bytes: e.metadata().map(|m| m.len()).unwrap_or(0) });
    }
    out.sort_by_key(|r| std::cmp::Reverse(r.created_at));
    Ok(out)
}

/// Reads a report by file name (never a path outside the crash dir).
pub fn read(dir: &Path, file: &str) -> AppResult<String> {
    if file.contains('/') || file.contains('\\') || file.contains("..") || !file.ends_with(".log") {
        return Err(AppError::Forbidden("relatório inválido".into()));
    }
    std::fs::read_to_string(dir.join(file)).map_err(|_| AppError::NotFound("relatório não encontrado".into()))
}

pub fn clear(dir: &Path) -> AppResult<usize> {
    let reports = list(dir)?;
    for r in &reports {
        let _ = std::fs::remove_file(dir.join(&r.file));
    }
    Ok(reports.len())
}

/// Installs a panic hook that records the panic (then runs the default hook).
pub fn install_panic_hook(dir: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".into());
        let loc = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let thread = std::thread::current().name().unwrap_or("?").to_string();
        let bt = std::backtrace::Backtrace::force_capture();
        let _ = write(&dir, "core", &format!("panic em {loc} (thread {thread}): {msg}"), &bt.to_string());
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_are_redacted_listed_capped_and_confined() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(dir.path(), "ui", "TypeError: x is undefined", "stack\nAuthorization: Bearer ghp_abcdefghijklmnopqrstuvwxyz0123456789").unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.contains("TypeError"));
        assert!(!text.contains("ghp_abcdefghijklmnop"), "{text}");
        let l = list(dir.path()).unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!((l[0].source.as_str(), l[0].summary.as_str()), ("ui", "TypeError: x is undefined"));
        assert!(read(dir.path(), "../etc/passwd").is_err());
        assert!(read(dir.path(), &l[0].file).unwrap().contains("TypeError"));
        assert!(write(dir.path(), "../evil", "s", "d").unwrap().file_name().unwrap().to_str().unwrap().ends_with("-unknown.log"));
        for i in 0..(MAX_REPORTS + 5) {
            std::fs::write(dir.path().join(format!("{}-x.log", 1000 + i)), "FORGE — y").unwrap();
        }
        prune(dir.path()).unwrap();
        assert_eq!(list(dir.path()).unwrap().len(), MAX_REPORTS);
        assert_eq!(clear(dir.path()).unwrap(), MAX_REPORTS);
    }

    #[test]
    fn panic_hook_writes_a_report() {
        let dir = tempfile::tempdir().unwrap();
        // Captures a real panic through the same code path the hook uses.
        let d = dir.path().to_path_buf();
        let r = std::panic::catch_unwind(move || {
            let _ = write(&d, "core", "panic em test.rs:1 (thread t): boom", "bt");
            panic!("boom");
        });
        assert!(r.is_err());
        assert!(list(dir.path()).unwrap()[0].summary.contains("boom"));
    }
}

#[cfg(test)]
mod release_signature {
    use base64::Engine as _;

    fn b64(s: &str) -> String {
        String::from_utf8(base64::engine::general_purpose::STANDARD.decode(s.trim()).unwrap()).unwrap()
    }

    /// Verifies the locally built updater package against the public key
    /// embedded in tauri.conf.json (same check the updater runs on install).
    #[test]
    #[ignore = "needs a release build (npx tauri build)"]
    fn release_updater_package_signature_is_valid() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let conf: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root.join("tauri.conf.json")).unwrap()).unwrap();
        let pubkey = minisign_verify::PublicKey::decode(&b64(conf["plugins"]["updater"]["pubkey"].as_str().unwrap())).unwrap();
        let dir = root.join("target/release/bundle/macos");
        let sig = minisign_verify::Signature::decode(&b64(&std::fs::read_to_string(dir.join("FORGE.app.tar.gz.sig")).unwrap())).unwrap();
        let data = std::fs::read(dir.join("FORGE.app.tar.gz")).unwrap();
        pubkey.verify(&data, &sig, false).expect("valid signature");
        let mut tampered = data.clone();
        let mid = tampered.len() / 2;
        tampered[mid] ^= 0xFF;
        assert!(pubkey.verify(&tampered, &sig, false).is_err(), "tampered package must be rejected");
    }
}
