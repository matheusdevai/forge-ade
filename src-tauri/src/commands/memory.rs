//! Shared memory commands (M14).

use serde::Serialize;
use tauri::State;

use crate::db::{activity, Db};
use crate::error::AppResult;
use crate::memory::{self, Memory, MemoryCounts, MemoryFilter, MemoryInput};

fn log(db: &Db, m: &Memory, kind: &str, msg: &str) {
    let _ = db.with(|c| activity::log(c, kind, msg, m.workspace_id.as_deref(), m.mission_id.as_deref()));
}

#[tauri::command]
pub fn memory_list(db: State<'_, Db>, filter: MemoryFilter) -> AppResult<Vec<Memory>> {
    db.with(|c| memory::list(c, &filter))
}

#[tauri::command]
pub fn memory_counts(db: State<'_, Db>, workspace_id: Option<String>) -> AppResult<MemoryCounts> {
    db.with(|c| memory::counts(c, workspace_id.as_deref()))
}

#[tauri::command]
pub fn memory_create(db: State<'_, Db>, input: MemoryInput) -> AppResult<Memory> {
    let m = db.with(|c| memory::create(c, &input))?;
    log(&db, &m, "memory.created", &format!("Memória criada: {}", m.title));
    Ok(m)
}

#[tauri::command]
pub fn memory_update(db: State<'_, Db>, id: String, input: MemoryInput) -> AppResult<Memory> {
    let m = db.with(|c| memory::update(c, &id, &input))?;
    log(&db, &m, "memory.updated", &format!("Memória editada: {}", m.title));
    Ok(m)
}

#[tauri::command]
pub fn memory_set_status(db: State<'_, Db>, id: String, status: String) -> AppResult<Memory> {
    let m = db.with(|c| memory::set_status(c, &id, &status))?;
    let what = if m.status == "archived" { "arquivada" } else { "reativada" };
    log(&db, &m, "memory.status", &format!("Memória {what}: {}", m.title));
    Ok(m)
}

#[tauri::command]
pub fn memory_set_pinned(db: State<'_, Db>, id: String, pinned: bool) -> AppResult<Memory> {
    db.with(|c| memory::set_pinned(c, &id, pinned))
}

#[tauri::command]
pub fn memory_approve(db: State<'_, Db>, id: String) -> AppResult<Memory> {
    let m = db.with(|c| memory::approve(c, &id))?;
    log(&db, &m, "memory.approved", &format!("Sugestão de memória aprovada: {}", m.title));
    Ok(m)
}

#[tauri::command]
pub fn memory_delete(db: State<'_, Db>, id: String) -> AppResult<()> {
    let m = db.with(|c| memory::delete(c, &id))?;
    let msg = if m.status == "suggested" { "Sugestão de memória rejeitada" } else { "Memória excluída" };
    log(&db, &m, "memory.deleted", &format!("{msg}: {}", m.title));
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryPreview {
    pub labels: Vec<String>,
    pub dropped: Vec<String>,
    pub bytes: usize,
}

/// What an agent would receive for this context (exactly what the runner composes).
#[tauri::command]
pub fn memory_preview(db: State<'_, Db>, workspace_id: Option<String>, mission_id: Option<String>, profile_id: Option<String>) -> AppResult<MemoryPreview> {
    let c = db.with(|c| memory::compose(c, workspace_id.as_deref(), mission_id.as_deref(), profile_id.as_deref()))?;
    Ok(MemoryPreview { bytes: c.text.len(), labels: c.labels, dropped: c.dropped })
}
