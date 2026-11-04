//! Autopilot commands (M18).

use tauri::State;

use crate::autopilot::{self, Autopilot, Routine, RoutineInput, RoutineRun};
use crate::db::{activity, Db};
use crate::error::AppResult;

#[tauri::command]
pub fn autopilot_list(db: State<'_, Db>, workspace_id: Option<String>) -> AppResult<Vec<Routine>> {
    db.with(|c| autopilot::list(c, workspace_id.as_deref()))
}

#[tauri::command]
pub fn autopilot_create(db: State<'_, Db>, input: RoutineInput) -> AppResult<Routine> {
    db.with(|c| {
        let r = autopilot::create(c, &input)?;
        activity::log(c, "autopilot.created", &format!("Rotina “{}” criada", r.name), Some(&r.workspace_id), None)?;
        Ok(r)
    })
}

#[tauri::command]
pub fn autopilot_update(db: State<'_, Db>, id: String, input: RoutineInput) -> AppResult<Routine> {
    db.with(|c| autopilot::update(c, &id, &input))
}

#[tauri::command]
pub fn autopilot_set_enabled(db: State<'_, Db>, id: String, enabled: bool) -> AppResult<Routine> {
    db.with(|c| autopilot::set_enabled(c, &id, enabled))
}

#[tauri::command]
pub fn autopilot_delete(db: State<'_, Db>, id: String) -> AppResult<()> {
    db.with(|c| {
        let r = autopilot::get(c, &id)?;
        autopilot::delete(c, &id)?;
        activity::log(c, "autopilot.deleted", &format!("Rotina “{}” excluída", r.name), Some(&r.workspace_id), None)
    })
}

#[tauri::command]
pub fn autopilot_run_now(ap: State<'_, Autopilot>, id: String) -> AppResult<RoutineRun> {
    ap.trigger(&id, "manual")
}

#[tauri::command]
pub fn autopilot_cancel(ap: State<'_, Autopilot>, run_id: String) {
    ap.cancel(&run_id);
}

#[tauri::command]
pub fn autopilot_runs(db: State<'_, Db>, routine_id: Option<String>, limit: Option<i64>) -> AppResult<Vec<RoutineRun>> {
    db.with(|c| autopilot::runs(c, routine_id.as_deref(), limit.unwrap_or(50)))
}
