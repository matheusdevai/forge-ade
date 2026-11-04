//! Built-in browser (M13): the user's installed Chrome, headless, driven over
//! CDP with an isolated profile. Pages are streamed to the pane as a
//! screencast; console, exceptions and network failures are captured for the
//! verify → fix loop. See ADR 081–086.

pub mod cdp;
pub mod chrome;
pub mod manager;
pub mod preview;

use serde::Serialize;
use serde_json::Value;

use crate::error::{AppError, AppResult};

pub const MAX_ENTRIES: usize = 1000;
pub const MAX_ENTRY_TEXT: usize = 4000;

/// Normalizes what the user typed in the address bar. Only http(s) and
/// `about:blank` are allowed: `file:`, `javascript:`, `chrome:` etc. are
/// refused so the pane cannot be used to read local files or browser internals.
pub fn normalize_url(input: &str) -> AppResult<String> {
    let s = input.trim();
    if s.is_empty() {
        return Err(AppError::Invalid("informe uma URL".into()));
    }
    if s.eq_ignore_ascii_case("about:blank") {
        return Ok("about:blank".into());
    }
    if s.contains(char::is_whitespace) {
        return Err(AppError::Invalid("a URL não pode conter espaços".into()));
    }
    let has_scheme = s.split_once("://").is_some_and(|(sch, _)| !sch.is_empty() && sch.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c)));
    let lower = s.to_ascii_lowercase();
    let candidate = if has_scheme {
        s.to_string()
    } else if ["javascript:", "data:", "file:", "chrome:", "view-source:", "blob:", "about:", "chrome-extension:"]
        .iter()
        .any(|p| lower.starts_with(p))
    {
        return Err(AppError::Forbidden("esse tipo de endereço não é permitido no navegador do FORGE".into()));
    } else {
        let host = lower.split(['/', '?', '#']).next().unwrap_or("");
        let host_only = host.rsplit_once(':').map(|(h, p)| if p.chars().all(|c| c.is_ascii_digit()) { h } else { host }).unwrap_or(host);
        let local = matches!(host_only, "localhost" | "127.0.0.1" | "0.0.0.0" | "[::1]") || host_only.ends_with(".localhost");
        if local {
            format!("http://{s}")
        } else if host_only.contains('.') {
            format!("https://{s}")
        } else {
            return Err(AppError::Invalid(format!("“{s}” não parece um endereço válido")));
        }
    };
    let mut url = url::Url::parse(&candidate).map_err(|_| AppError::Invalid(format!("URL inválida: {s}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::Forbidden(format!("o esquema “{}:” não é permitido", url.scheme())));
    }
    if url.host_str().is_none() {
        return Err(AppError::Invalid(format!("URL sem host: {s}")));
    }
    // Dev servers often print 0.0.0.0, which Chrome blocks as a destination.
    if url.host_str() == Some("0.0.0.0") {
        let _ = url.set_host(Some("localhost"));
    }
    Ok(url.to_string())
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleEntry {
    pub seq: u64,
    pub ts: i64,
    /// error | warning | info | log | debug
    pub level: String,
    /// console | exception | network | log
    pub source: String,
    pub text: String,
    pub url: Option<String>,
}

fn clip(s: &str) -> String {
    if s.len() <= MAX_ENTRY_TEXT {
        return s.to_string();
    }
    let mut end = MAX_ENTRY_TEXT;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

fn remote_object_text(o: &Value) -> String {
    if let Some(v) = o.get("value") {
        return match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
    }
    if let Some(d) = o.get("description").and_then(Value::as_str) {
        return d.to_string();
    }
    if let Some(u) = o.get("unserializableValue").and_then(Value::as_str) {
        return u.to_string();
    }
    o.get("type").and_then(Value::as_str).unwrap_or("?").to_string()
}

/// Partial entry (seq/ts filled in by the caller).
#[derive(Debug, Clone, PartialEq)]
pub struct RawEntry {
    pub level: &'static str,
    pub source: &'static str,
    pub text: String,
    pub url: Option<String>,
}

/// Converts a CDP event into a console entry, when relevant.
/// `request_url` resolves a network requestId to its URL.
pub fn entry_from_event(method: &str, p: &Value, request_url: impl Fn(&str) -> Option<String>) -> Option<RawEntry> {
    match method {
        "Runtime.consoleAPICalled" => {
            let ty = p.get("type").and_then(Value::as_str).unwrap_or("log");
            let level = match ty {
                "error" | "assert" => "error",
                "warning" => "warning",
                "info" => "info",
                "debug" => "debug",
                _ => "log",
            };
            let text = p
                .get("args")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(remote_object_text).collect::<Vec<_>>().join(" "))
                .unwrap_or_default();
            let url = p
                .pointer("/stackTrace/callFrames/0/url")
                .and_then(Value::as_str)
                .filter(|u| !u.is_empty())
                .map(String::from);
            Some(RawEntry { level, source: "console", text: clip(&text), url })
        }
        "Runtime.exceptionThrown" => {
            let d = p.get("exceptionDetails")?;
            let text = d
                .pointer("/exception/description")
                .and_then(Value::as_str)
                .or_else(|| d.get("text").and_then(Value::as_str))
                .unwrap_or("Exceção não tratada");
            let url = d.get("url").and_then(Value::as_str).filter(|u| !u.is_empty()).map(|u| {
                match (d.get("lineNumber").and_then(Value::as_i64), d.get("columnNumber").and_then(Value::as_i64)) {
                    (Some(l), Some(c)) => format!("{u}:{}:{}", l + 1, c + 1),
                    _ => u.to_string(),
                }
            });
            Some(RawEntry { level: "error", source: "exception", text: clip(text), url })
        }
        "Log.entryAdded" => {
            let e = p.get("entry")?;
            // Network problems are reported precisely by the Network domain.
            if e.get("source").and_then(Value::as_str) == Some("network") {
                return None;
            }
            let level = match e.get("level").and_then(Value::as_str).unwrap_or("info") {
                "error" => "error",
                "warning" => "warning",
                "verbose" => "debug",
                _ => "info",
            };
            let text = e.get("text").and_then(Value::as_str).unwrap_or_default();
            let url = e.get("url").and_then(Value::as_str).filter(|u| !u.is_empty()).map(String::from);
            Some(RawEntry { level, source: "log", text: clip(text), url })
        }
        "Network.responseReceived" => {
            let r = p.get("response")?;
            let status = r.get("status").and_then(Value::as_i64).unwrap_or(0);
            if status < 400 {
                return None;
            }
            let url = r.get("url").and_then(Value::as_str).unwrap_or_default().to_string();
            let st = r.get("statusText").and_then(Value::as_str).unwrap_or("").trim();
            let text = if st.is_empty() { format!("HTTP {status}") } else { format!("HTTP {status} {st}") };
            Some(RawEntry { level: "error", source: "network", text, url: Some(url) })
        }
        "Network.loadingFailed" => {
            if p.get("canceled").and_then(Value::as_bool) == Some(true) {
                return None;
            }
            let id = p.get("requestId").and_then(Value::as_str).unwrap_or_default();
            let err = p.get("errorText").and_then(Value::as_str).unwrap_or("falha de rede");
            let blocked = p.get("blockedReason").and_then(Value::as_str);
            let text = match blocked {
                Some(b) => format!("{err} (bloqueado: {b})"),
                None => err.to_string(),
            };
            Some(RawEntry { level: "error", source: "network", text, url: request_url(id) })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_urls() {
        assert_eq!(normalize_url("localhost:3000").unwrap(), "http://localhost:3000/");
        assert_eq!(normalize_url("  127.0.0.1:5173/app ").unwrap(), "http://127.0.0.1:5173/app");
        assert_eq!(normalize_url("http://0.0.0.0:8080/x").unwrap(), "http://localhost:8080/x");
        assert_eq!(normalize_url("0.0.0.0:8080").unwrap(), "http://localhost:8080/");
        assert_eq!(normalize_url("example.com").unwrap(), "https://example.com/");
        assert_eq!(normalize_url("https://example.com/a?b=1#c").unwrap(), "https://example.com/a?b=1#c");
        assert_eq!(normalize_url("app.localhost:3000").unwrap(), "http://app.localhost:3000/");
        assert_eq!(normalize_url("about:blank").unwrap(), "about:blank");
    }

    #[test]
    fn refuses_dangerous_or_invalid_urls() {
        for bad in ["file:///etc/passwd", "javascript:alert(1)", "data:text/html,hi", "chrome://settings", "view-source:https://x.com", "ftp://x.com/a"] {
            assert!(normalize_url(bad).is_err(), "{bad}");
        }
        for bad in ["", "   ", "hello", "two words.com"] {
            assert!(normalize_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn maps_console_and_exceptions() {
        let e = entry_from_event(
            "Runtime.consoleAPICalled",
            &json!({"type":"error","args":[{"type":"string","value":"boom"},{"type":"number","value":42},{"type":"object","description":"Object"}],
                    "stackTrace":{"callFrames":[{"url":"http://localhost/app.js"}]}}),
            |_| None,
        )
        .unwrap();
        assert_eq!((e.level, e.source, e.text.as_str()), ("error", "console", "boom 42 Object"));
        assert_eq!(e.url.as_deref(), Some("http://localhost/app.js"));
        let w = entry_from_event("Runtime.consoleAPICalled", &json!({"type":"warning","args":[]}), |_| None).unwrap();
        assert_eq!(w.level, "warning");
        let x = entry_from_event(
            "Runtime.exceptionThrown",
            &json!({"exceptionDetails":{"text":"Uncaught","url":"http://l/a.js","lineNumber":9,"columnNumber":4,
                    "exception":{"description":"TypeError: x is undefined\n    at a.js:10:5"}}}),
            |_| None,
        )
        .unwrap();
        assert_eq!(x.level, "error");
        assert!(x.text.starts_with("TypeError"));
        assert_eq!(x.url.as_deref(), Some("http://l/a.js:10:5"));
    }

    #[test]
    fn maps_network_problems_and_skips_noise() {
        let r = entry_from_event(
            "Network.responseReceived",
            &json!({"response":{"url":"http://l/api","status":404,"statusText":"Not Found"}}),
            |_| None,
        )
        .unwrap();
        assert_eq!(r.text, "HTTP 404 Not Found");
        assert!(entry_from_event("Network.responseReceived", &json!({"response":{"url":"u","status":200}}), |_| None).is_none());
        let f = entry_from_event(
            "Network.loadingFailed",
            &json!({"requestId":"7","errorText":"net::ERR_CONNECTION_REFUSED"}),
            |id| (id == "7").then(|| "http://l/x.js".to_string()),
        )
        .unwrap();
        assert_eq!(f.url.as_deref(), Some("http://l/x.js"));
        assert!(entry_from_event("Network.loadingFailed", &json!({"requestId":"7","errorText":"x","canceled":true}), |_| None).is_none());
        assert!(entry_from_event("Log.entryAdded", &json!({"entry":{"source":"network","level":"error","text":"404"}}), |_| None).is_none());
        let l = entry_from_event("Log.entryAdded", &json!({"entry":{"source":"security","level":"error","text":"Mixed content"}}), |_| None).unwrap();
        assert_eq!((l.level, l.source), ("error", "log"));
        assert!(entry_from_event("Page.loadEventFired", &json!({}), |_| None).is_none());
    }

    #[test]
    fn clips_long_text_on_char_boundary() {
        let s = "é".repeat(MAX_ENTRY_TEXT);
        let c = clip(&s);
        assert!(c.len() <= MAX_ENTRY_TEXT + 3);
        assert!(c.ends_with('…'));
    }
}
