//! Minimal MCP client used to verify servers: initialize → initialized →
//! tools/list (paginated). Supports stdio (newline-delimited JSON-RPC) and
//! Streamable HTTP (JSON or SSE responses, `Mcp-Session-Id`).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::environment;
use crate::redact::sanitize;

pub const PROTOCOL_VERSION: &str = "2025-06-18";
const MAX_PAGES: usize = 20;

/// Fully resolved connection parameters (secrets already substituted).
#[derive(Clone)]
pub struct Endpoint {
    pub transport: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub url: Option<String>,
    pub headers: HashMap<String, String>,
    /// Values to scrub from logs/errors (the resolved secrets).
    pub secret_values: Vec<String>,
}

// Debug never prints resolved values (they may contain secrets).
impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint").field("transport", &self.transport).field("command", &self.command).finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub ok: bool,
    pub protocol_version: Option<String>,
    pub server_info: Option<Value>,
    pub tools: Vec<ToolInfo>,
    pub error: Option<String>,
    pub log: String,
    pub duration_ms: i64,
}

fn scrub(text: &str, secrets: &[String]) -> String {
    let mut s = text.to_string();
    for v in secrets.iter().filter(|v| v.len() >= 4) {
        s = s.replace(v.as_str(), "[REDACTED]");
    }
    sanitize(&s, 16 * 1024)
}

fn init_params() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {},
        "clientInfo": { "name": "FORGE", "version": env!("CARGO_PKG_VERSION") }
    })
}

fn parse_tools(result: &Value) -> Vec<ToolInfo> {
    result
        .get("tools")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|t| {
                    Some(ToolInfo {
                        name: t.get("name")?.as_str()?.to_string(),
                        description: t.get("description").and_then(Value::as_str).unwrap_or_default().chars().take(2000).collect(),
                        input_schema: t.get("inputSchema").cloned().unwrap_or(Value::Null),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn rpc_error(v: &Value) -> Option<String> {
    v.get("error").map(|e| format!("{} (código {})", e.get("message").and_then(Value::as_str).unwrap_or("erro"), e.get("code").and_then(Value::as_i64).unwrap_or(0)))
}

/// Runs the handshake + tools/list against the endpoint.
pub fn probe(ep: &Endpoint, timeout: Duration) -> ProbeResult {
    let start = Instant::now();
    let mut log = Vec::new();
    let outcome = match ep.transport.as_str() {
        "stdio" => probe_stdio(ep, timeout, &mut log),
        "http" => probe_http(ep, timeout, &mut log),
        other => Err(format!("transporte desconhecido: {other}")),
    };
    let duration_ms = start.elapsed().as_millis() as i64;
    let log = scrub(&log.join("\n"), &ep.secret_values);
    match outcome {
        Ok((pv, info, tools)) => ProbeResult { ok: true, protocol_version: pv, server_info: Some(info), tools, error: None, log, duration_ms },
        Err(e) => ProbeResult { ok: false, protocol_version: None, server_info: None, tools: vec![], error: Some(scrub(&e, &ep.secret_values)), log, duration_ms },
    }
}

type Handshake = (Option<String>, Value, Vec<ToolInfo>);

// ── stdio ─────────────────────────────────────────────────────────────────

struct StdioConn {
    child: Child,
    stdin: std::process::ChildStdin,
    rx: mpsc::Receiver<Value>,
    deadline: Instant,
    next_id: i64,
}

impl StdioConn {
    fn send(&mut self, msg: &Value) -> Result<(), String> {
        let line = format!("{msg}\n");
        self.stdin.write_all(line.as_bytes()).and_then(|_| self.stdin.flush()).map_err(|e| format!("falha ao escrever no servidor: {e}"))
    }

    fn request(&mut self, method: &str, params: Value, log: &mut Vec<String>) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))?;
        log.push(format!("→ {method}"));
        loop {
            let left = self.deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(format!("tempo esgotado aguardando resposta de {method}"));
            }
            let msg = self.rx.recv_timeout(left).map_err(|_| format!("o servidor encerrou ou não respondeu a {method}"))?;
            // Server → client requests (sampling, roots…): politely refuse.
            if msg.get("method").is_some() && msg.get("id").is_some() {
                let _ = self.send(&json!({ "jsonrpc": "2.0", "id": msg["id"], "error": { "code": -32601, "message": "not supported by FORGE probe" } }));
                continue;
            }
            if msg.get("id").and_then(Value::as_i64) == Some(id) {
                if let Some(e) = rpc_error(&msg) {
                    log.push(format!("← {method}: erro {e}"));
                    return Err(format!("{method} falhou: {e}"));
                }
                log.push(format!("← {method}: ok"));
                return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
            }
        }
    }
}

fn kill_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill").args(["-TERM", &format!("-{}", child.id())]).status();
        std::thread::sleep(Duration::from_millis(100));
        let _ = Command::new("kill").args(["-KILL", &format!("-{}", child.id())]).status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn probe_stdio(ep: &Endpoint, timeout: Duration, log: &mut Vec<String>) -> Result<Handshake, String> {
    let command = ep.command.as_deref().filter(|c| !c.trim().is_empty()).ok_or("comando não definido")?;
    let program = if command.contains('/') { std::path::PathBuf::from(command) } else { environment::which(command).ok_or_else(|| format!("comando não encontrado no PATH: {command}"))? };
    let mut cmd = Command::new(&program);
    let base = environment::login_env();
    if !base.is_empty() {
        cmd.env_clear().envs(base);
    }
    cmd.env("PATH", environment::search_path()).envs(&ep.env).args(&ep.args);
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    log.push(format!("$ {} {}", command, ep.args.join(" ")));
    let mut child = cmd.spawn().map_err(|e| format!("falha ao iniciar {command}: {e}"))?;
    let stdout = child.stdout.take().ok_or("stdout indisponível")?;
    let stdin = child.stdin.take().ok_or("stdin indisponível")?;
    let mut stderr = child.stderr.take();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Ok(v) = serde_json::from_str::<Value>(line.trim()) {
                if tx.send(v).is_err() {
                    break;
                }
            }
        }
    });
    let err_t = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(s) = stderr.as_mut() {
            let _ = s.take(64 * 1024).read_to_end(&mut buf);
        }
        String::from_utf8_lossy(&buf).into_owned()
    });
    let mut conn = StdioConn { child, stdin, rx, deadline: Instant::now() + timeout, next_id: 0 };
    let result = (|| {
        let init = conn.request("initialize", init_params(), log)?;
        conn.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))?;
        let pv = init.get("protocolVersion").and_then(Value::as_str).map(str::to_string);
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let params = match &cursor {
                Some(c) => json!({ "cursor": c }),
                None => json!({}),
            };
            let page = conn.request("tools/list", params, log)?;
            tools.extend(parse_tools(&page));
            cursor = page.get("nextCursor").and_then(Value::as_str).map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        Ok((pv, init, tools))
    })();
    kill_tree(&mut conn.child);
    let stderr_text = err_t.join().unwrap_or_default();
    if !stderr_text.trim().is_empty() {
        log.push("stderr:".into());
        log.extend(stderr_text.lines().rev().take(40).collect::<Vec<_>>().into_iter().rev().map(str::to_string));
    }
    result
}

// ── Streamable HTTP ───────────────────────────────────────────────────────

/// Extracts the JSON-RPC message with `id` from a JSON or SSE body.
pub fn parse_http_body(content_type: &str, body: &str, id: i64) -> Option<Value> {
    if content_type.contains("text/event-stream") {
        let mut data = String::new();
        let mut found = None;
        let mut flush = |data: &mut String| {
            if !data.is_empty() {
                if let Ok(v) = serde_json::from_str::<Value>(data) {
                    if v.get("id").and_then(Value::as_i64) == Some(id) && found.is_none() {
                        found = Some(v);
                    }
                }
                data.clear();
            }
        };
        for line in body.lines() {
            if let Some(d) = line.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(d.trim_start());
            } else if line.trim().is_empty() {
                flush(&mut data);
            }
        }
        flush(&mut data);
        found
    } else {
        let v: Value = serde_json::from_str(body).ok()?;
        match v {
            Value::Array(items) => items.into_iter().find(|m| m.get("id").and_then(Value::as_i64) == Some(id)),
            other => Some(other),
        }
    }
}

fn probe_http(ep: &Endpoint, timeout: Duration, log: &mut Vec<String>) -> Result<Handshake, String> {
    let url = ep.url.as_deref().filter(|u| u.starts_with("https://") || u.starts_with("http://")).ok_or("URL inválida (use http:// ou https://)")?;
    let client = reqwest::blocking::Client::builder().timeout(timeout).build().map_err(|e| e.to_string())?;
    let mut session: Option<String> = None;
    let mut protocol: Option<String> = None;
    let mut id = 0i64;
    let mut call = |method: &str, params: Option<Value>, log: &mut Vec<String>, session: &mut Option<String>, protocol: &Option<String>| -> Result<Option<Value>, String> {
        let is_request = params.is_some();
        let body = if is_request {
            id += 1;
            json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
        } else {
            json!({ "jsonrpc": "2.0", "method": method })
        };
        let mut req = client.post(url).header("Content-Type", "application/json").header("Accept", "application/json, text/event-stream");
        for (k, v) in &ep.headers {
            req = req.header(k, v);
        }
        if let Some(s) = session.as_ref() {
            req = req.header("Mcp-Session-Id", s);
        }
        if let Some(p) = protocol {
            req = req.header("MCP-Protocol-Version", p);
        }
        log.push(format!("→ POST {method}"));
        let resp = req.json(&body).send().map_err(|e| format!("falha de rede em {method}: {e}"))?;
        let status = resp.status();
        if let Some(s) = resp.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()) {
            *session = Some(s.to_string());
        }
        let ct = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
        let text = resp.text().map_err(|e| format!("falha ao ler resposta de {method}: {e}"))?;
        log.push(format!("← {method}: HTTP {}", status.as_u16()));
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(format!("{method}: acesso negado (HTTP {}) — verifique o token/credenciais do servidor", status.as_u16()));
        }
        if !status.is_success() {
            return Err(format!("{method}: HTTP {} — {}", status.as_u16(), text.chars().take(300).collect::<String>()));
        }
        if !is_request {
            return Ok(None);
        }
        let msg = parse_http_body(&ct, &text, id).ok_or_else(|| format!("{method}: resposta sem mensagem JSON-RPC correspondente"))?;
        if let Some(e) = rpc_error(&msg) {
            return Err(format!("{method} falhou: {e}"));
        }
        Ok(Some(msg.get("result").cloned().unwrap_or(Value::Null)))
    };
    let init = call("initialize", Some(init_params()), log, &mut session, &protocol)?.unwrap_or(Value::Null);
    protocol = init.get("protocolVersion").and_then(Value::as_str).map(str::to_string);
    call("notifications/initialized", None, log, &mut session, &protocol)?;
    let mut tools = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let params = match &cursor {
            Some(c) => json!({ "cursor": c }),
            None => json!({}),
        };
        let page = call("tools/list", Some(params), log, &mut session, &protocol)?.unwrap_or(Value::Null);
        tools.extend(parse_tools(&page));
        cursor = page.get("nextCursor").and_then(Value::as_str).map(str::to_string);
        if cursor.is_none() {
            break;
        }
    }
    if let Some(s) = &session {
        let mut req = client.delete(url).header("Mcp-Session-Id", s);
        for (k, v) in &ep.headers {
            req = req.header(k, v);
        }
        let _ = req.send();
    }
    Ok((protocol, init, tools))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny MCP server in Python (available on macOS/Linux dev machines) that
    /// paginates tools and sends a server→client request mid-handshake.
    #[cfg(unix)]
    fn fake_server(dir: &std::path::Path) -> std::path::PathBuf {
        let p = dir.join("fake_mcp.py");
        std::fs::write(
            &p,
            r#"import sys, json, os
def out(m):
    sys.stdout.write(json.dumps(m) + "\n"); sys.stdout.flush()
sys.stderr.write("starting with token " + os.environ.get("FAKE_TOKEN", "none") + "\n"); sys.stderr.flush()
for line in sys.stdin:
    m = json.loads(line)
    meth, mid = m.get("method"), m.get("id")
    if meth == "initialize":
        out({"jsonrpc":"2.0","method":"notifications/message","params":{"level":"info","data":"hi"}})
        out({"jsonrpc":"2.0","id":"srv-1","method":"roots/list","params":{}})
        out({"jsonrpc":"2.0","id":mid,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"fake","version":"1.0"}}})
    elif meth == "tools/list":
        cur = (m.get("params") or {}).get("cursor")
        if cur is None:
            out({"jsonrpc":"2.0","id":mid,"result":{"tools":[{"name":"echo","description":"Echo","inputSchema":{"type":"object"}}],"nextCursor":"p2"}})
        else:
            out({"jsonrpc":"2.0","id":mid,"result":{"tools":[{"name":"add","description":"Add","inputSchema":{"type":"object"}}]}})
"#,
        )
        .unwrap();
        p
    }

    fn ep(command: &str, args: Vec<String>) -> Endpoint {
        Endpoint { transport: "stdio".into(), command: Some(command.into()), args, env: HashMap::new(), url: None, headers: HashMap::new(), secret_values: vec![] }
    }

    #[cfg(unix)]
    #[test]
    fn stdio_handshake_paginates_and_scrubs_secrets() {
        if environment::which("python3").is_none() {
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let script = fake_server(d.path());
        let mut e = ep("python3", vec![script.display().to_string()]);
        e.env.insert("FAKE_TOKEN".into(), "super-secret-value-123".into());
        e.secret_values.push("super-secret-value-123".into());
        let r = probe(&e, Duration::from_secs(20));
        assert!(r.ok, "{:?} {}", r.error, r.log);
        assert_eq!(r.tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["echo", "add"]);
        assert_eq!(r.server_info.unwrap()["serverInfo"]["name"], "fake");
        assert!(!r.log.contains("super-secret-value-123"), "secret leaked into log: {}", r.log);
        assert!(r.log.contains("[REDACTED]"));
    }

    #[test]
    fn failures_are_reported() {
        let r = probe(&ep("definitely-not-a-command-forge", vec![]), Duration::from_secs(5));
        assert!(!r.ok && r.error.unwrap().contains("não encontrado"));
        #[cfg(unix)]
        {
            let r = probe(&ep("/bin/sh", vec!["-c".into(), "exit 0".into()]), Duration::from_secs(5));
            assert!(!r.ok && r.error.unwrap().contains("encerrou"));
            let start = Instant::now();
            let r = probe(&ep("/bin/sh", vec!["-c".into(), "sleep 30".into()]), Duration::from_millis(400));
            assert!(!r.ok && start.elapsed() < Duration::from_secs(5), "timeout honored");
        }
        let mut h = ep("x", vec![]);
        h.transport = "http".into();
        h.url = Some("file:///etc/passwd".into());
        assert!(probe(&h, Duration::from_secs(2)).error.unwrap().contains("URL inválida"));
    }

    #[test]
    fn parses_sse_and_json_bodies() {
        let sse = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\n\nevent: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":3,\n";
        let sse = format!("{sse}data: \"result\":{{\"tools\":[]}}}}\n\n");
        let m = parse_http_body("text/event-stream", &sse, 3).unwrap();
        assert_eq!(m["result"]["tools"], json!([]));
        let m = parse_http_body("application/json", r#"[{"id":1,"result":1},{"id":2,"result":2}]"#, 2).unwrap();
        assert_eq!(m["result"], 2);
        assert!(parse_http_body("text/event-stream", "data: nope\n\n", 1).is_none());
    }

    /// Live probe of a public remote server (no code downloaded).
    /// `cargo test mcp::client::tests::live_cloudflare -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_cloudflare_docs_http() {
        let e = Endpoint { transport: "http".into(), command: None, args: vec![], env: HashMap::new(), url: Some("https://docs.mcp.cloudflare.com/mcp".into()), headers: HashMap::new(), secret_values: vec![] };
        let r = probe(&e, Duration::from_secs(60));
        eprintln!("ok={} pv={:?} tools={:?} err={:?}\n{}", r.ok, r.protocol_version, r.tools.iter().map(|t| &t.name).collect::<Vec<_>>(), r.error, r.log);
        assert!(r.ok && !r.tools.is_empty());
    }

    /// Downloads and runs the official filesystem server via npx — run only if you accept that.
    /// `cargo test mcp::client::tests::live_filesystem -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_filesystem_stdio_via_npx() {
        let d = tempfile::tempdir().unwrap();
        let dir = std::fs::canonicalize(d.path()).unwrap().display().to_string();
        let e = ep("npx", vec!["-y".into(), "@modelcontextprotocol/server-filesystem".into(), dir]);
        let r = probe(&e, Duration::from_secs(180));
        eprintln!("ok={} tools={:?} err={:?}", r.ok, r.tools.iter().map(|t| &t.name).collect::<Vec<_>>(), r.error);
        assert!(r.ok && !r.tools.is_empty());
    }
}
