//! Tauri command surface. Commands are thin: validate input, call the
//! repositories / services, log activity. Keep business logic out of here.

pub mod browser;
pub mod memory;
pub mod voice;
pub mod harness;
pub mod media;
pub mod autopilot;
pub mod observability;
pub mod app;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

use crate::db::{activity, missions, presets, sessions, settings, workspaces, Db};
use crate::environment::{self, ToolStatus};
use crate::agents::{profiles, runner::{self, RunManager, RunRequest}, runs};
use crate::git;
use crate::library::{bundle, recipes, squads};
use crate::skills::{format as skill_format, repo as skills};
use crate::mcp::{client as mcp_client, repo as mcp, secrets::Secrets};
use tauri_plugin_dialog::DialogExt;
use crate::missions::{engine::{MissionDetail, MissionEngine}, tasks::{Task, TaskInput}};
use crate::providers;
use crate::error::{AppError, AppResult};
use crate::terminal::{AttachResult, SpawnSpec, TermEvent, TerminalInfo, TerminalManager};
use crate::search::{self, SearchOptions, SearchResult};
use crate::workspace_fs::{self, DirListing, FileContent, FileMeta, GitInfo};

pub struct AppPaths {
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
}

// ── App ──────────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    platform: String,
    arch: String,
    data_dir: String,
    db_path: String,
    schema_version: usize,
}

#[tauri::command]
pub fn app_info(app: tauri::AppHandle, paths: State<'_, AppPaths>) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        platform: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        data_dir: paths.data_dir.display().to_string(),
        db_path: paths.db_path.display().to_string(),
        schema_version: crate::db::migrations::latest_version(),
    }
}

#[tauri::command]
pub fn reveal_data_dir(app: tauri::AppHandle, paths: State<'_, AppPaths>) -> AppResult<()> {
    app.opener()
        .reveal_item_in_dir(&paths.db_path)
        .map_err(|e| AppError::Internal(e.to_string()))
}

// ── Environment ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn detect_environment() -> Vec<ToolStatus> {
    tauri::async_runtime::spawn_blocking(environment::detect_all)
        .await
        .unwrap_or_default()
}

// ── Settings ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn settings_get_all(db: State<'_, Db>) -> AppResult<HashMap<String, Value>> {
    db.with(settings::get_all)
}

#[tauri::command]
pub fn settings_set(db: State<'_, Db>, key: String, value: Value) -> AppResult<()> {
    db.with(|c| settings::set(c, &key, &value))
}

// ── Workspaces ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn workspaces_list(db: State<'_, Db>) -> AppResult<Vec<workspaces::Workspace>> {
    db.with(workspaces::list)
}

#[tauri::command]
pub fn workspaces_add(db: State<'_, Db>, path: String) -> AppResult<workspaces::Workspace> {
    let dir = workspace_fs::canonical_dir(&path)?;
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.display().to_string());
    db.with(|c| {
        let ws = workspaces::add(c, &dir.display().to_string(), &name)?;
        activity::log(c, "workspace.added", &format!("Workspace “{}” adicionado", ws.name), Some(&ws.id), None)?;
        Ok(ws)
    })
}

#[tauri::command]
pub fn workspaces_open(db: State<'_, Db>, id: String) -> AppResult<workspaces::Workspace> {
    db.with(|c| {
        let ws = workspaces::get(c, &id)?;
        if !Path::new(&ws.path).is_dir() {
            return Err(AppError::NotFound(format!("directory no longer exists: {}", ws.path)));
        }
        let ws = workspaces::touch(c, &id)?;
        settings::set(c, "session.lastWorkspaceId", &Value::String(ws.id.clone()))?;
        activity::log(c, "workspace.opened", &format!("Workspace “{}” aberto", ws.name), Some(&ws.id), None)?;
        Ok(ws)
    })
}

#[tauri::command]
pub fn workspaces_set_pinned(db: State<'_, Db>, id: String, pinned: bool) -> AppResult<workspaces::Workspace> {
    db.with(|c| workspaces::set_pinned(c, &id, pinned))
}

#[tauri::command]
pub fn workspaces_rename(db: State<'_, Db>, id: String, name: String) -> AppResult<workspaces::Workspace> {
    db.with(|c| workspaces::rename(c, &id, &name))
}

#[tauri::command]
pub fn workspaces_remove(db: State<'_, Db>, terminals: State<'_, TerminalManager>, browser: State<'_, browser::Browser>, id: String) -> AppResult<()> {
    terminals.kill_workspace(&id);
    browser.close_workspace(&id);
    db.with(|c| {
        let ws = workspaces::get(c, &id)?;
        workspaces::remove(c, &id)?;
        activity::log(c, "workspace.removed", &format!("Workspace “{}” removido da lista (arquivos preservados)", ws.name), None, None)
    })
}

#[tauri::command]
pub fn workspaces_reveal(app: tauri::AppHandle, db: State<'_, Db>, id: String) -> AppResult<()> {
    let ws = db.with(|c| workspaces::get(c, &id))?;
    app.opener()
        .reveal_item_in_dir(&ws.path)
        .map_err(|e| AppError::Internal(e.to_string()))
}

pub(crate) fn workspace_root(db: &Db, id: &str) -> AppResult<PathBuf> {
    Ok(PathBuf::from(db.with(|c| workspaces::get(c, id))?.path))
}

#[tauri::command]
pub async fn fs_list_dir(db: State<'_, Db>, workspace_id: String, rel_path: String) -> AppResult<DirListing> {
    let root = workspace_root(&db, &workspace_id)?;
    workspace_fs::list_dir(&root, &rel_path)
}

#[tauri::command]
pub async fn fs_read_text(db: State<'_, Db>, workspace_id: String, rel_path: String, max_bytes: Option<u64>) -> AppResult<FileContent> {
    let root = workspace_root(&db, &workspace_id)?;
    workspace_fs::read_text(&root, &rel_path, max_bytes.unwrap_or(workspace_fs::MAX_TEXT_BYTES))
}

#[tauri::command]
pub async fn fs_write_text(
    db: State<'_, Db>,
    workspace_id: String,
    rel_path: String,
    content: String,
    expected_mtime_ms: Option<i64>,
) -> AppResult<FileMeta> {
    let root = workspace_root(&db, &workspace_id)?;
    workspace_fs::write_text(&root, &rel_path, &content, expected_mtime_ms)
}

#[tauri::command]
pub async fn fs_stat(db: State<'_, Db>, workspace_id: String, rel_path: String) -> AppResult<FileMeta> {
    let root = workspace_root(&db, &workspace_id)?;
    workspace_fs::stat(&root, &rel_path)
}

#[tauri::command]
pub fn fs_create(db: State<'_, Db>, workspace_id: String, rel_path: String, is_dir: bool) -> AppResult<FileMeta> {
    let root = workspace_root(&db, &workspace_id)?;
    let meta = if is_dir { workspace_fs::create_dir(&root, &rel_path)? } else { workspace_fs::create_file(&root, &rel_path)? };
    let what = if is_dir { "Pasta" } else { "Arquivo" };
    db.with(|c| activity::log(c, "fs.created", &format!("{what} {rel_path} criado"), Some(&workspace_id), None))?;
    Ok(meta)
}

#[tauri::command]
pub fn fs_rename(db: State<'_, Db>, workspace_id: String, from: String, to: String) -> AppResult<FileMeta> {
    let root = workspace_root(&db, &workspace_id)?;
    let meta = workspace_fs::rename(&root, &from, &to)?;
    db.with(|c| activity::log(c, "fs.renamed", &format!("{from} → {to}"), Some(&workspace_id), None))?;
    Ok(meta)
}

#[tauri::command]
pub async fn fs_trash(db: State<'_, Db>, workspace_id: String, rel_path: String) -> AppResult<()> {
    let root = workspace_root(&db, &workspace_id)?;
    workspace_fs::trash(&root, &rel_path)?;
    db.with(|c| activity::log(c, "fs.trashed", &format!("{rel_path} movido para a lixeira"), Some(&workspace_id), None))
}

#[tauri::command]
pub async fn fs_list_files(db: State<'_, Db>, workspace_id: String) -> AppResult<search::FileList> {
    let root = workspace_root(&db, &workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || search::list_files(&root))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn fs_search(db: State<'_, Db>, workspace_id: String, options: SearchOptions) -> AppResult<SearchResult> {
    let root = workspace_root(&db, &workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || search::search(&root, &options))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub fn workspace_git_info(db: State<'_, Db>, workspace_id: String) -> AppResult<GitInfo> {
    let root = workspace_root(&db, &workspace_id)?;
    Ok(workspace_fs::git_info(&root))
}

// ── Sessions (tabs) & layouts ───────────────────────────────────────────

#[tauri::command]
pub fn sessions_list(db: State<'_, Db>, workspace_id: String) -> AppResult<sessions::SessionsState> {
    db.with(|c| {
        workspaces::get(c, &workspace_id)?;
        sessions::state(c, &workspace_id)
    })
}

#[tauri::command]
pub fn sessions_create(db: State<'_, Db>, workspace_id: String, name: String, layout_json: String) -> AppResult<sessions::Session> {
    db.with(|c| {
        workspaces::get(c, &workspace_id)?;
        sessions::create(c, &workspace_id, &name, &layout_json)
    })
}

#[tauri::command]
pub fn sessions_rename(db: State<'_, Db>, id: String, name: String) -> AppResult<sessions::Session> {
    db.with(|c| sessions::rename(c, &id, &name))
}

#[tauri::command]
pub fn sessions_save_layout(db: State<'_, Db>, id: String, layout_json: String) -> AppResult<()> {
    db.with(|c| sessions::save_layout(c, &id, &layout_json))
}

#[tauri::command]
pub fn sessions_set_active(db: State<'_, Db>, workspace_id: String, id: String) -> AppResult<()> {
    db.with(|c| sessions::set_active(c, &workspace_id, &id))
}

#[tauri::command]
pub fn sessions_delete(db: State<'_, Db>, id: String) -> AppResult<sessions::SessionsState> {
    db.with(|c| sessions::delete(c, &id))
}

#[tauri::command]
pub fn sessions_reorder(db: State<'_, Db>, workspace_id: String, ids: Vec<String>) -> AppResult<sessions::SessionsState> {
    db.with(|c| sessions::reorder(c, &workspace_id, &ids))
}

#[tauri::command]
pub fn presets_list(db: State<'_, Db>) -> AppResult<Vec<presets::Preset>> {
    db.with(presets::list)
}

#[tauri::command]
pub fn presets_create(db: State<'_, Db>, name: String, layout_json: String) -> AppResult<presets::Preset> {
    db.with(|c| presets::create(c, &name, &layout_json))
}

#[tauri::command]
pub fn presets_rename(db: State<'_, Db>, id: String, name: String) -> AppResult<presets::Preset> {
    db.with(|c| presets::rename(c, &id, &name))
}

#[tauri::command]
pub fn presets_delete(db: State<'_, Db>, id: String) -> AppResult<()> {
    db.with(|c| presets::delete(c, &id))
}

// ── Terminals ────────────────────────────────────────────────────────────

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn terminal_spawn(
    db: State<'_, Db>,
    terminals: State<'_, TerminalManager>,
    workspace_id: String,
    rel_cwd: Option<String>,
    cols: u16,
    rows: u16,
    provider_id: Option<String>,
    mode: Option<String>,
    model: Option<String>,
) -> AppResult<TerminalInfo> {
    let root = workspace_root(&db, &workspace_id)?;
    let cwd = match rel_cwd.as_deref().filter(|r| !r.is_empty()) {
        Some(rel) => workspace_fs::resolve(&root, rel)?,
        None => std::fs::canonicalize(&root)?,
    };
    // Agent CLIs run directly (no shell) with the user's login environment.
    let (program, extra_env, label) = match provider_id.as_deref() {
        Some(pid) => {
            let def = providers::def(pid)?;
            let bin = environment::which(def.bin).ok_or_else(|| AppError::NotFound(format!("{} não está instalado", def.name)))?;
            let mode = mode.as_deref().unwrap_or("agent");
            if !matches!(mode, "agent" | "login") {
                return Err(AppError::Invalid(format!("modo inválido: {mode}")));
            }
            let args = providers::interactive_args(def, mode, model.as_deref())?;
            let label = if mode == "login" { format!("Login {}", def.name) } else { def.name.to_string() };
            (Some((bin.display().to_string(), args)), Some(environment::login_env().clone()), label)
        }
        None => (None, None, String::new()),
    };
    let info = terminals.spawn(SpawnSpec {
        workspace_id: workspace_id.clone(),
        cwd,
        cols,
        rows,
        program,
        path_env: Some(environment::search_path().clone()),
        extra_env,
        provider_id: provider_id.clone(),
    })?;
    let what = if label.is_empty() {
        let shell = Path::new(&info.shell).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        format!("Terminal {shell} iniciado")
    } else {
        format!("{label} iniciado")
    };
    db.with(|c| activity::log(c, "terminal.spawned", &what, Some(&workspace_id), None))?;
    Ok(info)
}

// ── Providers ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn providers_list(db: State<'_, Db>) -> AppResult<Vec<providers::ProviderStatus>> {
    let checks: Vec<Option<providers::ProviderCheck>> =
        providers::PROVIDERS.iter().map(|p| db.with(|c| providers::load_check(c, p.id))).collect::<AppResult<_>>()?;
    blocking(move || {
        Ok(std::thread::scope(|s| {
            let handles: Vec<_> = providers::PROVIDERS
                .iter()
                .zip(checks)
                .map(|(p, chk)| s.spawn(move || providers::status(p, chk)))
                .collect();
            handles.into_iter().filter_map(|h| h.join().ok()).collect()
        }))
    })
    .await
}

#[tauri::command]
pub async fn provider_status(db: State<'_, Db>, id: String) -> AppResult<providers::ProviderStatus> {
    let def = providers::def(&id)?;
    let chk = db.with(|c| providers::load_check(c, def.id))?;
    blocking(move || Ok(providers::status(def, chk))).await
}

/// Runs a real headless prompt against the provider (uses a few tokens of the
/// user's plan) and records the outcome.
#[tauri::command]
pub async fn provider_verify(db: State<'_, Db>, paths: State<'_, AppPaths>, id: String) -> AppResult<providers::ProviderCheck> {
    let def = providers::def(&id)?;
    let scratch = paths.data_dir.join("provider-checks").join(def.id);
    let check = blocking(move || providers::verify(def, &scratch)).await?;
    db.with(|c| {
        providers::save_check(c, def.id, &check)?;
        let msg = if check.ok { format!("{} verificado", def.name) } else { format!("Verificação de {} falhou", def.name) };
        activity::log(c, "provider.verify", &msg, None, None)
    })?;
    Ok(check)
}

#[tauri::command]
pub fn terminal_attach(terminals: State<'_, TerminalManager>, id: String, on_event: Channel<TermEvent>) -> AppResult<AttachResult> {
    terminals.attach(&id, Arc::new(move |ev| {
        let _ = on_event.send(ev);
    }))
}

#[tauri::command]
pub fn terminal_detach(terminals: State<'_, TerminalManager>, id: String, token: u64) -> AppResult<()> {
    terminals.detach(&id, token)
}

#[tauri::command]
pub fn terminal_write(terminals: State<'_, TerminalManager>, id: String, data: String) -> AppResult<()> {
    terminals.write(&id, &data)
}

#[tauri::command]
pub fn terminal_resize(terminals: State<'_, TerminalManager>, id: String, cols: u16, rows: u16) -> AppResult<()> {
    terminals.resize(&id, cols, rows)
}

#[tauri::command]
pub fn terminal_kill(terminals: State<'_, TerminalManager>, id: String) -> AppResult<()> {
    terminals.kill(&id)
}

#[tauri::command]
pub fn terminal_list(terminals: State<'_, TerminalManager>, workspace_id: Option<String>) -> Vec<TerminalInfo> {
    terminals.list(workspace_id.as_deref())
}

// ── Git ──────────────────────────────────────────────────────────────────
// Git commands are async: they run the git binary off the main thread.

pub(crate) fn log_ws(db: &Db, ws: &str, kind: &str, msg: &str) {
    let _ = db.with(|c| activity::log(c, kind, msg, Some(ws), None));
}

pub(crate) async fn blocking<T: Send + 'static>(f: impl FnOnce() -> AppResult<T> + Send + 'static) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn git_status(db: State<'_, Db>, workspace_id: String) -> AppResult<git::GitStatus> {
    let root = workspace_root(&db, &workspace_id)?;
    blocking(move || git::status(&root)).await
}

#[tauri::command]
pub async fn git_init(db: State<'_, Db>, workspace_id: String) -> AppResult<()> {
    let root = workspace_root(&db, &workspace_id)?;
    blocking(move || git::init(&root)).await?;
    log_ws(&db, &workspace_id, "git.init", "Repositório Git inicializado");
    Ok(())
}

#[tauri::command]
pub async fn git_file_contents(db: State<'_, Db>, workspace_id: String, rev: String, path: String) -> AppResult<git::GitBlob> {
    let root = workspace_root(&db, &workspace_id)?;
    blocking(move || git::file_contents(&root, &rev, &path)).await
}

#[tauri::command]
pub async fn git_stage(db: State<'_, Db>, workspace_id: String, paths: Vec<String>) -> AppResult<()> {
    let root = workspace_root(&db, &workspace_id)?;
    blocking(move || git::stage(&root, &paths)).await
}

#[tauri::command]
pub async fn git_unstage(db: State<'_, Db>, workspace_id: String, paths: Vec<String>) -> AppResult<()> {
    let root = workspace_root(&db, &workspace_id)?;
    blocking(move || git::unstage(&root, &paths)).await
}

#[tauri::command]
pub async fn git_discard(db: State<'_, Db>, workspace_id: String, paths: Vec<String>) -> AppResult<()> {
    let root = workspace_root(&db, &workspace_id)?;
    let n = paths.len();
    blocking(move || git::discard(&root, &paths)).await?;
    log_ws(&db, &workspace_id, "git.discard", &format!("Alterações descartadas em {n} arquivo(s)"));
    Ok(())
}

#[tauri::command]
pub async fn git_stage_content(db: State<'_, Db>, workspace_id: String, path: String, content: String) -> AppResult<()> {
    let root = workspace_root(&db, &workspace_id)?;
    blocking(move || git::stage_content(&root, &path, &content)).await
}

#[tauri::command]
pub async fn git_commit(db: State<'_, Db>, workspace_id: String, message: String, amend: bool) -> AppResult<String> {
    let root = workspace_root(&db, &workspace_id)?;
    let subject = message.lines().next().unwrap_or_default().chars().take(80).collect::<String>();
    let sha = blocking(move || git::commit(&root, &message, amend)).await?;
    log_ws(&db, &workspace_id, "git.commit", &format!("Commit {} — {subject}", &sha[..7.min(sha.len())]));
    Ok(sha)
}

#[tauri::command]
pub async fn git_log(db: State<'_, Db>, workspace_id: String, limit: Option<u32>, rev: Option<String>) -> AppResult<Vec<git::Commit>> {
    let root = workspace_root(&db, &workspace_id)?;
    blocking(move || git::log(&root, limit.unwrap_or(200), rev.as_deref())).await
}

#[tauri::command]
pub async fn git_commit_files(db: State<'_, Db>, workspace_id: String, sha: String) -> AppResult<Vec<git::CommitFile>> {
    let root = workspace_root(&db, &workspace_id)?;
    blocking(move || git::commit_files(&root, &sha)).await
}

#[tauri::command]
pub async fn git_branches(db: State<'_, Db>, workspace_id: String) -> AppResult<Vec<git::Branch>> {
    let root = workspace_root(&db, &workspace_id)?;
    blocking(move || git::branches(&root)).await
}

#[tauri::command]
pub async fn git_switch(db: State<'_, Db>, workspace_id: String, branch: String, create: bool, start: Option<String>) -> AppResult<()> {
    let root = workspace_root(&db, &workspace_id)?;
    let b = branch.clone();
    blocking(move || git::switch(&root, &b, create, start.as_deref())).await?;
    let verb = if create { "criada" } else { "ativada" };
    log_ws(&db, &workspace_id, "git.switch", &format!("Branch {branch} {verb}"));
    Ok(())
}

#[tauri::command]
pub async fn git_delete_branch(db: State<'_, Db>, workspace_id: String, name: String, force: bool) -> AppResult<()> {
    let root = workspace_root(&db, &workspace_id)?;
    let n = name.clone();
    blocking(move || git::delete_branch(&root, &n, force)).await?;
    log_ws(&db, &workspace_id, "git.branch.delete", &format!("Branch {name} excluída"));
    Ok(())
}

#[tauri::command]
pub async fn git_merge(db: State<'_, Db>, workspace_id: String, branch: String) -> AppResult<String> {
    let root = workspace_root(&db, &workspace_id)?;
    let b = branch.clone();
    let res = blocking(move || git::merge(&root, &b)).await;
    let msg = match &res {
        Ok(_) => format!("Merge de {branch} concluído"),
        Err(_) => format!("Merge de {branch} com conflitos ou falha"),
    };
    log_ws(&db, &workspace_id, "git.merge", &msg);
    res
}

#[tauri::command]
pub async fn git_merge_abort(db: State<'_, Db>, workspace_id: String) -> AppResult<()> {
    let root = workspace_root(&db, &workspace_id)?;
    blocking(move || git::merge_abort(&root)).await?;
    log_ws(&db, &workspace_id, "git.merge.abort", "Merge abortado");
    Ok(())
}

#[tauri::command]
pub async fn git_fetch(db: State<'_, Db>, workspace_id: String) -> AppResult<String> {
    let root = workspace_root(&db, &workspace_id)?;
    blocking(move || git::fetch(&root)).await
}

#[tauri::command]
pub async fn git_pull(db: State<'_, Db>, workspace_id: String) -> AppResult<String> {
    let root = workspace_root(&db, &workspace_id)?;
    let out = blocking(move || git::pull(&root)).await?;
    log_ws(&db, &workspace_id, "git.pull", "Pull (fast-forward) concluído");
    Ok(out)
}

#[tauri::command]
pub async fn git_push(db: State<'_, Db>, workspace_id: String) -> AppResult<String> {
    let root = workspace_root(&db, &workspace_id)?;
    let out = blocking(move || git::push(&root)).await?;
    log_ws(&db, &workspace_id, "git.push", "Push concluído");
    Ok(out)
}

#[tauri::command]
pub async fn git_stash_list(db: State<'_, Db>, workspace_id: String) -> AppResult<Vec<git::StashEntry>> {
    let root = workspace_root(&db, &workspace_id)?;
    blocking(move || git::stash_list(&root)).await
}

#[tauri::command]
pub async fn git_stash_push(db: State<'_, Db>, workspace_id: String, message: Option<String>) -> AppResult<()> {
    let root = workspace_root(&db, &workspace_id)?;
    blocking(move || git::stash_push(&root, message.as_deref())).await?;
    log_ws(&db, &workspace_id, "git.stash", "Alterações guardadas no stash");
    Ok(())
}

#[tauri::command]
pub async fn git_stash_action(db: State<'_, Db>, workspace_id: String, action: String, index: u32) -> AppResult<()> {
    let root = workspace_root(&db, &workspace_id)?;
    blocking(move || git::stash_action(&root, &action, index)).await
}

#[tauri::command]
pub async fn git_worktrees(db: State<'_, Db>, workspace_id: String) -> AppResult<Vec<git::Worktree>> {
    let root = workspace_root(&db, &workspace_id)?;
    blocking(move || git::worktrees(&root)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeCreated {
    worktree: git::Worktree,
    workspace: Option<workspaces::Workspace>,
}

/// Creates a worktree in FORGE's managed folder and (optionally) registers it as a workspace.
#[tauri::command]
pub async fn git_worktree_add(
    db: State<'_, Db>,
    paths: State<'_, AppPaths>,
    workspace_id: String,
    branch: String,
    new_branch: bool,
    base: Option<String>,
    register: bool,
) -> AppResult<WorktreeCreated> {
    let root = workspace_root(&db, &workspace_id)?;
    let managed = paths.data_dir.join("worktrees");
    let b = branch.clone();
    let wt = blocking(move || git::worktree_add(&root, &managed, &b, new_branch, base.as_deref())).await?;
    let workspace = if register {
        let parent = db.with(|c| workspaces::get(c, &workspace_id))?;
        let dir = workspace_fs::canonical_dir(&wt.path)?;
        let ws = db.with(|c| {
            let ws = workspaces::add(c, &dir.display().to_string(), &format!("{} ⎇ {branch}", parent.name))?;
            workspaces::rename(c, &ws.id, &format!("{} ⎇ {branch}", parent.name))
        })?;
        Some(ws)
    } else {
        None
    };
    log_ws(&db, &workspace_id, "git.worktree.add", &format!("Worktree criada para {branch}"));
    Ok(WorktreeCreated { worktree: wt, workspace })
}

/// Removes a worktree; also unregisters its workspace (if any) and stops its terminals.
#[tauri::command]
pub async fn git_worktree_remove(
    db: State<'_, Db>,
    terminals: State<'_, TerminalManager>,
    workspace_id: String,
    path: String,
    force: bool,
) -> AppResult<()> {
    let root = workspace_root(&db, &workspace_id)?;
    let canonical = std::fs::canonicalize(&path).ok().map(|p| p.display().to_string());
    let registered = db.with(workspaces::list)?.into_iter().find(|w| Some(&w.path) == canonical.as_ref() || w.path == path);
    if let Some(ws) = &registered {
        terminals.kill_workspace(&ws.id);
    }
    let p = path.clone();
    blocking(move || git::worktree_remove(&root, &p, force)).await?;
    if let Some(ws) = registered {
        db.with(|c| workspaces::remove(c, &ws.id))?;
    }
    log_ws(&db, &workspace_id, "git.worktree.remove", "Worktree removida");
    Ok(())
}

// ── Agents ───────────────────────────────────────────────────────────────

#[tauri::command]
pub fn agent_profiles_list(db: State<'_, Db>) -> AppResult<Vec<profiles::Profile>> {
    db.with(profiles::list)
}

#[tauri::command]
pub fn agent_profile_create(db: State<'_, Db>, input: profiles::ProfileInput) -> AppResult<profiles::Profile> {
    db.with(|c| profiles::create(c, &input))
}

#[tauri::command]
pub fn agent_profile_update(db: State<'_, Db>, id: String, input: profiles::ProfileInput) -> AppResult<profiles::Profile> {
    db.with(|c| profiles::update(c, &id, &input))
}

#[tauri::command]
pub fn agent_profile_delete(db: State<'_, Db>, id: String) -> AppResult<()> {
    db.with(|c| profiles::delete(c, &id))
}

#[tauri::command]
pub fn agent_profile_restore(db: State<'_, Db>, id: String) -> AppResult<profiles::Profile> {
    db.with(|c| profiles::restore_builtin(c, &id))
}

/// Starts a headless agent run. When `parent_run_id` is set the run continues
/// that conversation (same provider session, same directory/worktree).
#[tauri::command]
pub async fn agent_run_start(
    db: State<'_, Db>,
    paths: State<'_, AppPaths>,
    manager: State<'_, RunManager>,
    secrets: State<'_, Secrets>,
    mut request: RunRequest,
) -> AppResult<runs::Run> {
    let root = workspace_root(&db, &request.workspace_id)?;
    if let Some(parent_id) = request.parent_run_id.clone() {
        let parent = db.with(|c| runs::get(c, &parent_id))?;
        if parent.provider_id != request.provider_id {
            return Err(AppError::Invalid("para continuar a conversa, use o mesmo provider".into()));
        }
        request.resume_session = parent.session_id.clone();
        if request.resume_session.is_none() {
            return Err(AppError::Invalid("a execução anterior não tem sessão para continuar".into()));
        }
        request.use_worktree = false;
        if parent.worktree_branch.is_some() {
            request.cwd_override = Some(PathBuf::from(&parent.cwd));
        }
    }
    if let Some(pid) = request.profile_id.clone() {
        let composed = db.with(|c| crate::skills::repo::compose_with(c, Some(&pid), None, None, &request.extra_skill_ids))?;
        request.instructions = crate::skills::repo::with_skills(request.instructions.take(), &composed);
        request.skill_labels = composed.labels;
        let dir = paths.data_dir.join("mcp-run");
        let (attach, notes) = db.with(|c| {
            mcp::prepare_run_with(c, secrets.as_ref(), &dir, &request.provider_id, &request.permission, Some(&pid), None, None, &request.extra_mcp_ids)
        })?;
        request.mcp = attach;
        for n in notes {
            log_ws(&db, &request.workspace_id, "agent.mcp", &n);
        }
    }
    let db_path = paths.db_path.clone();
    let worktrees = paths.data_dir.join("worktrees");
    let provider = request.provider_id.clone();
    let ws = request.workspace_id.clone();
    let run = manager.start(&db_path, &root, &worktrees, request)?;
    let name = providers::def(&provider).map(|d| d.name).unwrap_or("agente");
    log_ws(&db, &ws, "agent.run", &format!("Agente {name} iniciado ({})", run.permission));
    Ok(run)
}

#[tauri::command]
pub fn agent_run_attach(
    paths: State<'_, AppPaths>,
    manager: State<'_, RunManager>,
    id: String,
    on_event: Channel<serde_json::Value>,
) -> AppResult<runner::AttachResult> {
    manager.attach(&paths.db_path, &id, Arc::new(move |v| {
        let _ = on_event.send(v);
    }))
}

#[tauri::command]
pub fn agent_run_detach(manager: State<'_, RunManager>, id: String, token: u64) {
    manager.detach(&id, token)
}

#[tauri::command]
pub fn agent_run_cancel(manager: State<'_, RunManager>, id: String) -> AppResult<()> {
    manager.cancel(&id)
}

#[tauri::command]
pub fn agent_runs_list(db: State<'_, Db>, workspace_id: Option<String>, status: Option<String>, limit: Option<u32>) -> AppResult<Vec<runs::Run>> {
    db.with(|c| runs::list(c, workspace_id.as_deref(), status.as_deref(), limit.unwrap_or(50)))
}

#[tauri::command]
pub fn agent_run_delete(db: State<'_, Db>, id: String) -> AppResult<()> {
    db.with(|c| runs::delete(c, &id))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunChanges {
    cwd: String,
    prefix: String,
    worktree_branch: Option<String>,
    is_repo: bool,
    files: Vec<git::FileStatus>,
    commits: Vec<git::Commit>,
}

/// Files changed in the run's directory (git status) and commits made since the run started.
#[tauri::command]
pub async fn agent_run_changes(db: State<'_, Db>, id: String) -> AppResult<RunChanges> {
    let run = db.with(|c| runs::get(c, &id))?;
    blocking(move || {
        let cwd = PathBuf::from(&run.cwd);
        if !cwd.is_dir() {
            return Ok(RunChanges { cwd: run.cwd, prefix: String::new(), worktree_branch: run.worktree_branch, is_repo: false, files: vec![], commits: vec![] });
        }
        let st = git::status(&cwd)?;
        let commits = match &run.base_commit {
            Some(b) if st.is_repo => git::commits_since(&cwd, b).unwrap_or_default(),
            _ => vec![],
        };
        Ok(RunChanges { cwd: run.cwd, prefix: st.prefix, worktree_branch: run.worktree_branch, is_repo: st.is_repo, files: st.files, commits })
    })
    .await
}

// ── Mission Engine ───────────────────────────────────────────────────────

#[tauri::command]
pub fn mission_detail(engine: State<'_, MissionEngine>, id: String) -> AppResult<MissionDetail> {
    engine.detail(&id)
}

#[tauri::command]
pub fn mission_update(db: State<'_, Db>, id: String, title: String, briefing: String, workspace_id: Option<String>) -> AppResult<missions::Mission> {
    db.with(|c| missions::update_brief(c, &id, &title, &briefing, workspace_id.as_deref()))
}

#[tauri::command]
pub fn mission_maestro(db: State<'_, Db>, engine: State<'_, MissionEngine>, id: String, settings: missions::MaestroSettings) -> AppResult<missions::Mission> {
    let m = db.with(|c| missions::set_maestro(c, &id, &settings))?;
    // A paused mission is not resumed implicitly; a running one may launch more tasks now.
    let _ = engine.advance(&id);
    Ok(m)
}

#[tauri::command]
pub async fn mission_plan(engine: State<'_, MissionEngine>, id: String) -> AppResult<runs::Run> {
    let e = engine.inner().clone();
    blocking(move || e.plan(&id)).await
}

#[tauri::command]
pub fn mission_save_tasks(engine: State<'_, MissionEngine>, id: String, tasks: Vec<TaskInput>) -> AppResult<Vec<Task>> {
    engine.save_tasks(&id, &tasks)
}

#[tauri::command]
pub async fn mission_start(engine: State<'_, MissionEngine>, id: String) -> AppResult<()> {
    let e = engine.inner().clone();
    blocking(move || e.start(&id)).await
}

#[tauri::command]
pub fn mission_pause(engine: State<'_, MissionEngine>, id: String) -> AppResult<()> {
    engine.pause(&id)
}

#[tauri::command]
pub async fn mission_resume(engine: State<'_, MissionEngine>, id: String) -> AppResult<()> {
    let e = engine.inner().clone();
    blocking(move || e.resume(&id)).await
}

#[tauri::command]
pub fn mission_cancel_run(engine: State<'_, MissionEngine>, id: String) -> AppResult<()> {
    engine.cancel(&id)
}

#[tauri::command]
pub async fn mission_integrate(engine: State<'_, MissionEngine>, id: String) -> AppResult<usize> {
    let e = engine.inner().clone();
    blocking(move || e.integrate(&id)).await
}

#[tauri::command]
pub async fn mission_cleanup(engine: State<'_, MissionEngine>, id: String) -> AppResult<usize> {
    let e = engine.inner().clone();
    blocking(move || e.cleanup(&id)).await
}

/// `action`: approve | reject | retry | skip | cancel
#[tauri::command]
pub async fn mission_task_action(
    engine: State<'_, MissionEngine>,
    mission_id: String,
    task_id: String,
    action: String,
    feedback: Option<String>,
) -> AppResult<()> {
    let e = engine.inner().clone();
    blocking(move || match action.as_str() {
        "approve" => e.approve_task(&mission_id, &task_id),
        "reject" => e.reject_task(&mission_id, &task_id, feedback.as_deref().unwrap_or_default()),
        "retry" => e.retry_task(&mission_id, &task_id, feedback.as_deref()),
        "skip" => e.skip_task(&mission_id, &task_id),
        "cancel" => e.cancel_task(&mission_id, &task_id),
        other => Err(AppError::Invalid(format!("ação inválida: {other}"))),
    })
    .await
}

// ── Squads & recipes ─────────────────────────────────────────────────────

#[tauri::command]
pub fn squads_list(db: State<'_, Db>) -> AppResult<Vec<squads::Squad>> {
    db.with(squads::list)
}

#[tauri::command]
pub fn squad_create(db: State<'_, Db>, input: squads::SquadInput) -> AppResult<squads::Squad> {
    db.with(|c| squads::create(c, &input, "local"))
}

#[tauri::command]
pub fn squad_update(db: State<'_, Db>, id: String, input: squads::SquadInput) -> AppResult<squads::Squad> {
    db.with(|c| squads::update(c, &id, &input))
}

#[tauri::command]
pub fn squad_delete(db: State<'_, Db>, id: String) -> AppResult<()> {
    db.with(|c| squads::delete(c, &id))
}

#[tauri::command]
pub fn squad_restore(db: State<'_, Db>, id: String) -> AppResult<squads::Squad> {
    db.with(|c| squads::restore_builtin(c, &id))
}

#[tauri::command]
pub fn recipes_list(db: State<'_, Db>) -> AppResult<Vec<recipes::Recipe>> {
    db.with(recipes::list)
}

#[tauri::command]
pub fn recipe_create(db: State<'_, Db>, input: recipes::RecipeInput) -> AppResult<recipes::Recipe> {
    db.with(|c| recipes::create(c, &input, "local"))
}

#[tauri::command]
pub fn recipe_update(db: State<'_, Db>, id: String, input: recipes::RecipeInput) -> AppResult<recipes::Recipe> {
    db.with(|c| recipes::update(c, &id, &input))
}

#[tauri::command]
pub fn recipe_delete(db: State<'_, Db>, id: String) -> AppResult<()> {
    db.with(|c| recipes::delete(c, &id))
}

#[tauri::command]
pub fn recipe_restore(db: State<'_, Db>, id: String) -> AppResult<recipes::Recipe> {
    db.with(|c| recipes::restore_builtin(c, &id))
}

#[tauri::command]
pub fn mission_from_recipe(
    db: State<'_, Db>,
    recipe_id: String,
    workspace_id: Option<String>,
    title: String,
    values: HashMap<String, String>,
) -> AppResult<missions::Mission> {
    db.with(|c| {
        let m = recipes::instantiate(c, &recipe_id, workspace_id.as_deref(), &title, &values)?;
        activity::log(c, "mission.created", &format!("Missão “{}” criada a partir de receita", m.title), m.workspace_id.as_deref(), Some(&m.id))?;
        Ok(m)
    })
}

#[tauri::command]
pub fn recipe_from_mission(db: State<'_, Db>, mission_id: String, name: String) -> AppResult<recipes::Recipe> {
    db.with(|c| recipes::from_mission(c, &mission_id, &name))
}

#[tauri::command]
pub fn mission_set_squad(db: State<'_, Db>, id: String, squad_id: Option<String>) -> AppResult<missions::Mission> {
    db.with(|c| {
        if let Some(s) = &squad_id {
            squads::get(c, s)?;
        }
        missions::set_squad(c, &id, squad_id.as_deref())?;
        missions::get(c, &id)
    })
}

/// Exports a squad or recipe to a `.forge.json` file chosen in a native save dialog.
/// Returns the saved path, or None if the user cancelled.
#[tauri::command]
pub async fn bundle_export(app: tauri::AppHandle, db: State<'_, Db>, kind: String, id: String) -> AppResult<Option<String>> {
    let b = db.with(|c| match kind.as_str() {
        "squad" => bundle::export_squad(c, &id),
        "recipe" => bundle::export_recipe(c, &id),
        _ => Err(AppError::Invalid("tipo inválido".into())),
    })?;
    let file_name: String = b.name.chars().map(|ch| if ch.is_alphanumeric() || ch == '-' { ch } else { '-' }).collect::<String>().to_lowercase();
    let json = serde_json::to_string_pretty(&b)?;
    let picked = blocking(move || {
        Ok(app
            .dialog()
            .file()
            .set_title("Exportar para o FORGE")
            .add_filter("FORGE bundle", &["json"])
            .set_file_name(format!("{file_name}.forge.json"))
            .blocking_save_file())
    })
    .await?;
    let Some(fp) = picked else { return Ok(None) };
    let path = fp.into_path().map_err(|e| AppError::Internal(e.to_string()))?;
    std::fs::write(&path, json)?;
    Ok(Some(path.display().to_string()))
}

/// Opens a `.forge.json` chosen in a native dialog and returns a validated preview (nothing is imported yet).
#[tauri::command]
pub async fn bundle_open(app: tauri::AppHandle, db: State<'_, Db>) -> AppResult<Option<bundle::Preview>> {
    let picked = blocking(move || Ok(app.dialog().file().set_title("Importar no FORGE").add_filter("FORGE bundle", &["json"]).blocking_pick_file())).await?;
    let Some(fp) = picked else { return Ok(None) };
    let path = fp.into_path().map_err(|e| AppError::Internal(e.to_string()))?;
    let meta = std::fs::metadata(&path)?;
    if meta.len() as usize > bundle::MAX_BYTES {
        return Err(AppError::Invalid("arquivo grande demais para um bundle (máx. 1 MB)".into()));
    }
    let text = std::fs::read_to_string(&path)?;
    let parsed = bundle::parse(&text)?;
    db.with(|c| bundle::preview(c, parsed)).map(Some)
}

#[tauri::command]
pub fn bundle_import(db: State<'_, Db>, bundle_data: bundle::Bundle) -> AppResult<bundle::ImportResult> {
    db.with(|c| {
        let res = bundle::import(c, bundle_data)?;
        activity::log(c, "library.import", "Bundle importado para a biblioteca", None, None)?;
        Ok(res)
    })
}

// ── Skills ───────────────────────────────────────────────────────────────

/// Skills scanned from disk awaiting user confirmation (token → scan).
#[derive(Default)]
pub struct PendingSkills(pub std::sync::Mutex<HashMap<String, skill_format::ScannedSkill>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPreview {
    token: String,
    doc: skill_format::SkillDoc,
    files: Vec<skill_format::SkillFile>,
    warnings: Vec<String>,
    source: String,
    exists: bool,
}

#[derive(serde::Deserialize)]
pub struct SkillDocInput {
    name: String,
    description: String,
    body: String,
}

impl From<SkillDocInput> for skill_format::SkillDoc {
    fn from(d: SkillDocInput) -> Self {
        skill_format::SkillDoc { name: d.name.trim().to_string(), description: d.description.trim().to_string(), body: d.body }
    }
}

#[tauri::command]
pub fn skills_list(db: State<'_, Db>) -> AppResult<Vec<skills::Skill>> {
    db.with(skills::list)
}

#[tauri::command]
pub fn skill_detail(db: State<'_, Db>, id: String) -> AppResult<skills::SkillDetail> {
    db.with(|c| skills::detail(c, &id))
}

#[tauri::command]
pub fn skill_create(db: State<'_, Db>, doc: SkillDocInput) -> AppResult<skills::Skill> {
    db.with(|c| skills::create(c, &doc.into(), "local", None, &[]))
}

#[tauri::command]
pub fn skill_update(db: State<'_, Db>, id: String, doc: SkillDocInput, note: Option<String>) -> AppResult<skills::Skill> {
    db.with(|c| skills::update(c, &id, &doc.into(), note.as_deref().unwrap_or("editada")))
}

#[tauri::command]
pub fn skill_restore_version(db: State<'_, Db>, id: String, version: i64) -> AppResult<skills::Skill> {
    db.with(|c| skills::restore_version(c, &id, version))
}

#[tauri::command]
pub fn skill_restore_builtin(db: State<'_, Db>, id: String) -> AppResult<skills::Skill> {
    db.with(|c| skills::restore_builtin(c, &id))
}

#[tauri::command]
pub fn skill_set_enabled(db: State<'_, Db>, id: String, enabled: bool) -> AppResult<skills::Skill> {
    db.with(|c| skills::set_enabled(c, &id, enabled))
}

#[tauri::command]
pub fn skill_delete(db: State<'_, Db>, id: String) -> AppResult<()> {
    db.with(|c| skills::delete(c, &id))
}

#[tauri::command]
pub fn skill_set_link(db: State<'_, Db>, skill_id: String, target_kind: String, target_id: String, linked: bool) -> AppResult<()> {
    db.with(|c| skills::set_link(c, &skill_id, &target_kind, &target_id, linked))
}

#[tauri::command]
pub fn skills_linked(db: State<'_, Db>, target_kind: String, target_id: String) -> AppResult<Vec<skills::Skill>> {
    db.with(|c| skills::linked_to(c, &target_kind, &target_id))
}

/// Opens a native picker (folder with SKILL.md, or a SKILL.md file) and returns
/// a preview. Nothing is installed until `skill_install` is called with the token.
#[tauri::command]
pub async fn skill_pick(app: tauri::AppHandle, db: State<'_, Db>, pending: State<'_, PendingSkills>, folder: bool) -> AppResult<Option<SkillPreview>> {
    let picked = blocking(move || {
        let dialog = app.dialog().file().set_title("Instalar skill");
        Ok(if folder { dialog.blocking_pick_folder() } else { dialog.add_filter("SKILL.md", &["md"]).blocking_pick_file() })
    })
    .await?;
    let Some(fp) = picked else { return Ok(None) };
    let path = fp.into_path().map_err(|e| AppError::Internal(e.to_string()))?;
    let scanned = blocking(move || skill_format::scan(&path)).await?;
    let exists = db.with(|c| Ok(skills::list(c)?.iter().any(|s| s.slug == skill_format::slug(&scanned.doc.name))))?;
    let token = crate::db::new_id();
    pending.0.lock().map_err(|_| AppError::Internal("lock".into()))?.insert(token.clone(), scanned.clone());
    Ok(Some(SkillPreview { token, doc: scanned.doc, files: scanned.files.into_iter().map(|mut f| { f.content = None; f }).collect(), warnings: scanned.warnings, source: scanned.source, exists }))
}

#[tauri::command]
pub fn skill_install(db: State<'_, Db>, pending: State<'_, PendingSkills>, token: String, update_existing: bool) -> AppResult<skills::Skill> {
    let scanned = pending
        .0
        .lock()
        .map_err(|_| AppError::Internal("lock".into()))?
        .remove(&token)
        .ok_or_else(|| AppError::Invalid("prévia expirada — selecione a skill novamente".into()))?;
    db.with(|c| {
        let s = skills::install(c, &scanned, update_existing)?;
        activity::log(c, "skill.install", &format!("Skill “{}” instalada (v{})", s.name, s.version), None, None)?;
        Ok(s)
    })
}

/// Exports a skill as a `<slug>/SKILL.md` folder (plus text supporting files) into a chosen directory.
#[tauri::command]
pub async fn skill_export(app: tauri::AppHandle, db: State<'_, Db>, id: String) -> AppResult<Option<String>> {
    let d = db.with(|c| skills::detail(c, &id))?;
    let picked = blocking(move || Ok(app.dialog().file().set_title("Exportar skill para a pasta…").blocking_pick_folder())).await?;
    let Some(fp) = picked else { return Ok(None) };
    let base = fp.into_path().map_err(|e| AppError::Internal(e.to_string()))?;
    let mut dir = base.join(&d.skill.slug);
    let mut n = 2;
    while dir.exists() {
        dir = base.join(format!("{}-{n}", d.skill.slug));
        n += 1;
    }
    std::fs::create_dir_all(&dir)?;
    let doc = skill_format::SkillDoc { name: d.skill.name.clone(), description: d.skill.description.clone(), body: d.skill.body.clone() };
    std::fs::write(dir.join("SKILL.md"), skill_format::to_markdown(&doc))?;
    for f in &d.files {
        let Some(content) = &f.content else { continue };
        if crate::git::check_path(&f.path).is_err() {
            continue; // never write outside the export folder
        }
        let target = dir.join(&f.path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, content)?;
    }
    Ok(Some(dir.display().to_string()))
}

// ── MCP ──────────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpDetail {
    server: mcp::Server,
    /// Which declared secrets have a value in the keychain (never the values).
    secret_status: HashMap<String, bool>,
    links: Vec<(String, String)>,
}

#[tauri::command]
pub fn mcp_list(db: State<'_, Db>) -> AppResult<Vec<mcp::Server>> {
    db.with(mcp::list)
}

#[tauri::command]
pub async fn mcp_detail(db: State<'_, Db>, secrets: State<'_, Secrets>, id: String) -> AppResult<McpDetail> {
    let server = db.with(|c| mcp::get(c, &id))?;
    let links = db.with(|c| mcp::links_of(c, &id))?;
    let store = secrets.inner().clone();
    let s2 = server.clone();
    let secret_status = blocking(move || mcp::secret_status(store.as_ref(), &s2)).await?;
    Ok(McpDetail { server, secret_status, links })
}

#[tauri::command]
pub fn mcp_templates() -> Vec<mcp::Template> {
    mcp::templates()
}

#[tauri::command]
pub fn mcp_create(db: State<'_, Db>, input: mcp::ServerInput) -> AppResult<mcp::Server> {
    db.with(|c| mcp::create(c, &input, None))
}

#[tauri::command]
pub async fn mcp_create_from_template(
    db: State<'_, Db>,
    secrets: State<'_, Secrets>,
    template_id: String,
    label: Option<String>,
    values: HashMap<String, String>,
) -> AppResult<mcp::Server> {
    let store = secrets.inner().clone();
    db.with(|c| mcp::create_from_template(c, store.as_ref(), &template_id, label.as_deref(), &values))
}

#[tauri::command]
pub async fn mcp_update(db: State<'_, Db>, secrets: State<'_, Secrets>, id: String, input: mcp::ServerInput) -> AppResult<mcp::Server> {
    let store = secrets.inner().clone();
    db.with(|c| mcp::update(c, store.as_ref(), &id, &input))
}

#[tauri::command]
pub async fn mcp_delete(db: State<'_, Db>, secrets: State<'_, Secrets>, id: String) -> AppResult<()> {
    let store = secrets.inner().clone();
    db.with(|c| mcp::delete(c, store.as_ref(), &id))
}

#[tauri::command]
pub fn mcp_set_enabled(db: State<'_, Db>, id: String, enabled: bool) -> AppResult<mcp::Server> {
    db.with(|c| mcp::set_enabled(c, &id, enabled))
}

/// Stores a secret value in the OS keychain (the value is never returned).
#[tauri::command]
pub async fn mcp_set_secret(db: State<'_, Db>, secrets: State<'_, Secrets>, id: String, key: String, value: String) -> AppResult<()> {
    let store = secrets.inner().clone();
    db.with(|c| {
        mcp::set_secret(c, store.as_ref(), &id, &key, &value)?;
        // Audit: key name only, never the value.
        let name = mcp::get(c, &id).map(|s| s.name).unwrap_or_default();
        activity::log(c, "mcp.secret.set", &format!("Segredo {key} do MCP “{name}” definido no cofre"), None, None)
    })
}

#[tauri::command]
pub async fn mcp_clear_secret(db: State<'_, Db>, secrets: State<'_, Secrets>, id: String, key: String) -> AppResult<()> {
    let store = secrets.inner().clone();
    let (i2, k2) = (id.clone(), key.clone());
    blocking(move || mcp::clear_secret(store.as_ref(), &i2, &k2)).await?;
    let _ = db.with(|c| {
        let name = mcp::get(c, &id).map(|s| s.name).unwrap_or_default();
        activity::log(c, "mcp.secret.clear", &format!("Segredo {key} do MCP “{name}” removido do cofre"), None, None)
    });
    Ok(())
}

/// Connects to the server (initialize + tools/list), records and returns the result.
#[tauri::command]
pub async fn mcp_test(db: State<'_, Db>, secrets: State<'_, Secrets>, id: String) -> AppResult<mcp_client::ProbeResult> {
    let server = db.with(|c| mcp::get(c, &id))?;
    let store = secrets.inner().clone();
    let result = blocking(move || {
        let ep = mcp::endpoint(store.as_ref(), &server)?;
        Ok(mcp_client::probe(&ep, std::time::Duration::from_secs(180)))
    })
    .await?;
    db.with(|c| {
        mcp::record_probe(c, &id, &result)?;
        let s = mcp::get(c, &id)?;
        let msg = if result.ok { format!("MCP “{}” verificado: {} ferramenta(s)", s.label, result.tools.len()) } else { format!("Teste do MCP “{}” falhou", s.label) };
        activity::log(c, "mcp.test", &msg, None, None)
    })?;
    Ok(result)
}

#[tauri::command]
pub fn mcp_set_link(db: State<'_, Db>, server_id: String, target_kind: String, target_id: String, linked: bool) -> AppResult<()> {
    db.with(|c| mcp::set_link(c, &server_id, &target_kind, &target_id, linked))
}

#[tauri::command]
pub fn mcp_linked(db: State<'_, Db>, target_kind: String, target_id: String) -> AppResult<Vec<mcp::Server>> {
    db.with(|c| mcp::linked_to(c, &target_kind, &target_id))
}

// ── Missions ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn missions_list(db: State<'_, Db>, workspace_id: Option<String>, limit: Option<u32>) -> AppResult<Vec<missions::Mission>> {
    db.with(|c| missions::list(c, workspace_id.as_deref(), limit.unwrap_or(100)))
}

#[tauri::command]
pub fn missions_create(db: State<'_, Db>, input: missions::NewMission) -> AppResult<missions::Mission> {
    db.with(|c| {
        let m = missions::create(c, &input)?;
        activity::log(c, "mission.created", &format!("Missão “{}” criada como rascunho", m.title), m.workspace_id.as_deref(), Some(&m.id))?;
        Ok(m)
    })
}

#[tauri::command]
pub fn missions_cancel(db: State<'_, Db>, id: String) -> AppResult<missions::Mission> {
    db.with(|c| {
        let m = missions::cancel(c, &id)?;
        activity::log(c, "mission.cancelled", &format!("Missão “{}” cancelada", m.title), m.workspace_id.as_deref(), Some(&m.id))?;
        Ok(m)
    })
}

#[tauri::command]
pub fn missions_delete(db: State<'_, Db>, id: String) -> AppResult<()> {
    db.with(|c| {
        let m = missions::get(c, &id)?;
        missions::delete(c, &id)?;
        activity::log(c, "mission.deleted", &format!("Missão “{}” excluída", m.title), m.workspace_id.as_deref(), None)
    })
}

// ── Activity ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn activity_recent(db: State<'_, Db>, limit: Option<u32>) -> AppResult<Vec<activity::ActivityEvent>> {
    db.with(|c| activity::recent(c, limit.unwrap_or(30)))
}
