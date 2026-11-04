//! Voice commands (M15).

use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::State;

use super::{blocking, AppPaths};
use crate::agents::runner::RunManager;
use crate::db::{activity, Db};
use crate::missions::engine::MissionEngine;
use crate::error::AppResult;
use crate::voice::{pause_plan, PausePlan, SelfTest, VoiceEvent, VoiceLocale, VoiceManager, VoiceStatus};

pub type Voice = Arc<VoiceManager>;

#[tauri::command]
pub async fn voice_status(voice: State<'_, Voice>) -> AppResult<VoiceStatus> {
    let v = voice.inner().clone();
    blocking(move || Ok(v.status())).await
}

#[tauri::command]
pub async fn voice_authorize(voice: State<'_, Voice>) -> AppResult<VoiceStatus> {
    let v = voice.inner().clone();
    blocking(move || v.authorize()).await
}

#[tauri::command]
pub async fn voice_locales(voice: State<'_, Voice>) -> AppResult<Vec<VoiceLocale>> {
    let v = voice.inner().clone();
    blocking(move || v.locales()).await
}

#[tauri::command]
pub async fn voice_listen(voice: State<'_, Voice>, locale: String, on_device: bool, silence: Option<f64>, on_event: Channel<VoiceEvent>) -> AppResult<u64> {
    let v = voice.inner().clone();
    blocking(move || {
        v.listen(&locale, on_device, silence.unwrap_or(1.6), Arc::new(move |ev| {
            let _ = on_event.send(ev);
        }))
    })
    .await
}

#[tauri::command]
pub fn voice_stop(voice: State<'_, Voice>) {
    voice.stop();
}

#[tauri::command]
pub fn voice_cancel(voice: State<'_, Voice>) {
    voice.cancel();
}

#[tauri::command]
pub fn voice_speak(voice: State<'_, Voice>, text: String) -> AppResult<()> {
    voice.speak(&text)
}

#[tauri::command]
pub fn voice_stop_speaking(voice: State<'_, Voice>) {
    voice.stop_speaking();
}

#[tauri::command]
pub async fn voice_self_test(voice: State<'_, Voice>, paths: State<'_, AppPaths>, locale: String, on_device: bool) -> AppResult<SelfTest> {
    let v = voice.inner().clone();
    let scratch = paths.data_dir.join("voice-tmp");
    blocking(move || v.self_test(&locale, on_device, &scratch)).await
}

/// Jarvis "pause all agents". `dry_run` returns what would be affected (for
/// the confirmation); otherwise pauses running missions and stops standalone runs.
#[tauri::command]
pub fn agents_pause_all(db: State<'_, Db>, engine: State<'_, MissionEngine>, runs: State<'_, RunManager>, dry_run: bool) -> AppResult<PausePlan> {
    let plan = db.with(pause_plan)?;
    if dry_run {
        return Ok(plan);
    }
    for m in &plan.missions {
        let _ = engine.pause(&m.id);
    }
    for r in &plan.runs {
        let _ = runs.cancel(&r.id);
    }
    let msg = format!("Jarvis: {} missão(ões) pausada(s), {} execução(ões) interrompida(s)", plan.missions.len(), plan.runs.len());
    let _ = db.with(|c| activity::log(c, "voice.pause_all", &msg, None, None));
    Ok(plan)
}
