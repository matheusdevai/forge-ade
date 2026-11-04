//! Browser + preview commands (M13).

use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

use super::{blocking, log_ws, workspace_root};
use crate::browser::manager::{AttachSnapshot, BrowserEvent, BrowserManager, BrowserStatus, KeyInput, MouseInput, NavState, Shot, TabInfo, VerifyReport};
use crate::browser::{preview, ConsoleEntry};
use crate::db::Db;
use crate::environment;
use crate::error::{AppError, AppResult};
use crate::terminal::{SpawnSpec, TerminalInfo, TerminalManager};

pub type Browser = Arc<BrowserManager>;

#[tauri::command]
pub async fn browser_status(browser: State<'_, Browser>) -> AppResult<BrowserStatus> {
    let b = browser.inner().clone();
    blocking(move || Ok(b.status())).await
}

#[tauri::command]
pub async fn browser_open(
    db: State<'_, Db>,
    browser: State<'_, Browser>,
    workspace_id: String,
    url: Option<String>,
    width: u32,
    height: u32,
    mobile: Option<bool>,
) -> AppResult<TabInfo> {
    workspace_root(&db, &workspace_id)?;
    let b = browser.inner().clone();
    let ws = workspace_id.clone();
    let info = blocking(move || b.open(&ws, url.as_deref(), width, height, mobile.unwrap_or(false))).await?;
    log_ws(&db, &workspace_id, "browser.opened", "Navegador aberto");
    Ok(info)
}

#[tauri::command]
pub fn browser_list(browser: State<'_, Browser>, workspace_id: Option<String>) -> Vec<TabInfo> {
    browser.list(workspace_id.as_deref())
}

#[tauri::command]
pub async fn browser_attach(browser: State<'_, Browser>, id: String, on_event: Channel<BrowserEvent>) -> AppResult<AttachSnapshot> {
    let b = browser.inner().clone();
    blocking(move || {
        b.attach(&id, Arc::new(move |ev| {
            let _ = on_event.send(ev);
        }))
    })
    .await
}

#[tauri::command]
pub async fn browser_detach(browser: State<'_, Browser>, id: String, token: u64) -> AppResult<()> {
    let b = browser.inner().clone();
    blocking(move || b.detach(&id, token)).await
}

#[tauri::command]
pub async fn browser_navigate(browser: State<'_, Browser>, id: String, url: String) -> AppResult<String> {
    let b = browser.inner().clone();
    blocking(move || b.navigate(&id, &url)).await
}

#[tauri::command]
pub async fn browser_go(browser: State<'_, Browser>, id: String, delta: i64) -> AppResult<()> {
    let b = browser.inner().clone();
    blocking(move || b.go(&id, delta)).await
}

#[tauri::command]
pub async fn browser_reload(browser: State<'_, Browser>, id: String, ignore_cache: Option<bool>) -> AppResult<()> {
    let b = browser.inner().clone();
    blocking(move || b.reload(&id, ignore_cache.unwrap_or(false))).await
}

#[tauri::command]
pub async fn browser_nav_state(browser: State<'_, Browser>, id: String) -> AppResult<NavState> {
    let b = browser.inner().clone();
    blocking(move || b.nav_state(&id)).await
}

#[tauri::command]
pub async fn browser_viewport(browser: State<'_, Browser>, id: String, width: u32, height: u32, mobile: bool) -> AppResult<TabInfo> {
    let b = browser.inner().clone();
    blocking(move || b.set_viewport(&id, width, height, mobile)).await
}

#[tauri::command]
pub fn browser_mouse(browser: State<'_, Browser>, id: String, input: MouseInput) -> AppResult<()> {
    browser.mouse(&id, &input)
}

#[tauri::command]
pub fn browser_key(browser: State<'_, Browser>, id: String, input: KeyInput) -> AppResult<()> {
    browser.key(&id, &input)
}

#[tauri::command]
pub async fn browser_insert_text(browser: State<'_, Browser>, id: String, text: String) -> AppResult<()> {
    let b = browser.inner().clone();
    blocking(move || b.insert_text(&id, &text)).await
}

#[tauri::command]
pub fn browser_console(browser: State<'_, Browser>, id: String) -> AppResult<Vec<ConsoleEntry>> {
    browser.entries(&id)
}

#[tauri::command]
pub fn browser_console_clear(browser: State<'_, Browser>, id: String) -> AppResult<()> {
    browser.clear_entries(&id)
}

#[tauri::command]
pub async fn browser_screenshot(
    db: State<'_, Db>,
    browser: State<'_, Browser>,
    media: State<'_, super::media::Media>,
    id: String,
    full_page: Option<bool>,
) -> AppResult<Shot> {
    let b = browser.inner().clone();
    let tab = b.list(None).into_iter().find(|t| t.id == id);
    let shot = blocking(move || b.screenshot(&id, full_page.unwrap_or(false))).await?;
    // Screenshots also land in the media library (M17).
    if let Some(t) = tab {
        let _ = db.with(|c| media.record_screenshot(c, Some(&t.workspace_id), std::path::Path::new(&shot.path), &t.url));
    }
    Ok(shot)
}

#[tauri::command]
pub async fn browser_verify(db: State<'_, Db>, browser: State<'_, Browser>, id: String) -> AppResult<VerifyReport> {
    let b = browser.inner().clone();
    let ws = b.list(None).into_iter().find(|t| t.id == id).map(|t| t.workspace_id);
    let report = blocking(move || b.verify(&id, std::time::Duration::from_millis(1500))).await?;
    if let Some(ws) = ws {
        let msg = if report.ok {
            "Página verificada sem erros".to_string()
        } else {
            format!("Página verificada: {} erro(s), {} aviso(s)", report.errors.len(), report.warnings.len())
        };
        log_ws(&db, &ws, "browser.verified", &msg);
    }
    Ok(report)
}

#[tauri::command]
pub async fn browser_close(browser: State<'_, Browser>, id: String) -> AppResult<()> {
    let b = browser.inner().clone();
    blocking(move || b.close(&id)).await
}

/// Reveals a FORGE screenshot in the file manager (only files in shots/).
#[tauri::command]
pub fn browser_reveal_shot(app: tauri::AppHandle, browser: State<'_, Browser>, path: String) -> AppResult<()> {
    let p = std::fs::canonicalize(&path)?;
    let dir = std::fs::canonicalize(browser.shots_dir()).map_err(|_| AppError::NotFound("nenhuma captura salva".into()))?;
    if !p.starts_with(&dir) {
        return Err(AppError::Forbidden("só é possível revelar capturas do FORGE".into()));
    }
    app.opener().reveal_item_in_dir(&p).map_err(|e| AppError::Internal(e.to_string()))
}

#[tauri::command]
pub fn preview_detect(db: State<'_, Db>, workspace_id: String) -> AppResult<preview::PreviewInfo> {
    let root = workspace_root(&db, &workspace_id)?;
    preview::detect(&root)
}

/// Starts a package.json script in a real terminal session of the workspace.
#[tauri::command]
pub fn preview_start(
    db: State<'_, Db>,
    terminals: State<'_, TerminalManager>,
    workspace_id: String,
    script: String,
    cols: u16,
    rows: u16,
) -> AppResult<TerminalInfo> {
    let root = std::fs::canonicalize(workspace_root(&db, &workspace_id)?)?;
    let (pm, args) = preview::command_for(&root, &script)?;
    let bin = environment::which(&pm).ok_or_else(|| AppError::NotFound(format!("{pm} não está instalado")))?;
    let info = terminals.spawn(SpawnSpec {
        workspace_id: workspace_id.clone(),
        cwd: root,
        cols,
        rows,
        program: Some((bin.display().to_string(), args)),
        path_env: Some(environment::search_path().clone()),
        extra_env: Some(environment::login_env().clone()),
        provider_id: None,
    })?;
    log_ws(&db, &workspace_id, "preview.started", &format!("Prévia iniciada: {pm} run {script}"));
    Ok(info)
}
