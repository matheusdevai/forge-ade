//! Secret redaction for anything FORGE persists or displays from tools and
//! agents (event logs, run results, error messages).
//!
//! Best-effort pattern matching: it catches common credential shapes; it is a
//! safety net, not a guarantee.

use std::sync::OnceLock;

use regex::Regex;

const MASK: &str = "[REDACTED]";

struct Rule {
    re: Regex,
    /// Replacement template (may reference capture groups).
    with: &'static str,
}

fn rules() -> &'static [Rule] {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        let r = |p: &str, with: &'static str| Rule { re: Regex::new(p).expect("valid redaction regex"), with };
        vec![
            // PEM private keys (multi-line).
            r(r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z0-9 ]*PRIVATE KEY-----", "[REDACTED PRIVATE KEY]"),
            // Credentials embedded in URLs.
            r(r"(?i)\b([a-z][a-z0-9+.\-]*://)[^/\s@:]+:[^/\s@]+@", "${1}[REDACTED]@"),
            // Provider-specific token shapes.
            r(r"\bsk-ant-[A-Za-z0-9_\-]{20,}", MASK),
            r(r"\bsk-(?:proj-|live-|test-)?[A-Za-z0-9_\-]{20,}", MASK),
            r(r"\bgh[pousr]_[A-Za-z0-9]{20,}", MASK),
            r(r"\bgithub_pat_[A-Za-z0-9_]{20,}", MASK),
            r(r"\bglpat-[A-Za-z0-9_\-]{20,}", MASK),
            r(r"\bxox[baprs]-[A-Za-z0-9\-]{10,}", MASK),
            r(r"\bAKIA[0-9A-Z]{16}\b", MASK),
            r(r"\bAIza[0-9A-Za-z_\-]{35}\b", MASK),
            r(r"\b(?:sk|rk|pk)_(?:live|test)_[A-Za-z0-9]{16,}", MASK),
            r(r"\bnpm_[A-Za-z0-9]{30,}", MASK),
            // JWTs.
            r(r"\beyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}", MASK),
            // Bearer tokens in headers.
            r(r"(?i)\b(authorization\s*:\s*bearer\s+)[A-Za-z0-9._\-~+/]{12,}=*", "${1}[REDACTED]"),
        ]
    })
}

/// `key=value` / `"key": "value"` with secret-ish key names. The value is
/// masked only when it looks like a secret (not code like `process.env.X`).
fn assignment_rule() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?i)\b([A-Z0-9_]*(?:api[_-]?key|secret|token|passw(?:or)?d|private[_-]?key|access[_-]?key)[A-Z0-9_]*)(["']?\s*[:=]\s*["']?)([^\s"',;]{8,})"#,
        )
        .expect("valid regex")
    })
}

fn looks_secret(v: &str) -> bool {
    if v.contains('.') || v.contains('(') || v.contains('$') || v.starts_with("process") || v.starts_with("env") {
        return false;
    }
    let digits = v.chars().any(|c| c.is_ascii_digit());
    let letters = v.chars().any(|c| c.is_ascii_alphabetic());
    (digits && letters) || v.len() >= 20
}

pub fn redact_secrets(input: &str) -> String {
    let mut out = input.to_string();
    for rule in rules() {
        if rule.re.is_match(&out) {
            out = rule.re.replace_all(&out, rule.with).into_owned();
        }
    }
    let re = assignment_rule();
    if re.is_match(&out) {
        out = re
            .replace_all(&out, |c: &regex::Captures| {
                if looks_secret(&c[3]) {
                    format!("{}{}{MASK}", &c[1], &c[2])
                } else {
                    c[0].to_string()
                }
            })
            .into_owned();
    }
    out
}

/// Redacts and caps a string at `max` bytes (on a char boundary).
pub fn sanitize(input: &str, max: usize) -> String {
    let s = redact_secrets(input);
    if s.len() <= max {
        return s;
    }
    let mut cut = max;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}… [truncado: {} bytes]", &s[..cut], s.len() - cut)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_common_secrets() {
        let cases = [
            ("export OPENAI_API_KEY=sk-proj-abcdefghijklmnopqrstuvwxyz123456", "sk-proj-abc"),
            ("token ghp_abcdefghijklmnopqrstuvwxyz0123456789", "ghp_abc"),
            ("ANTHROPIC_API_KEY=sk-ant-api03-abcdefghijklmnopqrstuvwx", "sk-ant-api03"),
            ("aws AKIAIOSFODNN7EXAMPLE here", "AKIAIOSFODNN7EXAMPLE"),
            ("google AIzaSyA1234567890abcdefghijklmnopqrstuv", "AIzaSyA123"),
            ("slack xoxb-1234567890-abcdefghij", "xoxb-1234567890"),
            ("Authorization: Bearer abc.def.ghijklmnopqrstu", "abc.def.ghij"),
            ("git clone https://user:s3cr3tpass@github.com/x.git", "s3cr3tpass"),
            (r#"{"password": "hunter2hunter2"}"#, "hunter2hunter2"),
            ("DB_PASSWORD=correct-horse-battery", "correct-horse"),
            ("jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.abcdefghijklmnop", "eyJzdWIi"),
            ("-----BEGIN OPENSSH PRIVATE KEY-----\nAAAAB3NzaC1yc2E\nxyz\n-----END OPENSSH PRIVATE KEY-----", "AAAAB3NzaC1yc2E"),
        ];
        for (input, secret) in cases {
            let out = redact_secrets(input);
            assert!(!out.contains(secret), "leaked {secret:?} in {out:?}");
            assert!(out.contains("REDACTED"), "no mask in {out:?}");
        }
    }

    #[test]
    fn keeps_normal_text() {
        let s = "Edited src/tokenizer.ts: renamed `token` variable; password field is optional.";
        assert_eq!(redact_secrets(s), s);
        assert_eq!(redact_secrets("const apiKey = process.env.KEY"), "const apiKey = process.env.KEY");
    }

    #[test]
    fn sanitize_caps_on_char_boundary() {
        let s = "ção".repeat(100);
        let out = sanitize(&s, 11);
        assert!(out.starts_with("çãoçã"));
        assert!(out.contains("truncado"));
    }
}
