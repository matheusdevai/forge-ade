//! App lifecycle commands (M20): updates and local crash reports.

use serde::Serialize;
use tauri::State;
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::UpdaterExt;

use super::AppPaths;
use crate::crash::{self, CrashReport};
use crate::error::{AppError, AppResult};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub available: bool,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub date: Option<String>,
}

fn upd_err(e: impl std::fmt::Display) -> AppError {
    AppError::Internal(format!("atualizador: {e}"))
}

/// Checks GitHub Releases (latest.json). Signature is verified on install.
#[tauri::command]
pub async fn app_check_update(app: tauri::AppHandle) -> AppResult<UpdateInfo> {
    let current = app.package_info().version.to_string();
    let update = app.updater().map_err(upd_err)?.check().await.map_err(upd_err)?;
    Ok(match update {
        Some(u) => UpdateInfo { current, available: true, version: Some(u.version.clone()), notes: u.body.clone(), date: u.date.map(|d| d.to_string()) },
        None => UpdateInfo { current, available: false, version: None, notes: None, date: None },
    })
}

/// Downloads, verifies the minisign signature, installs and restarts.
#[tauri::command]
pub async fn app_install_update(app: tauri::AppHandle) -> AppResult<()> {
    let update = app.updater().map_err(upd_err)?.check().await.map_err(upd_err)?.ok_or_else(|| AppError::NotFound("nenhuma atualização disponível".into()))?;
    update.download_and_install(|_, _| {}, || {}).await.map_err(upd_err)?;
    app.restart();
}

#[tauri::command]
pub fn crash_list(paths: State<'_, AppPaths>) -> AppResult<Vec<CrashReport>> {
    crash::list(&paths.data_dir.join("crash"))
}

#[tauri::command]
pub fn crash_read(paths: State<'_, AppPaths>, file: String) -> AppResult<String> {
    crash::read(&paths.data_dir.join("crash"), &file)
}

#[tauri::command]
pub fn crash_clear(paths: State<'_, AppPaths>) -> AppResult<usize> {
    crash::clear(&paths.data_dir.join("crash"))
}

/// Records an unhandled UI error (from the error boundary / window.onerror).
#[tauri::command]
pub fn crash_report_ui(paths: State<'_, AppPaths>, summary: String, details: String) -> AppResult<()> {
    let summary: String = summary.chars().take(500).collect();
    let details: String = details.chars().take(32_000).collect();
    crash::write(&paths.data_dir.join("crash"), "ui", &summary, &details).map(|_| ())
}

#[tauri::command]
pub fn crash_reveal(app: tauri::AppHandle, paths: State<'_, AppPaths>) -> AppResult<()> {
    let dir = paths.data_dir.join("crash");
    std::fs::create_dir_all(&dir)?;
    app.opener().reveal_item_in_dir(&dir).map_err(|e| AppError::Internal(e.to_string()))
}
