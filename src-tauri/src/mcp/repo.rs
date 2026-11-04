//! MCP server registry: CRUD, keychain-backed secrets, `${KEY}` placeholders,
//! templates, links to profiles/squads/missions and per-run configuration
//! for Claude Code (`--mcp-config` + `--strict-mcp-config` + `--allowedTools`).

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::client::{Endpoint, ProbeResult, ToolInfo};
use super::secrets::{account, SecretStore};
use crate::db::{new_id, now_ms};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub id: String,
    pub name: String,
    pub label: String,
    pub transport: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub url: Option<String>,
    pub headers: HashMap<String, String>,
    pub secret_keys: Vec<String>,
    pub read_only: bool,
    pub enabled: bool,
    pub template_id: Option<String>,
    pub tools: Vec<ToolInfo>,
    pub server_info: Option<Value>,
    pub last_check_at: Option<i64>,
    pub last_ok: Option<bool>,
    pub last_error: Option<String>,
    pub last_log: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServerInput {
    pub label: String,
    pub transport: String,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub url: Option<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub secret_keys: Vec<String>,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

const COLS: &str = "id, name, label, transport, command, args, env, url, headers, secret_keys, read_only, enabled, template_id, tools, server_info,
  last_check_at, last_ok, last_error, last_log, created_at, updated_at";

fn json_col<T: serde::de::DeserializeOwned + Default>(r: &Row, i: usize) -> rusqlite::Result<T> {
    Ok(serde_json::from_str(&r.get::<_, String>(i)?).unwrap_or_default())
}

fn map(r: &Row) -> rusqlite::Result<Server> {
    let info: Option<String> = r.get(14)?;
    Ok(Server {
        id: r.get(0)?,
        name: r.get(1)?,
        label: r.get(2)?,
        transport: r.get(3)?,
        command: r.get(4)?,
        args: json_col(r, 5)?,
        env: json_col(r, 6)?,
        url: r.get(7)?,
        headers: json_col(r, 8)?,
        secret_keys: json_col(r, 9)?,
        read_only: r.get::<_, i64>(10)? != 0,
        enabled: r.get::<_, i64>(11)? != 0,
        template_id: r.get(12)?,
        tools: json_col(r, 13)?,
        server_info: info.and_then(|s| serde_json::from_str(&s).ok()),
        last_check_at: r.get(15)?,
        last_ok: r.get::<_, Option<i64>>(16)?.map(|v| v != 0),
        last_error: r.get(17)?,
        last_log: r.get(18)?,
        created_at: r.get(19)?,
        updated_at: r.get(20)?,
    })
}

// ── Validation ────────────────────────────────────────────────────────────

fn valid_env_key(k: &str) -> bool {
    !k.is_empty() && k.len() <= 64 && k.chars().next().is_some_and(|c| c.is_ascii_uppercase() || c == '_') && k.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn valid_header(k: &str) -> bool {
    !k.is_empty() && k.len() <= 64 && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// `${KEY}` placeholders in a string.
pub fn placeholders(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(a) = rest.find("${") {
        let after = &rest[a + 2..];
        let Some(b) = after.find('}') else { break };
        out.push(after[..b].to_string());
        rest = &after[b + 1..];
    }
    out
}

pub fn validate(input: &ServerInput) -> AppResult<()> {
    let label = input.label.trim();
    if label.is_empty() || label.chars().count() > 80 {
        return Err(AppError::Invalid("o nome do servidor deve ter de 1 a 80 caracteres".into()));
    }
    for k in &input.secret_keys {
        if !valid_env_key(k) {
            return Err(AppError::Invalid(format!("nome de segredo inválido: {k} (use MAIÚSCULAS_COM_UNDERSCORE)")));
        }
    }
    let mut texts: Vec<&str> = Vec::new();
    match input.transport.as_str() {
        "stdio" => {
            let cmd = input.command.as_deref().map(str::trim).unwrap_or_default();
            if cmd.is_empty() || cmd.len() > 500 || cmd.contains('\n') || cmd.contains('\0') {
                return Err(AppError::Invalid("informe o comando do servidor (ex.: npx)".into()));
            }
            if input.args.len() > 50 || input.args.iter().any(|a| a.len() > 2000 || a.contains('\0')) {
                return Err(AppError::Invalid("argumentos inválidos".into()));
            }
            if input.env.len() > 50 {
                return Err(AppError::Invalid("variáveis de ambiente demais".into()));
            }
            for (k, v) in &input.env {
                if !valid_env_key(k) || v.len() > 4000 {
                    return Err(AppError::Invalid(format!("variável de ambiente inválida: {k}")));
                }
                if input.secret_keys.contains(k) {
                    return Err(AppError::Invalid(format!("{k} é um segredo: não repita como variável comum")));
                }
            }
            texts.extend(input.args.iter().map(String::as_str));
        }
        "http" => {
            let url = input.url.as_deref().map(str::trim).unwrap_or_default();
            if !(url.starts_with("https://") || url.starts_with("http://localhost") || url.starts_with("http://127.0.0.1")) || url.len() > 2000 {
                return Err(AppError::Invalid("a URL deve usar https:// (http só para localhost)".into()));
            }
            for (k, v) in &input.headers {
                if !valid_header(k) || v.len() > 4000 || v.contains('\n') {
                    return Err(AppError::Invalid(format!("cabeçalho inválido: {k}")));
                }
            }
            texts.push(url);
            texts.extend(input.headers.values().map(String::as_str));
        }
        other => return Err(AppError::Invalid(format!("transporte inválido: {other}"))),
    }
    for t in texts {
        for p in placeholders(t) {
            if !input.secret_keys.contains(&p) {
                return Err(AppError::Invalid(format!("o placeholder ${{{p}}} não corresponde a nenhum segredo declarado")));
            }
        }
    }
    Ok(())
}

fn slug(label: &str) -> String {
    let s: String = label.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "mcp".into() } else { s.chars().take(40).collect() }
}

// ── CRUD ──────────────────────────────────────────────────────────────────

pub fn list(conn: &Connection) -> AppResult<Vec<Server>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM mcp_servers ORDER BY label COLLATE NOCASE"))?;
    let rows = stmt.query_map([], map)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Server> {
    conn.query_row(&format!("SELECT {COLS} FROM mcp_servers WHERE id = ?1"), [id], map)
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("servidor MCP não encontrado: {id}")))
}

fn unique_name(conn: &Connection, base: &str, except: Option<&str>) -> AppResult<String> {
    let mut name = base.to_string();
    let mut n = 2;
    loop {
        let taken: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM mcp_servers WHERE name = ?1 AND (?2 IS NULL OR id <> ?2))",
            params![name, except],
            |r| r.get(0),
        )?;
        if !taken {
            return Ok(name);
        }
        name = format!("{base}-{n}");
        n += 1;
    }
}

pub fn create(conn: &Connection, input: &ServerInput, template_id: Option<&str>) -> AppResult<Server> {
    validate(input)?;
    let id = new_id();
    let name = unique_name(conn, &slug(&input.label), None)?;
    let now = now_ms();
    conn.execute(
        "INSERT INTO mcp_servers (id, name, label, transport, command, args, env, url, headers, secret_keys, read_only, enabled, template_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)",
        params![
            id,
            name,
            input.label.trim(),
            input.transport,
            input.command.as_deref().map(str::trim),
            serde_json::to_string(&input.args)?,
            serde_json::to_string(&input.env)?,
            input.url.as_deref().map(str::trim),
            serde_json::to_string(&input.headers)?,
            serde_json::to_string(&input.secret_keys)?,
            input.read_only as i64,
            input.enabled as i64,
            template_id,
            now
        ],
    )?;
    get(conn, &id)
}

pub fn update(conn: &Connection, store: &dyn SecretStore, id: &str, input: &ServerInput) -> AppResult<Server> {
    validate(input)?;
    let current = get(conn, id)?;
    // Secrets no longer declared are removed from the keychain.
    for k in current.secret_keys.iter().filter(|k| !input.secret_keys.contains(k)) {
        store.delete(&account(id, k))?;
    }
    let name = unique_name(conn, &slug(&input.label), Some(id))?;
    conn.execute(
        "UPDATE mcp_servers SET name = ?2, label = ?3, transport = ?4, command = ?5, args = ?6, env = ?7, url = ?8, headers = ?9,
         secret_keys = ?10, read_only = ?11, enabled = ?12, updated_at = ?13 WHERE id = ?1",
        params![
            id,
            name,
            input.label.trim(),
            input.transport,
            input.command.as_deref().map(str::trim),
            serde_json::to_string(&input.args)?,
            serde_json::to_string(&input.env)?,
            input.url.as_deref().map(str::trim),
            serde_json::to_string(&input.headers)?,
            serde_json::to_string(&input.secret_keys)?,
            input.read_only as i64,
            input.enabled as i64,
            now_ms()
        ],
    )?;
    get(conn, id)
}

pub fn delete(conn: &Connection, store: &dyn SecretStore, id: &str) -> AppResult<()> {
    let s = get(conn, id)?;
    for k in &s.secret_keys {
        store.delete(&account(id, k))?;
    }
    conn.execute("DELETE FROM mcp_servers WHERE id = ?1", [id])?;
    Ok(())
}

pub fn set_enabled(conn: &Connection, id: &str, enabled: bool) -> AppResult<Server> {
    get(conn, id)?;
    conn.execute("UPDATE mcp_servers SET enabled = ?2, updated_at = ?3 WHERE id = ?1", params![id, enabled as i64, now_ms()])?;
    get(conn, id)
}

// ── Secrets ───────────────────────────────────────────────────────────────

pub fn set_secret(conn: &Connection, store: &dyn SecretStore, id: &str, key: &str, value: &str) -> AppResult<()> {
    let s = get(conn, id)?;
    if !s.secret_keys.iter().any(|k| k == key) {
        return Err(AppError::Invalid(format!("{key} não é um segredo declarado deste servidor")));
    }
    if value.is_empty() || value.len() > 16 * 1024 {
        return Err(AppError::Invalid("valor de segredo inválido".into()));
    }
    store.set(&account(id, key), value)
}

pub fn clear_secret(store: &dyn SecretStore, id: &str, key: &str) -> AppResult<()> {
    store.delete(&account(id, key))
}

/// Which declared secrets currently have a value (never the values themselves).
pub fn secret_status(store: &dyn SecretStore, s: &Server) -> AppResult<HashMap<String, bool>> {
    s.secret_keys.iter().map(|k| Ok((k.clone(), store.get(&account(&s.id, k))?.is_some()))).collect()
}

fn substitute(text: &str, secrets: &HashMap<String, String>) -> String {
    let mut out = text.to_string();
    for (k, v) in secrets {
        out = out.replace(&format!("${{{k}}}"), v);
    }
    out
}

/// Resolves a server into concrete connection parameters (secrets included).
pub fn endpoint(store: &dyn SecretStore, s: &Server) -> AppResult<Endpoint> {
    let mut secrets = HashMap::new();
    for k in &s.secret_keys {
        let v = store.get(&account(&s.id, k))?.ok_or_else(|| AppError::Invalid(format!("defina o segredo {k} do servidor “{}”", s.label)))?;
        secrets.insert(k.clone(), v);
    }
    let mut env = s.env.clone();
    if s.transport == "stdio" {
        env.extend(secrets.clone()); // secrets are also exposed as env vars to stdio servers
    }
    Ok(Endpoint {
        transport: s.transport.clone(),
        command: s.command.clone(),
        args: s.args.iter().map(|a| substitute(a, &secrets)).collect(),
        env,
        url: s.url.as_deref().map(|u| substitute(u, &secrets)),
        headers: s.headers.iter().map(|(k, v)| (k.clone(), substitute(v, &secrets))).collect(),
        secret_values: secrets.into_values().collect(),
    })
}

pub fn record_probe(conn: &Connection, id: &str, r: &ProbeResult) -> AppResult<()> {
    conn.execute(
        "UPDATE mcp_servers SET last_check_at = ?2, last_ok = ?3, last_error = ?4, last_log = ?5,
         tools = CASE WHEN ?3 = 1 THEN ?6 ELSE tools END, server_info = CASE WHEN ?3 = 1 THEN ?7 ELSE server_info END WHERE id = ?1",
        params![
            id,
            now_ms(),
            r.ok as i64,
            r.error,
            r.log,
            serde_json::to_string(&r.tools)?,
            r.server_info.as_ref().map(|v| v.to_string())
        ],
    )?;
    Ok(())
}

// ── Links & per-run configuration ─────────────────────────────────────────

fn check_target(conn: &Connection, kind: &str, target: &str) -> AppResult<()> {
    let table = match kind {
        "profile" => "agent_profiles",
        "squad" => "squads",
        "mission" => "missions",
        _ => return Err(AppError::Invalid(format!("tipo de associação inválido: {kind}"))),
    };
    let ok: bool = conn.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ?1)"), [target], |r| r.get(0))?;
    if ok { Ok(()) } else { Err(AppError::NotFound(format!("{kind} não encontrado"))) }
}

pub fn set_link(conn: &Connection, server_id: &str, kind: &str, target: &str, linked: bool) -> AppResult<()> {
    get(conn, server_id)?;
    check_target(conn, kind, target)?;
    if linked {
        conn.execute(
            "INSERT OR IGNORE INTO mcp_links (server_id, target_kind, target_id, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![server_id, kind, target, now_ms()],
        )?;
    } else {
        conn.execute("DELETE FROM mcp_links WHERE server_id = ?1 AND target_kind = ?2 AND target_id = ?3", params![server_id, kind, target])?;
    }
    Ok(())
}

pub fn linked_to(conn: &Connection, kind: &str, target: &str) -> AppResult<Vec<Server>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM mcp_servers s JOIN mcp_links l ON l.server_id = s.id WHERE l.target_kind = ?1 AND l.target_id = ?2 ORDER BY s.label COLLATE NOCASE",
        COLS.split(',').map(|c| format!("s.{}", c.trim())).collect::<Vec<_>>().join(", ")
    ))?;
    let rows = stmt.query_map(params![kind, target], map)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn links_of(conn: &Connection, server_id: &str) -> AppResult<Vec<(String, String)>> {
    let mut stmt = conn.prepare("SELECT target_kind, target_id FROM mcp_links WHERE server_id = ?1 ORDER BY target_kind")?;
    let rows = stmt.query_map([server_id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Enabled servers for a run: profile ∪ squad ∪ mission.
pub fn compose(conn: &Connection, profile_id: Option<&str>, squad_id: Option<&str>, mission_id: Option<&str>) -> AppResult<Vec<Server>> {
    let mut out: Vec<Server> = Vec::new();
    for (kind, t) in [("profile", profile_id), ("squad", squad_id), ("mission", mission_id)] {
        let Some(t) = t else { continue };
        for s in linked_to(conn, kind, t)? {
            if s.enabled && !out.iter().any(|x| x.id == s.id) {
                out.push(s);
            }
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunMcp {
    /// `{"mcpServers": {...}}` for `--mcp-config` (contains resolved secrets!).
    pub config: Option<Value>,
    /// Claude `--allowedTools` entries (`mcp__<name>`).
    pub allowed_tools: Vec<String>,
    /// Server names attached (recorded on the run).
    pub names: Vec<String>,
    /// Servers skipped (missing secrets, etc.) with the reason.
    pub skipped: Vec<String>,
}

/// Builds the Claude Code MCP configuration for a run. Tools of non-read-only
/// servers are only pre-approved when the agent may edit.
pub fn run_config(store: &dyn SecretStore, servers: &[Server], permission: &str) -> RunMcp {
    let mut out = RunMcp::default();
    let mut map = serde_json::Map::new();
    for s in servers {
        let ep = match endpoint(store, s) {
            Ok(ep) => ep,
            Err(e) => {
                out.skipped.push(format!("{}: {e}", s.label));
                continue;
            }
        };
        let entry = if s.transport == "stdio" {
            json!({ "type": "stdio", "command": ep.command, "args": ep.args, "env": ep.env })
        } else {
            json!({ "type": "http", "url": ep.url, "headers": ep.headers })
        };
        map.insert(s.name.clone(), entry);
        out.names.push(s.name.clone());
        if permission != "read" || s.read_only {
            // Claude names MCP tools `mcp__<server>__<tool>` keeping the server name verbatim (hyphens included).
            out.allowed_tools.push(format!("mcp__{}", s.name));
        }
    }
    if !map.is_empty() {
        out.config = Some(json!({ "mcpServers": map }));
    }
    out
}

/// MCP attachment prepared for one run (Claude Code only).
#[derive(Debug, Clone, PartialEq)]
pub struct McpAttach {
    /// Temporary config file (0600) with resolved secrets; deleted when the run ends.
    pub config_file: std::path::PathBuf,
    pub allowed_tools: Vec<String>,
    pub names: Vec<String>,
    /// `--strict-mcp-config` (not compatible with bypassPermissions).
    pub strict: bool,
}

/// Writes the run's MCP config to `dir/<tag>.json` with owner-only permissions.
pub fn write_run_config(dir: &std::path::Path, tag: &str, config: &Value) -> AppResult<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{tag}.json"));
    let body = serde_json::to_vec(config)?;
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&path)?;
        f.write_all(&body)?;
    }
    #[cfg(not(unix))]
    std::fs::write(&path, &body)?;
    Ok(path)
}

#[allow(clippy::too_many_arguments)]
/// Composes servers for a run and prepares the attachment. Returns `(attach, notes)`
/// where notes explain anything skipped (unsupported provider, missing secrets…).
pub fn prepare_run(
    conn: &Connection,
    store: &dyn SecretStore,
    dir: &std::path::Path,
    provider: &str,
    permission: &str,
    profile_id: Option<&str>,
    squad_id: Option<&str>,
    mission_id: Option<&str>,
) -> AppResult<(Option<McpAttach>, Vec<String>)> {
    prepare_run_with(conn, store, dir, provider, permission, profile_id, squad_id, mission_id, &[])
}

/// Like `prepare_run`, plus explicitly chosen servers for this run (e.g. Harness).
#[allow(clippy::too_many_arguments)]
pub fn prepare_run_with(
    conn: &Connection,
    store: &dyn SecretStore,
    dir: &std::path::Path,
    provider: &str,
    permission: &str,
    profile_id: Option<&str>,
    squad_id: Option<&str>,
    mission_id: Option<&str>,
    extra_ids: &[String],
) -> AppResult<(Option<McpAttach>, Vec<String>)> {
    let mut servers = compose(conn, profile_id, squad_id, mission_id)?;
    for id in extra_ids {
        if servers.iter().any(|s| &s.id == id) {
            continue;
        }
        if let Ok(s) = get(conn, id) {
            if s.enabled {
                servers.push(s);
            }
        }
    }
    if servers.is_empty() {
        return Ok((None, vec![]));
    }
    let mut notes = Vec::new();
    if provider != "claude" {
        notes.push(format!("MCP por agente ainda não é suportado no provider {provider}: {} servidor(es) ignorado(s)", servers.len()));
        return Ok((None, notes));
    }
    let cfg = run_config(store, &servers, permission);
    notes.extend(cfg.skipped.iter().map(|s| format!("MCP ignorado — {s}")));
    let Some(config) = cfg.config else { return Ok((None, notes)) };
    let strict = permission != "full";
    if !strict {
        notes.push("Acesso total: --strict-mcp-config desativado (incompatível com bypassPermissions); os MCPs globais do Claude também podem carregar".into());
    }
    let path = write_run_config(dir, &new_id(), &config)?;
    Ok((Some(McpAttach { config_file: path, allowed_tools: cfg.allowed_tools, names: cfg.names, strict }), notes))
}

// ── Templates ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInput {
    pub key: String,
    pub label: String,
    pub secret: bool,
    pub placeholder: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub transport: &'static str,
    pub command: Option<&'static str>,
    pub args: Vec<&'static str>,
    pub url: Option<&'static str>,
    pub headers: Vec<(&'static str, &'static str)>,
    pub inputs: Vec<TemplateInput>,
    pub read_only: bool,
    pub requirements: &'static str,
    pub docs: &'static str,
}

fn input(key: &str, label: &str, secret: bool, placeholder: &str) -> TemplateInput {
    TemplateInput { key: key.into(), label: label.into(), secret, placeholder: placeholder.into() }
}

pub fn templates() -> Vec<Template> {
    vec![
        Template {
            id: "filesystem",
            label: "Filesystem",
            description: "Leitura e escrita de arquivos em diretórios permitidos.",
            transport: "stdio",
            command: Some("npx"),
            args: vec!["-y", "@modelcontextprotocol/server-filesystem", "{dir}"],
            url: None,
            headers: vec![],
            inputs: vec![input("dir", "Diretório permitido", false, "/caminho/do/projeto")],
            read_only: false,
            requirements: "Node.js (npx). Baixa o pacote na primeira execução.",
            docs: "https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem",
        },
        Template {
            id: "github",
            label: "GitHub",
            description: "Repositórios, issues e pull requests via servidor remoto oficial do GitHub.",
            transport: "http",
            command: None,
            args: vec![],
            url: Some("https://api.githubcopilot.com/mcp/"),
            headers: vec![("Authorization", "Bearer ${GITHUB_TOKEN}")],
            inputs: vec![input("GITHUB_TOKEN", "Personal access token do GitHub", true, "github_pat_…")],
            read_only: false,
            requirements: "Token do GitHub com os escopos desejados.",
            docs: "https://github.com/github/github-mcp-server",
        },
        Template {
            id: "browser",
            label: "Browser (Playwright)",
            description: "Automação de navegador: navegar, clicar, ler páginas e capturar telas.",
            transport: "stdio",
            command: Some("npx"),
            args: vec!["-y", "@playwright/mcp@latest"],
            url: None,
            headers: vec![],
            inputs: vec![],
            read_only: false,
            requirements: "Node.js (npx). Os navegadores do Playwright podem ser baixados no primeiro uso.",
            docs: "https://github.com/microsoft/playwright-mcp",
        },
        Template {
            id: "supabase",
            label: "Supabase",
            description: "Consulta ao banco e gerenciamento do projeto Supabase (modo somente leitura).",
            transport: "stdio",
            command: Some("npx"),
            args: vec!["-y", "@supabase/mcp-server-supabase@latest", "--read-only", "--project-ref={project_ref}"],
            url: None,
            headers: vec![],
            inputs: vec![input("project_ref", "Project ref", false, "abcdefghijklmnop"), input("SUPABASE_ACCESS_TOKEN", "Access token do Supabase", true, "sbp_…")],
            read_only: true,
            requirements: "Node.js (npx) e um access token pessoal do Supabase.",
            docs: "https://github.com/supabase-community/supabase-mcp",
        },
        Template {
            id: "postgres",
            label: "PostgreSQL",
            description: "Consultas somente leitura e inspeção de esquema de um banco PostgreSQL.",
            transport: "stdio",
            command: Some("npx"),
            args: vec!["-y", "@modelcontextprotocol/server-postgres", "${DATABASE_URL}"],
            url: None,
            headers: vec![],
            inputs: vec![input("DATABASE_URL", "Connection string", true, "postgresql://user:senha@host:5432/db")],
            read_only: true,
            requirements: "Node.js (npx). A connection string fica no cofre do sistema.",
            docs: "https://github.com/modelcontextprotocol/servers-archived/tree/main/src/postgres",
        },
        Template {
            id: "docker",
            label: "Docker",
            description: "Gateway MCP do Docker Desktop (catálogo de servidores em containers).",
            transport: "stdio",
            command: Some("docker"),
            args: vec!["mcp", "gateway", "run"],
            url: None,
            headers: vec![],
            inputs: vec![],
            read_only: false,
            requirements: "Docker Desktop com o MCP Toolkit habilitado.",
            docs: "https://docs.docker.com/ai/mcp-catalog-and-toolkit/",
        },
        Template {
            id: "vercel",
            label: "Vercel",
            description: "Projetos, deploys e logs da Vercel (servidor remoto oficial).",
            transport: "http",
            command: None,
            args: vec![],
            url: Some("https://mcp.vercel.com"),
            headers: vec![],
            inputs: vec![],
            read_only: false,
            requirements: "Requer login OAuth da Vercel — o FORGE ainda não implementa OAuth para MCP; o teste mostrará a resposta do servidor.",
            docs: "https://vercel.com/docs/mcp",
        },
        Template {
            id: "cloudflare-docs",
            label: "Cloudflare Docs",
            description: "Busca na documentação da Cloudflare (servidor remoto público, sem login).",
            transport: "http",
            command: None,
            args: vec![],
            url: Some("https://docs.mcp.cloudflare.com/mcp"),
            headers: vec![],
            inputs: vec![],
            read_only: true,
            requirements: "Nenhum.",
            docs: "https://github.com/cloudflare/mcp-server-cloudflare",
        },
    ]
}

/// Creates a server from a template. Non-secret inputs are substituted into
/// args (`{key}`); secret inputs go to the keychain only.
pub fn create_from_template(conn: &Connection, store: &dyn SecretStore, template_id: &str, label: Option<&str>, values: &HashMap<String, String>) -> AppResult<Server> {
    let t = templates().into_iter().find(|t| t.id == template_id).ok_or_else(|| AppError::NotFound(format!("modelo desconhecido: {template_id}")))?;
    for i in &t.inputs {
        if values.get(&i.key).map(|v| v.trim()).unwrap_or_default().is_empty() {
            return Err(AppError::Invalid(format!("preencha “{}”", i.label)));
        }
    }
    let plain: HashMap<&str, &str> = t.inputs.iter().filter(|i| !i.secret).map(|i| (i.key.as_str(), values[&i.key].trim())).collect();
    let args: Vec<String> = t
        .args
        .iter()
        .map(|a| plain.iter().fold(a.to_string(), |acc, (k, v)| acc.replace(&format!("{{{k}}}"), v)))
        .collect();
    let input = ServerInput {
        label: label.map(str::to_string).filter(|l| !l.trim().is_empty()).unwrap_or_else(|| t.label.to_string()),
        transport: t.transport.into(),
        command: t.command.map(str::to_string),
        args,
        env: HashMap::new(),
        url: t.url.map(str::to_string),
        headers: t.headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        secret_keys: t.inputs.iter().filter(|i| i.secret).map(|i| i.key.clone()).collect(),
        read_only: t.read_only,
        enabled: true,
    };
    let s = create(conn, &input, Some(t.id))?;
    for i in t.inputs.iter().filter(|i| i.secret) {
        set_secret(conn, store, &s.id, &i.key, values[&i.key].trim())?;
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::mcp::secrets::MemoryStore;

    fn db() -> Db {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| crate::agents::profiles::ensure_builtins(c, "claude")).unwrap();
        db
    }

    fn stdio(label: &str) -> ServerInput {
        ServerInput {
            label: label.into(),
            transport: "stdio".into(),
            command: Some("npx".into()),
            args: vec!["-y".into(), "pkg".into(), "--token=${API_TOKEN}".into()],
            env: HashMap::from([("LOG_LEVEL".into(), "info".into())]),
            url: None,
            headers: HashMap::new(),
            secret_keys: vec!["API_TOKEN".into()],
            read_only: false,
            enabled: true,
        }
    }

    #[test]
    fn validation_rules() {
        assert!(validate(&stdio("ok")).is_ok());
        let mut bad = stdio("x");
        bad.args.push("${UNDECLARED}".into());
        assert!(validate(&bad).unwrap_err().to_string().contains("UNDECLARED"));
        let mut bad = stdio("x");
        bad.env.insert("bad-key".into(), "v".into());
        assert!(validate(&bad).is_err());
        let mut bad = stdio("x");
        bad.env.insert("API_TOKEN".into(), "plain".into());
        assert!(validate(&bad).is_err(), "secret duplicated as plain env");
        let mut bad = stdio("x");
        bad.command = Some("".into());
        assert!(validate(&bad).is_err());
        let http = |url: &str| ServerInput { transport: "http".into(), command: None, args: vec![], env: HashMap::new(), url: Some(url.into()), secret_keys: vec![], ..stdio("h") };
        assert!(validate(&http("https://x.dev/mcp")).is_ok());
        assert!(validate(&http("http://localhost:3000/mcp")).is_ok());
        assert!(validate(&http("http://evil.com/mcp")).is_err(), "plain http only for localhost");
        assert!(validate(&http("file:///etc/passwd")).is_err());
    }

    #[test]
    fn secrets_live_only_in_the_store() {
        let db = db();
        let store = MemoryStore::default();
        db.with(|c| {
            let s = create(c, &stdio("My Server"), None)?;
            assert_eq!(s.name, "my-server");
            assert_eq!(create(c, &stdio("My Server"), None)?.name, "my-server-2");
            assert!(endpoint(&store, &s).unwrap_err().to_string().contains("API_TOKEN"), "missing secret");
            set_secret(c, &store, &s.id, "API_TOKEN", "tok-123456")?;
            assert!(set_secret(c, &store, &s.id, "OTHER", "x").is_err());
            assert!(secret_status(&store, &s)?["API_TOKEN"]);
            let ep = endpoint(&store, &s)?;
            assert_eq!(ep.args[2], "--token=tok-123456");
            assert_eq!(ep.env["API_TOKEN"], "tok-123456");
            assert_eq!(ep.env["LOG_LEVEL"], "info");
            // The database never contains the value.
            let dump: String = c.query_row("SELECT args || env || secret_keys FROM mcp_servers WHERE id = ?1", [&s.id], |r| r.get(0))?;
            assert!(!dump.contains("tok-123456"));
            // Dropping a declared secret removes it from the store.
            let mut no_secret = stdio("My Server");
            no_secret.secret_keys.clear();
            no_secret.args.truncate(2);
            update(c, &store, &s.id, &no_secret)?;
            assert!(store.get(&account(&s.id, "API_TOKEN"))?.is_none());
            delete(c, &store, &s.id)?;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn templates_create_valid_servers_and_keep_secrets_out_of_db() {
        let db = db();
        let store = MemoryStore::default();
        db.with(|c| {
            for t in templates() {
                let values: HashMap<String, String> = t.inputs.iter().map(|i| (i.key.clone(), format!("value-{}", i.key))).collect();
                let s = create_from_template(c, &store, t.id, None, &values)?;
                let ep = endpoint(&store, &s)?;
                assert!(!ep.args.iter().any(|a| a.contains('{') && a.contains('}') && !a.contains("${")), "{}: unresolved {:?}", t.id, ep.args);
                for i in t.inputs.iter().filter(|i| i.secret) {
                    let row: String = c.query_row("SELECT args || headers || env FROM mcp_servers WHERE id = ?1", [&s.id], |r| r.get(0))?;
                    assert!(!row.contains(&format!("value-{}", i.key)), "{} secret in db", t.id);
                }
            }
            assert_eq!(list(c)?.len(), 8);
            let gh = list(c)?.into_iter().find(|s| s.template_id.as_deref() == Some("github")).unwrap();
            assert_eq!(endpoint(&store, &gh)?.headers["Authorization"], "Bearer value-GITHUB_TOKEN");
            let fs = list(c)?.into_iter().find(|s| s.template_id.as_deref() == Some("filesystem")).unwrap();
            assert_eq!(fs.args[2], "value-dir");
            assert!(create_from_template(c, &store, "filesystem", None, &HashMap::new()).is_err(), "inputs required");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn links_compose_and_claude_run_config() {
        let db = db();
        let store = MemoryStore::default();
        db.with(|c| {
            let docs = create_from_template(c, &store, "cloudflare-docs", None, &HashMap::new())?;
            let fs = create_from_template(c, &store, "filesystem", None, &HashMap::from([("dir".to_string(), "/tmp".to_string())]))?;
            let mut gh_vals = HashMap::new();
            gh_vals.insert("GITHUB_TOKEN".to_string(), "ghp_secret".to_string());
            let gh = create_from_template(c, &store, "github", None, &gh_vals)?;
            clear_secret(&store, &gh.id, "GITHUB_TOKEN")?;
            for s in [&docs, &fs, &gh] {
                set_link(c, &s.id, "profile", "builtin:backend", true)?;
            }
            assert!(set_link(c, &docs.id, "profile", "ghost", true).is_err());
            set_enabled(c, &fs.id, true)?;
            let servers = compose(c, Some("builtin:backend"), None, None)?;
            assert_eq!(servers.len(), 3);

            let read = run_config(&store, &servers, "read");
            assert_eq!(read.names, ["cloudflare-docs", "filesystem"], "server without its secret is skipped");
            assert!(read.skipped[0].contains("GITHUB_TOKEN"));
            assert_eq!(read.allowed_tools, ["mcp__cloudflare-docs"], "only read-only servers pre-approved for read agents");
            let cfg = read.config.unwrap();
            assert_eq!(cfg["mcpServers"]["cloudflare-docs"]["type"], "http");
            assert_eq!(cfg["mcpServers"]["filesystem"]["args"][2], "/tmp");

            let edit = run_config(&store, &servers, "edit");
            assert_eq!(edit.allowed_tools, ["mcp__cloudflare-docs", "mcp__filesystem"]);
            set_enabled(c, &docs.id, false)?;
            assert_eq!(compose(c, Some("builtin:backend"), None, None)?.len(), 2);
            assert_eq!(run_config(&store, &[], "edit"), RunMcp::default());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn extra_servers_attach_for_claude_only() {
        let db = db();
        let store = MemoryStore::default();
        let dir = tempfile::tempdir().unwrap();
        db.with(|c| {
            let open = ServerInput { secret_keys: vec![], args: vec!["-y".into(), "pkg".into()], ..stdio("Harness MCP") };
            let s = create(c, &open, None)?;
            let (none, _) = prepare_run(c, &store, dir.path(), "claude", "edit", Some("builtin:frontend"), None, None)?;
            assert!(none.is_none(), "not linked → not attached");
            let (attach, _) = prepare_run_with(c, &store, dir.path(), "claude", "edit", Some("builtin:frontend"), None, None, &[s.id.clone(), "missing".into()])?;
            let attach = attach.expect("explicit server attached");
            assert_eq!(attach.names, vec![s.name.clone()]);
            let _ = std::fs::remove_file(&attach.config_file);
            let (codex, notes) = prepare_run_with(c, &store, dir.path(), "codex", "edit", None, None, None, std::slice::from_ref(&s.id))?;
            assert!(codex.is_none() && notes.iter().any(|n| n.contains("codex")));
            Ok(())
        })
        .unwrap();
    }
}
