//! Provider Engine: adapters for agent CLIs (Claude Code, Codex, Gemini).
//!
//! Principles:
//! - Native authentication of each CLI (subscriptions/API keys the user already
//!   has). FORGE never asks for, reads or stores credentials: login state is
//!   detected through the CLI's own status command or the *presence* of its
//!   config files / environment variables.
//! - A provider is "verified" only after a real headless run succeeded on the
//!   installed version (`verify`). Usage/cost are shown only when the CLI
//!   reports them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;

use crate::db::now_ms;
use crate::environment::{self, run_capture};
use crate::error::{AppError, AppResult};

const STATUS_TIMEOUT: Duration = Duration::from_secs(20);
const HEADLESS_TIMEOUT: Duration = Duration::from_secs(180);
pub const VERIFY_PROMPT: &str = "Reply with exactly this text and nothing else: FORGE_OK";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub interactive: bool,
    pub headless: bool,
    pub structured_output: bool,
    pub resume: bool,
    pub mcp: bool,
    pub sandbox: bool,
    pub approval_modes: Vec<&'static str>,
}

pub struct ProviderDef {
    pub id: &'static str,
    pub name: &'static str,
    pub vendor: &'static str,
    pub bin: &'static str,
    pub install: &'static str,
    pub docs: &'static str,
    /// Args for an interactive login flow (run inside a PTY).
    pub login_args: &'static [&'static str],
    pub login_hint: &'static str,
    /// Model aliases documented by the CLI itself (empty = free text / CLI default).
    pub model_aliases: &'static [&'static str],
    /// Cheapest model to use for the verification run (None = CLI default).
    pub verify_model: Option<&'static str>,
    pub model_flag: &'static str,
}

pub const PROVIDERS: &[ProviderDef] = &[
    ProviderDef {
        id: "claude",
        name: "Claude Code",
        vendor: "Anthropic",
        bin: "claude",
        install: "npm i -g @anthropic-ai/claude-code",
        docs: "https://docs.claude.com/en/docs/claude-code",
        login_args: &["auth", "login"],
        login_hint: "claude auth login",
        model_aliases: &["sonnet", "opus", "haiku"],
        verify_model: Some("haiku"),
        model_flag: "--model",
    },
    ProviderDef {
        id: "codex",
        name: "OpenAI Codex CLI",
        vendor: "OpenAI",
        bin: "codex",
        install: "npm i -g @openai/codex",
        docs: "https://github.com/openai/codex",
        login_args: &["login"],
        login_hint: "codex login",
        model_aliases: &[],
        verify_model: None,
        model_flag: "-m",
    },
    ProviderDef {
        id: "gemini",
        name: "Gemini CLI",
        vendor: "Google",
        bin: "gemini",
        install: "npm i -g @google/gemini-cli",
        docs: "https://github.com/google-gemini/gemini-cli",
        // Gemini chooses the auth method inside its interactive UI on first run.
        login_args: &[],
        login_hint: "gemini (escolha o método de login na primeira execução)",
        model_aliases: &[],
        verify_model: None,
        model_flag: "-m",
    },
];

pub fn def(id: &str) -> AppResult<&'static ProviderDef> {
    PROVIDERS.iter().find(|p| p.id == id).ok_or_else(|| AppError::NotFound(format!("provider desconhecido: {id}")))
}

fn capabilities(id: &str) -> Capabilities {
    match id {
        "claude" => Capabilities {
            interactive: true,
            headless: true,
            structured_output: true,
            resume: true,
            mcp: true,
            sandbox: false,
            approval_modes: vec!["default", "acceptEdits", "plan", "bypassPermissions"],
        },
        "codex" => Capabilities {
            interactive: true,
            headless: true,
            structured_output: true,
            resume: true,
            mcp: true,
            sandbox: true,
            approval_modes: vec!["read-only", "workspace-write", "danger-full-access"],
        },
        _ => Capabilities {
            interactive: true,
            headless: true,
            structured_output: true,
            resume: true,
            mcp: true,
            sandbox: true,
            approval_modes: vec!["default", "auto_edit", "plan", "yolo"],
        },
    }
}

// ── Status ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    /// "logged_in" | "logged_out" | "unknown"
    pub state: &'static str,
    pub method: Option<String>,
    pub plan: Option<String>,
    pub detail: Option<String>,
}

impl AuthStatus {
    fn unknown(detail: impl Into<String>) -> Self {
        Self { state: "unknown", method: None, plan: None, detail: Some(detail.into()) }
    }
    fn out(detail: impl Into<String>) -> Self {
        Self { state: "logged_out", method: None, plan: None, detail: Some(detail.into()) }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCheck {
    pub ok: bool,
    pub version: Option<String>,
    pub model: Option<String>,
    pub message: String,
    pub duration_ms: i64,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cost_usd: Option<f64>,
    pub checked_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub auth: AuthStatus,
    pub configured_model: Option<String>,
    pub model_aliases: Vec<String>,
    pub capabilities: Capabilities,
    pub install: String,
    pub login_hint: String,
    pub docs: String,
    pub last_check: Option<ProviderCheck>,
    /// Last check succeeded on the currently installed version.
    pub verified: bool,
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

/// Command for a provider binary with the user's login environment.
pub fn provider_command(bin: &Path) -> Command {
    let mut cmd = Command::new(bin);
    let env = environment::login_env();
    if !env.is_empty() {
        cmd.env_clear().envs(env);
    }
    cmd.env("PATH", environment::search_path());
    cmd.env("NO_COLOR", "1");
    cmd
}

pub fn parse_claude_auth(json: &str) -> AuthStatus {
    let Ok(v) = serde_json::from_str::<Value>(json) else {
        return AuthStatus::unknown("saída de `claude auth status` não reconhecida");
    };
    if v.get("loggedIn").and_then(Value::as_bool) == Some(true) {
        AuthStatus {
            state: "logged_in",
            method: v.get("authMethod").and_then(Value::as_str).map(str::to_string),
            plan: v.get("subscriptionType").and_then(Value::as_str).map(str::to_string),
            detail: None,
        }
    } else {
        AuthStatus::out("execute `claude auth login`")
    }
}

pub fn parse_codex_login(success: bool, text: &str) -> AuthStatus {
    let t = text.trim();
    if success && t.to_lowercase().starts_with("logged in") {
        let method = t.split(" using ").nth(1).map(|m| m.trim().trim_end_matches('.').to_string());
        AuthStatus { state: "logged_in", method, plan: None, detail: None }
    } else if t.to_lowercase().contains("not logged in") {
        AuthStatus::out("execute `codex login`")
    } else {
        AuthStatus::unknown(t.lines().next().unwrap_or("status desconhecido").to_string())
    }
}

/// Gemini has no status command: inspect the selected auth type and the
/// presence (never the content) of its credential file / env vars.
pub fn detect_gemini_auth(gemini_dir: &Path, env: &HashMap<String, String>) -> AuthStatus {
    let settings: Value = std::fs::read_to_string(gemini_dir.join("settings.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null);
    let selected = settings
        .pointer("/security/auth/selectedType")
        .or_else(|| settings.get("selectedAuthType"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let has_env = |k: &str| env.get(k).is_some_and(|v| !v.is_empty());
    let has_dotenv = gemini_dir.join(".env").is_file();
    match selected.as_deref() {
        Some("oauth-personal") => {
            if gemini_dir.join("oauth_creds.json").is_file() {
                AuthStatus { state: "logged_in", method: Some("Google (OAuth)".into()), plan: None, detail: None }
            } else {
                AuthStatus::out("login Google selecionado, mas sem credenciais salvas — execute `gemini`")
            }
        }
        Some("gemini-api-key") => {
            if has_env("GEMINI_API_KEY") || has_dotenv {
                AuthStatus { state: "logged_in", method: Some("API key (GEMINI_API_KEY)".into()), plan: None, detail: None }
            } else {
                AuthStatus::out("método API key selecionado, mas GEMINI_API_KEY não está no ambiente")
            }
        }
        Some("vertex-ai") => AuthStatus { state: "logged_in", method: Some("Vertex AI".into()), plan: None, detail: Some("credenciais do Google Cloud".into()) },
        Some(other) => AuthStatus { state: "unknown", method: Some(other.into()), plan: None, detail: None },
        None if has_env("GEMINI_API_KEY") || has_env("GOOGLE_API_KEY") => {
            AuthStatus { state: "logged_in", method: Some("API key (ambiente)".into()), plan: None, detail: None }
        }
        None => AuthStatus::out("nenhum método de login configurado — execute `gemini`"),
    }
}

/// Reads the model configured in the CLI's own (non-secret) settings.
pub fn configured_model(id: &str, home: &Path) -> Option<String> {
    match id {
        "claude" => {
            let v: Value = serde_json::from_str(&std::fs::read_to_string(home.join(".claude/settings.json")).ok()?).ok()?;
            v.get("model").and_then(Value::as_str).map(str::to_string)
        }
        "codex" => {
            let s = std::fs::read_to_string(home.join(".codex/config.toml")).ok()?;
            s.lines()
                .take_while(|l| !l.trim_start().starts_with('['))
                .find_map(|l| {
                    let (k, v) = l.split_once('=')?;
                    (k.trim() == "model").then(|| v.trim().trim_matches('"').trim_matches('\'').to_string())
                })
        }
        "gemini" => {
            let v: Value = serde_json::from_str(&std::fs::read_to_string(home.join(".gemini/settings.json")).ok()?).ok()?;
            v.pointer("/model/name").or_else(|| v.get("model")).and_then(Value::as_str).map(str::to_string)
        }
        _ => None,
    }
}

fn detect_auth(p: &ProviderDef, bin: &Path) -> AuthStatus {
    match p.id {
        "claude" => {
            let mut cmd = provider_command(bin);
            cmd.args(["auth", "status", "--json"]);
            match run_capture(cmd, STATUS_TIMEOUT, 64 * 1024, 64 * 1024) {
                Ok(Some(out)) => parse_claude_auth(&String::from_utf8_lossy(&out.stdout)),
                Ok(None) => AuthStatus::unknown("`claude auth status` excedeu o tempo"),
                Err(e) => AuthStatus::unknown(format!("falha ao executar: {e}")),
            }
        }
        "codex" => {
            let mut cmd = provider_command(bin);
            cmd.args(["login", "status"]);
            match run_capture(cmd, STATUS_TIMEOUT, 64 * 1024, 64 * 1024) {
                Ok(Some(out)) => {
                    let text = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
                    parse_codex_login(out.status_ok, text.trim())
                }
                Ok(None) => AuthStatus::unknown("`codex login status` excedeu o tempo"),
                Err(e) => AuthStatus::unknown(format!("falha ao executar: {e}")),
            }
        }
        "gemini" => match home() {
            Some(h) => detect_gemini_auth(&h.join(".gemini"), environment::login_env()),
            None => AuthStatus::unknown("diretório home não encontrado"),
        },
        _ => AuthStatus::unknown("sem detector"),
    }
}

pub fn status(p: &ProviderDef, last_check: Option<ProviderCheck>) -> ProviderStatus {
    let tool = environment::TOOLS.iter().find(|t| t.id == p.id).map(environment::detect);
    let installed = tool.as_ref().is_some_and(|t| t.installed);
    let version = tool.as_ref().and_then(|t| t.version.clone());
    let path = tool.as_ref().and_then(|t| t.path.clone());
    let auth = match (&path, installed) {
        (Some(bin), true) => detect_auth(p, Path::new(bin)),
        _ => AuthStatus::unknown("não instalado"),
    };
    let verified = last_check.as_ref().is_some_and(|c| c.ok && c.version.is_some() && c.version == version);
    ProviderStatus {
        id: p.id.into(),
        name: p.name.into(),
        vendor: p.vendor.into(),
        installed,
        version,
        path,
        auth,
        configured_model: home().and_then(|h| configured_model(p.id, &h)),
        model_aliases: p.model_aliases.iter().map(|s| s.to_string()).collect(),
        capabilities: capabilities(p.id),
        install: p.install.into(),
        login_hint: p.login_hint.into(),
        docs: p.docs.into(),
        last_check,
        verified,
    }
}

// ── Headless runs ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HeadlessResult {
    pub ok: bool,
    pub text: Option<String>,
    pub error: Option<String>,
    pub model: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub cache_write_tokens: Option<i64>,
    /// Cost as reported by the CLI (Claude Code reports an API-equivalent value).
    pub cost_usd: Option<f64>,
    pub reported_duration_ms: Option<i64>,
}

fn json_lines(s: &str) -> impl Iterator<Item = Value> + '_ {
    s.lines().filter_map(|l| {
        let l = l.trim();
        l.starts_with('{').then(|| serde_json::from_str(l).ok()).flatten()
    })
}

fn int(v: &Value, key: &str) -> Option<i64> {
    v.get(key).and_then(Value::as_i64)
}

/// Claude Code `--output-format stream-json --verbose` (NDJSON).
pub fn parse_claude_stream(stdout: &str) -> HeadlessResult {
    let mut r = HeadlessResult::default();
    let mut text = String::new();
    let mut saw_result = false;
    for ev in json_lines(stdout) {
        match ev.get("type").and_then(Value::as_str) {
            Some("system") if ev.get("subtype").and_then(Value::as_str) == Some("init") => {
                r.model = ev.get("model").and_then(Value::as_str).map(str::to_string);
            }
            Some("assistant") => {
                if let Some(parts) = ev.pointer("/message/content").and_then(Value::as_array) {
                    for p in parts.iter().filter(|p| p.get("type").and_then(Value::as_str) == Some("text")) {
                        if let Some(t) = p.get("text").and_then(Value::as_str) {
                            text.push_str(t);
                        }
                    }
                }
            }
            Some("result") => {
                saw_result = true;
                let is_error = ev.get("is_error").and_then(Value::as_bool).unwrap_or(true);
                r.ok = !is_error && ev.get("subtype").and_then(Value::as_str) == Some("success");
                let res = ev.get("result").and_then(Value::as_str).map(str::to_string);
                if r.ok {
                    r.text = res.or_else(|| (!text.is_empty()).then(|| text.clone()));
                } else {
                    r.error = res.or_else(|| ev.get("subtype").and_then(Value::as_str).map(str::to_string));
                }
                if let Some(u) = ev.get("usage") {
                    r.input_tokens = int(u, "input_tokens");
                    r.output_tokens = int(u, "output_tokens");
                    r.cache_read_tokens = int(u, "cache_read_input_tokens");
                    r.cache_write_tokens = int(u, "cache_creation_input_tokens");
                }
                r.cost_usd = ev.get("total_cost_usd").and_then(Value::as_f64);
                r.reported_duration_ms = int(&ev, "duration_ms");
            }
            _ => {}
        }
    }
    if !saw_result {
        r.ok = false;
        r.error = Some("o Claude Code não retornou um evento `result`".into());
        if !text.is_empty() {
            r.text = Some(text);
        }
    }
    r
}

/// Codex `exec --json` (JSONL events).
pub fn parse_codex_jsonl(stdout: &str) -> HeadlessResult {
    let mut r = HeadlessResult::default();
    let mut completed = false;
    let mut last_error: Option<String> = None;
    for ev in json_lines(stdout) {
        match ev.get("type").and_then(Value::as_str) {
            Some("item.completed") => {
                let item = ev.get("item").cloned().unwrap_or(Value::Null);
                match item.get("type").and_then(Value::as_str) {
                    Some("agent_message") => r.text = item.get("text").and_then(Value::as_str).map(str::to_string),
                    Some("error") => last_error = item.get("message").and_then(Value::as_str).map(str::to_string),
                    _ => {}
                }
            }
            Some("turn.completed") => {
                completed = true;
                if let Some(u) = ev.get("usage") {
                    r.input_tokens = int(u, "input_tokens");
                    r.output_tokens = int(u, "output_tokens");
                    r.cache_read_tokens = int(u, "cached_input_tokens");
                }
            }
            Some("turn.failed") => {
                last_error = ev.pointer("/error/message").and_then(Value::as_str).map(str::to_string).or(last_error);
            }
            Some("error") => last_error = ev.get("message").and_then(Value::as_str).map(str::to_string),
            _ => {}
        }
    }
    r.ok = completed && r.text.is_some();
    if !r.ok {
        r.error = last_error.or_else(|| Some("o Codex não concluiu o turno".into()));
    }
    r
}

fn first_json_object(s: &str) -> Option<Value> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    serde_json::from_str(&s[start..=end]).ok()
}

/// Gemini `-p … -o json`: a single JSON document; errors come on stderr.
pub fn parse_gemini_json(stdout: &str, stderr: &str) -> HeadlessResult {
    let mut r = HeadlessResult::default();
    let doc = first_json_object(stdout).or_else(|| first_json_object(stderr));
    let Some(doc) = doc else {
        r.error = Some(stderr.lines().find(|l| !l.trim().is_empty()).unwrap_or("o Gemini não retornou JSON").to_string());
        return r;
    };
    if let Some(msg) = doc.pointer("/error/message").and_then(Value::as_str) {
        r.error = Some(msg.to_string());
        return r;
    }
    r.text = doc.get("response").and_then(Value::as_str).map(str::to_string);
    if let Some(models) = doc.pointer("/stats/models").and_then(Value::as_object) {
        let (mut input, mut output, mut cached) = (0i64, 0i64, 0i64);
        for (name, m) in models {
            r.model.get_or_insert_with(|| name.clone());
            if let Some(t) = m.get("tokens") {
                input += int(t, "prompt").unwrap_or(0);
                output += int(t, "candidates").unwrap_or(0);
                cached += int(t, "cached").unwrap_or(0);
            }
        }
        if !models.is_empty() {
            r.input_tokens = Some(input);
            r.output_tokens = Some(output);
            r.cache_read_tokens = Some(cached);
        }
    }
    r.ok = r.text.is_some();
    if !r.ok {
        r.error = Some("resposta sem o campo `response`".into());
    }
    r
}

fn valid_model(m: &str) -> AppResult<()> {
    if m.is_empty() || m.len() > 100 || m.starts_with('-') || !m.chars().all(|c| c.is_ascii_alphanumeric() || "._:/-".contains(c)) {
        return Err(AppError::Invalid(format!("modelo inválido: {m}")));
    }
    Ok(())
}

/// Args for an interactive session inside a PTY (`mode`: "agent" | "login").
pub fn interactive_args(p: &ProviderDef, mode: &str, model: Option<&str>) -> AppResult<Vec<String>> {
    if mode == "login" {
        return Ok(p.login_args.iter().map(|s| s.to_string()).collect());
    }
    let mut args = Vec::new();
    if let Some(m) = model.filter(|m| !m.is_empty()) {
        valid_model(m)?;
        args.push(p.model_flag.to_string());
        args.push(m.to_string());
    }
    Ok(args)
}

/// Runs one headless prompt and parses the CLI's structured output.
pub fn run_headless(p: &ProviderDef, cwd: &Path, prompt: &str, model: Option<&str>) -> AppResult<(HeadlessResult, i64)> {
    let bin = environment::which(p.bin).ok_or_else(|| AppError::NotFound(format!("{} não está instalado", p.name)))?;
    let mut cmd = provider_command(&bin);
    cmd.current_dir(cwd);
    match p.id {
        "claude" => {
            cmd.args(["-p", prompt, "--output-format", "stream-json", "--verbose", "--permission-mode", "plan"]);
        }
        "codex" => {
            cmd.args(["exec", "--json", "--skip-git-repo-check", "--sandbox", "read-only", prompt]);
        }
        "gemini" => {
            cmd.args(["-p", prompt, "-o", "json", "--approval-mode", "plan"]);
        }
        _ => return Err(AppError::Invalid("provider sem modo headless".into())),
    }
    if let Some(m) = model {
        valid_model(m)?;
        cmd.args([p.model_flag, m]);
    }
    let start = Instant::now();
    let out = run_capture(cmd, HEADLESS_TIMEOUT, 8 * 1024 * 1024, 256 * 1024)?
        .ok_or_else(|| AppError::Internal(format!("{} excedeu o tempo limite", p.name)))?;
    let elapsed = start.elapsed().as_millis() as i64;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = crate::git::redact(&String::from_utf8_lossy(&out.stderr));
    let mut r = match p.id {
        "claude" => parse_claude_stream(&stdout),
        "codex" => parse_codex_jsonl(&stdout),
        _ => parse_gemini_json(&stdout, &stderr),
    };
    if !r.ok && r.error.is_none() {
        r.error = stderr.lines().rev().find(|l| !l.trim().is_empty()).map(str::to_string);
    }
    Ok((r, elapsed))
}

/// Real end-to-end check: a tiny prompt whose answer must contain FORGE_OK.
pub fn verify(p: &ProviderDef, scratch: &Path) -> AppResult<ProviderCheck> {
    let version = environment::TOOLS.iter().find(|t| t.id == p.id).map(environment::detect).and_then(|t| t.version);
    std::fs::create_dir_all(scratch)?;
    let (r, elapsed) = run_headless(p, scratch, VERIFY_PROMPT, p.verify_model)?;
    let answered = r.text.as_deref().is_some_and(|t| t.contains("FORGE_OK"));
    let ok = r.ok && answered;
    let message = if ok {
        "Resposta recebida e conferida (FORGE_OK)".to_string()
    } else if r.ok {
        format!("Resposta inesperada: {}", r.text.unwrap_or_default().chars().take(200).collect::<String>())
    } else {
        r.error.clone().unwrap_or_else(|| "falha sem mensagem".into()).chars().take(500).collect()
    };
    Ok(ProviderCheck {
        ok,
        version,
        model: r.model,
        message,
        duration_ms: r.reported_duration_ms.unwrap_or(elapsed),
        input_tokens: r.input_tokens,
        output_tokens: r.output_tokens,
        cost_usd: r.cost_usd,
        checked_at: now_ms(),
    })
}

// ── Persistence ───────────────────────────────────────────────────────────

pub fn save_check(conn: &Connection, id: &str, c: &ProviderCheck) -> AppResult<()> {
    conn.execute(
        "INSERT INTO provider_checks (provider_id, ok, version, model, message, duration_ms, input_tokens, output_tokens, cost_usd, checked_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(provider_id) DO UPDATE SET ok = excluded.ok, version = excluded.version, model = excluded.model,
           message = excluded.message, duration_ms = excluded.duration_ms, input_tokens = excluded.input_tokens,
           output_tokens = excluded.output_tokens, cost_usd = excluded.cost_usd, checked_at = excluded.checked_at",
        params![id, c.ok as i64, c.version, c.model, c.message, c.duration_ms, c.input_tokens, c.output_tokens, c.cost_usd, c.checked_at],
    )?;
    Ok(())
}

pub fn load_check(conn: &Connection, id: &str) -> AppResult<Option<ProviderCheck>> {
    Ok(conn
        .query_row(
            "SELECT ok, version, model, message, duration_ms, input_tokens, output_tokens, cost_usd, checked_at
             FROM provider_checks WHERE provider_id = ?1",
            [id],
            |r| {
                Ok(ProviderCheck {
                    ok: r.get::<_, i64>(0)? != 0,
                    version: r.get(1)?,
                    model: r.get(2)?,
                    message: r.get(3)?,
                    duration_ms: r.get(4)?,
                    input_tokens: r.get(5)?,
                    output_tokens: r.get(6)?,
                    cost_usd: r.get(7)?,
                    checked_at: r.get(8)?,
                })
            },
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_claude_stream_output() {
        let r = parse_claude_stream(include_str!("fixtures/claude_stream.jsonl"));
        assert!(r.ok);
        assert_eq!(r.text.as_deref(), Some("FORGE_OK"));
        assert_eq!(r.model.as_deref(), Some("claude-haiku-5-5"));
        assert_eq!(r.input_tokens, Some(2));
        assert_eq!(r.output_tokens, Some(65));
        assert_eq!(r.cache_read_tokens, Some(9013));
        assert!(r.cost_usd.unwrap() > 0.0);
        assert_eq!(r.reported_duration_ms, Some(2153));
    }

    #[test]
    fn claude_without_result_is_a_failure() {
        let r = parse_claude_stream(r#"{"type":"assistant","message":{"content":[{"type":"text","text":"oi"}]}}"#);
        assert!(!r.ok);
        assert!(r.error.is_some());
        let err = parse_claude_stream(r#"{"type":"result","subtype":"error_max_turns","is_error":true}"#);
        assert!(!err.ok);
        assert_eq!(err.error.as_deref(), Some("error_max_turns"));
    }

    #[test]
    fn parses_real_codex_unauthorized_output() {
        let r = parse_codex_jsonl(include_str!("fixtures/codex_unauthorized.jsonl"));
        assert!(!r.ok);
        assert!(r.error.unwrap().contains("401 Unauthorized"));
    }

    #[test]
    fn parses_documented_codex_success_shape() {
        let s = r#"{"type":"thread.started","thread_id":"t"}
{"type":"turn.started"}
{"type":"item.completed","item":{"id":"item_0","type":"reasoning","text":"..."}}
{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"FORGE_OK"}}
{"type":"turn.completed","usage":{"input_tokens":120,"cached_input_tokens":40,"output_tokens":5}}"#;
        let r = parse_codex_jsonl(s);
        assert!(r.ok);
        assert_eq!(r.text.as_deref(), Some("FORGE_OK"));
        assert_eq!((r.input_tokens, r.output_tokens, r.cache_read_tokens), (Some(120), Some(5), Some(40)));
        assert_eq!(r.cost_usd, None, "Codex does not report cost — never estimated");
    }

    #[test]
    fn parses_real_gemini_auth_error_and_documented_success() {
        let r = parse_gemini_json("", include_str!("fixtures/gemini_no_auth.stderr"));
        assert!(!r.ok);
        assert!(r.error.unwrap().contains("Auth method"));
        let ok = parse_gemini_json(
            r#"{"response":"FORGE_OK","stats":{"models":{"gemini-2.5-pro":{"tokens":{"prompt":10,"candidates":3,"cached":2,"total":13}}}}}"#,
            "",
        );
        assert!(ok.ok);
        assert_eq!(ok.model.as_deref(), Some("gemini-2.5-pro"));
        assert_eq!((ok.input_tokens, ok.output_tokens), (Some(10), Some(3)));
    }

    #[test]
    fn auth_parsers() {
        let a = parse_claude_auth(r#"{"loggedIn":true,"authMethod":"claude.ai","subscriptionType":"pro","email":"x@y.z"}"#);
        assert_eq!(a.state, "logged_in");
        assert_eq!(a.method.as_deref(), Some("claude.ai"));
        assert_eq!(a.plan.as_deref(), Some("pro"));
        assert!(!format!("{a:?}").contains("x@y.z"), "e-mail must not be kept");
        assert_eq!(parse_claude_auth(r#"{"loggedIn":false}"#).state, "logged_out");
        assert_eq!(parse_claude_auth("garbage").state, "unknown");

        assert_eq!(parse_codex_login(false, "Not logged in").state, "logged_out");
        let c = parse_codex_login(true, "Logged in using ChatGPT");
        assert_eq!((c.state, c.method.as_deref()), ("logged_in", Some("ChatGPT")));
    }

    #[test]
    fn gemini_auth_detection_uses_presence_only() {
        let d = tempfile::tempdir().unwrap();
        let empty = HashMap::new();
        assert_eq!(detect_gemini_auth(d.path(), &empty).state, "logged_out");
        std::fs::write(d.path().join("settings.json"), r#"{"security":{"auth":{"selectedType":"oauth-personal"}}}"#).unwrap();
        assert_eq!(detect_gemini_auth(d.path(), &empty).state, "logged_out");
        std::fs::write(d.path().join("oauth_creds.json"), "{}").unwrap();
        assert_eq!(detect_gemini_auth(d.path(), &empty).state, "logged_in");
        std::fs::write(d.path().join("settings.json"), r#"{"security":{"auth":{"selectedType":"gemini-api-key"}}}"#).unwrap();
        assert_eq!(detect_gemini_auth(d.path(), &empty).state, "logged_out");
        let env = HashMap::from([("GEMINI_API_KEY".to_string(), "k".to_string())]);
        assert_eq!(detect_gemini_auth(d.path(), &env).state, "logged_in");
    }

    #[test]
    fn reads_configured_models_and_validates() {
        let h = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(h.path().join(".codex")).unwrap();
        std::fs::write(h.path().join(".codex/config.toml"), "model = \"gpt-5-codex\"\n[profiles.x]\nmodel = \"other\"\n").unwrap();
        assert_eq!(configured_model("codex", h.path()).as_deref(), Some("gpt-5-codex"));
        std::fs::create_dir_all(h.path().join(".gemini")).unwrap();
        std::fs::write(h.path().join(".gemini/settings.json"), r#"{"model":{"name":"gemini-2.5-flash"}}"#).unwrap();
        assert_eq!(configured_model("gemini", h.path()).as_deref(), Some("gemini-2.5-flash"));
        assert_eq!(configured_model("claude", h.path()), None);

        let claude = def("claude").unwrap();
        assert_eq!(interactive_args(claude, "agent", Some("opus")).unwrap(), ["--model", "opus"]);
        assert_eq!(interactive_args(claude, "login", None).unwrap(), ["auth", "login"]);
        assert!(interactive_args(claude, "agent", Some("--dangerously-skip-permissions")).is_err());
        assert!(interactive_args(claude, "agent", Some("a b")).is_err());
    }

    #[test]
    fn check_persistence_roundtrip() {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|c| {
            assert!(load_check(c, "claude")?.is_none());
            let chk = ProviderCheck {
                ok: true,
                version: Some("2.1.317".into()),
                model: Some("claude-haiku-5-5".into()),
                message: "ok".into(),
                duration_ms: 2000,
                input_tokens: Some(2),
                output_tokens: Some(65),
                cost_usd: Some(0.0058),
                checked_at: 1,
            };
            save_check(c, "claude", &chk)?;
            save_check(c, "claude", &ProviderCheck { ok: false, ..chk.clone() })?;
            let got = load_check(c, "claude")?.unwrap();
            assert!(!got.ok);
            assert_eq!(got.output_tokens, Some(65));
            Ok(())
        })
        .unwrap();
    }

    /// Live checks against the CLIs installed on this machine. Run with
    /// `cargo test providers::tests::live -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_status_of_installed_providers() {
        for p in PROVIDERS {
            let st = status(p, None);
            eprintln!(
                "{:<18} installed={} version={:?} auth={} method={:?} plan={:?} model={:?}",
                st.name, st.installed, st.version, st.auth.state, st.auth.method, st.auth.plan, st.configured_model
            );
        }
        assert!(!environment::login_env().is_empty(), "login env should be captured");
    }

    #[test]
    #[ignore]
    fn live_verify_claude() {
        let d = tempfile::tempdir().unwrap();
        let chk = verify(def("claude").unwrap(), d.path()).unwrap();
        eprintln!("{chk:?}");
        assert!(chk.ok, "{}", chk.message);
    }

    #[test]
    #[ignore]
    fn live_verify_unauthenticated_reports_failure() {
        for id in ["codex", "gemini"] {
            let d = tempfile::tempdir().unwrap();
            let chk = verify(def(id).unwrap(), d.path()).unwrap();
            eprintln!("{id}: ok={} msg={}", chk.ok, chk.message);
        }
    }
}
