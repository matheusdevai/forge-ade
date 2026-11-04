//! Workspace file listing (quick open) and text search.
//!
//! Uses the `ignore` crate — the same walker ripgrep uses — so `.gitignore`,
//! `.ignore` and global git excludes are respected, without depending on an
//! external `rg` binary. `.git/` is always skipped; other dotfiles are kept.

use std::fs;
use std::io::Read;
use std::path::Path;

use ignore::WalkBuilder;
use regex::RegexBuilder;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

pub const MAX_LISTED_FILES: usize = 50_000;
pub const MAX_SEARCH_FILE_BYTES: u64 = 2 * 1024 * 1024;
pub const DEFAULT_MAX_MATCHES: usize = 2_000;
const PREVIEW_CONTEXT: usize = 60;
const PREVIEW_MAX: usize = 240;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileList {
    pub files: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOptions {
    pub query: String,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub whole_word: bool,
    pub max_matches: Option<usize>,
}

/// Columns/offsets are UTF-16 code units (what Monaco and JS strings use), 1-based for `column`.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchMatch {
    pub rel_path: String,
    pub line: usize,
    pub column: usize,
    pub length: usize,
    pub preview: String,
    pub preview_start: usize,
    pub preview_end: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub matches: Vec<SearchMatch>,
    pub files_with_matches: usize,
    pub files_scanned: usize,
    pub truncated: bool,
}

fn walker(root: &Path) -> ignore::Walk {
    WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|e| e.file_name() != ".git" && e.file_name() != "node_modules" || e.depth() == 0)
        .build()
}

fn rel_of(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn list_files(root: &Path) -> AppResult<FileList> {
    let root = fs::canonicalize(root)?;
    let mut files = Vec::new();
    let mut truncated = false;
    for entry in walker(&root).flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        if files.len() >= MAX_LISTED_FILES {
            truncated = true;
            break;
        }
        files.push(rel_of(&root, entry.path()));
    }
    files.sort_unstable();
    Ok(FileList { files, truncated })
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

fn make_preview(line: &str, start: usize, end: usize) -> (String, usize, usize) {
    // Byte offsets → char indices, then window around the match.
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let cs = chars.iter().position(|(b, _)| *b >= start).unwrap_or(chars.len());
    let ce = chars.iter().position(|(b, _)| *b >= end).unwrap_or(chars.len());
    let from = cs.saturating_sub(PREVIEW_CONTEXT);
    let to = (from + PREVIEW_MAX).max(ce).min(chars.len());
    let slice: String = chars[from..to].iter().map(|(_, c)| *c).collect();
    let lead_trim = slice.len() - slice.trim_start().len();
    let preview = slice.trim().to_string();
    let prefix: String = chars[from..cs].iter().map(|(_, c)| *c).collect();
    let matched: String = chars[cs..ce].iter().map(|(_, c)| *c).collect();
    let pre_u16 = utf16_len(&prefix).saturating_sub(utf16_len(&slice[..lead_trim]));
    (preview, pre_u16, pre_u16 + utf16_len(&matched))
}

pub fn search(root: &Path, opts: &SearchOptions) -> AppResult<SearchResult> {
    let mut result = SearchResult { matches: vec![], files_with_matches: 0, files_scanned: 0, truncated: false };
    if opts.query.is_empty() {
        return Ok(result);
    }
    if opts.query.len() > 1000 {
        return Err(AppError::Invalid("query too long".into()));
    }
    let mut pattern = if opts.regex { opts.query.clone() } else { regex::escape(&opts.query) };
    if opts.whole_word {
        pattern = format!(r"\b(?:{pattern})\b");
    }
    let re = RegexBuilder::new(&pattern)
        .case_insensitive(!opts.case_sensitive)
        .size_limit(4 << 20)
        .build()
        .map_err(|e| AppError::Invalid(format!("invalid regular expression: {e}")))?;
    let max = opts.max_matches.unwrap_or(DEFAULT_MAX_MATCHES).clamp(1, 10_000);

    let root = fs::canonicalize(root)?;
    'files: for entry in walker(&root).flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if meta.len() > MAX_SEARCH_FILE_BYTES {
            continue;
        }
        let mut bytes = Vec::with_capacity(meta.len() as usize);
        if fs::File::open(entry.path()).and_then(|mut f| f.read_to_end(&mut bytes)).is_err() {
            continue;
        }
        if bytes[..bytes.len().min(8192)].contains(&0) {
            continue; // binary
        }
        let Ok(text) = std::str::from_utf8(&bytes) else { continue };
        result.files_scanned += 1;
        let rel = rel_of(&root, entry.path());
        let mut file_had_match = false;
        for (idx, line) in text.lines().enumerate() {
            for m in re.find_iter(line) {
                if m.start() == m.end() {
                    continue; // skip empty matches (e.g. `a*`)
                }
                if result.matches.len() >= max {
                    result.truncated = true;
                    break 'files;
                }
                file_had_match = true;
                let (preview, ps, pe) = make_preview(line, m.start(), m.end());
                result.matches.push(SearchMatch {
                    rel_path: rel.clone(),
                    line: idx + 1,
                    column: utf16_len(&line[..m.start()]) + 1,
                    length: utf16_len(m.as_str()),
                    preview,
                    preview_start: ps,
                    preview_end: pe,
                });
            }
        }
        if file_had_match {
            result.files_with_matches += 1;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(q: &str) -> SearchOptions {
        SearchOptions { query: q.into(), case_sensitive: false, regex: false, whole_word: false, max_matches: None }
    }

    fn fixture() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join("src")).unwrap();
        fs::create_dir_all(d.path().join("dist")).unwrap();
        fs::create_dir_all(d.path().join(".git")).unwrap();
        fs::write(d.path().join(".gitignore"), "dist/\n*.log\n").unwrap();
        fs::write(d.path().join("src/app.ts"), "const Forge = 1;\nconsole.log(forge, FORGE);\nconst café = 'ação forge';\n").unwrap();
        fs::write(d.path().join("src/forger.ts"), "forger\n").unwrap();
        fs::write(d.path().join("dist/bundle.js"), "forge forge").unwrap();
        fs::write(d.path().join("debug.log"), "forge").unwrap();
        fs::write(d.path().join(".git/config"), "forge").unwrap();
        fs::write(d.path().join(".env.example"), "FORGE=1").unwrap();
        fs::write(d.path().join("img.bin"), [b'f', b'o', 0, b'r']).unwrap();
        d
    }

    #[test]
    fn lists_files_respecting_gitignore() {
        let d = fixture();
        let l = list_files(d.path()).unwrap();
        assert_eq!(l.files, [".env.example", ".gitignore", "img.bin", "src/app.ts", "src/forger.ts"]);
    }

    #[test]
    fn plain_case_insensitive_search() {
        let d = fixture();
        let r = search(d.path(), &opts("forge")).unwrap();
        let hits: Vec<_> = r.matches.iter().map(|m| (m.rel_path.as_str(), m.line, m.column)).collect();
        assert!(hits.contains(&(".env.example", 1, 1)));
        assert!(hits.contains(&("src/app.ts", 1, 7)));
        assert!(hits.contains(&("src/app.ts", 2, 13)));
        assert!(hits.contains(&("src/app.ts", 2, 20)));
        assert!(hits.contains(&("src/forger.ts", 1, 1)));
        assert!(!hits.iter().any(|(p, ..)| p.starts_with("dist") || p.ends_with(".log") || p.starts_with(".git/") || *p == "img.bin"));
    }

    #[test]
    fn options_case_word_regex() {
        let d = fixture();
        let mut o = opts("FORGE");
        o.case_sensitive = true;
        let r = search(d.path(), &o).unwrap();
        assert_eq!(r.matches.iter().filter(|m| m.rel_path == "src/app.ts").count(), 1);

        let mut o = opts("forge");
        o.whole_word = true;
        let r = search(d.path(), &o).unwrap();
        assert!(!r.matches.iter().any(|m| m.rel_path == "src/forger.ts"));

        let mut o = opts(r"const \w+ =");
        o.regex = true;
        let r = search(d.path(), &o).unwrap();
        assert_eq!(r.matches.len(), 2);

        let mut o = opts("(unclosed");
        o.regex = true;
        assert!(matches!(search(d.path(), &o), Err(AppError::Invalid(_))));
    }

    #[test]
    fn utf16_columns_and_preview_offsets() {
        let d = fixture();
        let r = search(d.path(), &opts("forge")).unwrap();
        let m = r.matches.iter().find(|m| m.rel_path == "src/app.ts" && m.line == 3).unwrap();
        // "const café = 'ação forge';" — accented chars are 1 UTF-16 unit each.
        assert_eq!(m.column, 20);
        let preview_u16: Vec<u16> = m.preview.encode_utf16().collect();
        assert_eq!(String::from_utf16(&preview_u16[m.preview_start..m.preview_end]).unwrap(), "forge");
    }

    #[test]
    fn caps_matches() {
        let d = fixture();
        let mut o = opts("forge");
        o.max_matches = Some(2);
        let r = search(d.path(), &o).unwrap();
        assert_eq!(r.matches.len(), 2);
        assert!(r.truncated);
    }
}
