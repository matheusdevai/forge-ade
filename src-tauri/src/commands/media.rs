//! Media commands (M17).
//!
//! Input files never come as raw paths from the UI: they are either picked in
//! the native dialog (exchanged for a one-time token) or an existing asset.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use super::voice::Voice;
use super::{blocking, log_ws, workspace_root, AppPaths};
use crate::db::{activity, Db};
use crate::error::{AppError, AppResult};
use crate::mcp::secrets::Secrets;
use crate::media::{self, Asset, GeminiStatus, MediaEngine, GEMINI_ACCOUNT};

pub type Media = Arc<MediaEngine>;

#[derive(Default)]
pub struct PickedFiles(Mutex<HashMap<String, PathBuf>>);

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MediaInput {
    Picked { token: String },
    Asset { id: String },
}

fn input_path(db: &Db, picked: &PickedFiles, input: &MediaInput) -> AppResult<PathBuf> {
    match input {
        MediaInput::Picked { token } => picked.0.lock().unwrap().get(token).cloned().ok_or_else(|| AppError::NotFound("arquivo escolhido expirou — escolha de novo".into())),
        MediaInput::Asset { id } => {
            let a = db.with(|c| media::get(c, id))?;
            a.path.map(PathBuf::from).ok_or_else(|| AppError::Invalid("esse item não tem arquivo".into()))
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaStatus {
    pub gemini: GeminiStatus,
    pub claude_installed: bool,
    pub macos_speech: Option<String>,
}

#[tauri::command]
pub async fn media_status(db: State<'_, Db>, media: State<'_, Media>, secrets: State<'_, Secrets>, voice: State<'_, Voice>) -> AppResult<MediaStatus> {
    let gemini = db.with(|c| media.gemini_status(c, secrets.as_ref()))?;
    let v = voice.inner().clone();
    let speech = blocking(move || Ok(if cfg!(target_os = "macos") { Some(v.status().speech) } else { None })).await?;
    Ok(MediaStatus { gemini, claude_installed: crate::environment::which("claude").is_some(), macos_speech: speech })
}

/// Stores the key in the OS keychain and immediately tests it for real.
#[tauri::command]
pub async fn media_set_key(paths: State<'_, AppPaths>, db: State<'_, Db>, media: State<'_, Media>, secrets: State<'_, Secrets>, key: String) -> AppResult<GeminiStatus> {
    let key = key.trim().to_string();
    if key.len() < 20 || key.len() > 200 || key.chars().any(char::is_whitespace) {
        return Err(AppError::Invalid("isso não parece uma chave de API do Gemini".into()));
    }
    secrets.set(GEMINI_ACCOUNT, &key)?;
    let (m, s, p) = (media.inner().clone(), secrets.inner().clone(), paths.db_path.clone());
    let st = blocking(move || { let c = &crate::db::connect(&p)?; m.gemini_test(c, s.as_ref()) }).await?;
    let _ = db.with(|c| activity::log(c, "media.key", if st.state.last_ok == Some(true) { "Chave do Gemini salva e verificada" } else { "Chave do Gemini salva (teste falhou)" }, None, None));
    Ok(st)
}

#[tauri::command]
pub fn media_clear_key(db: State<'_, Db>, media: State<'_, Media>, secrets: State<'_, Secrets>) -> AppResult<GeminiStatus> {
    secrets.delete(GEMINI_ACCOUNT)?;
    let _ = db.with(|c| activity::log(c, "media.key.clear", "Chave do Gemini removida do Keychain", None, None));
    db.with(|c| {
        c.execute("UPDATE media_providers SET last_ok = NULL, last_error = NULL WHERE id = 'gemini'", [])?;
        media.gemini_status(c, secrets.as_ref())
    })
}

#[tauri::command]
pub async fn media_test(paths: State<'_, AppPaths>, media: State<'_, Media>, secrets: State<'_, Secrets>) -> AppResult<GeminiStatus> {
    let (m, s, p) = (media.inner().clone(), secrets.inner().clone(), paths.db_path.clone());
    blocking(move || { let c = &crate::db::connect(&p)?; m.gemini_test(c, s.as_ref()) }).await
}

#[tauri::command]
pub fn media_set_model(db: State<'_, Db>, media: State<'_, Media>, secrets: State<'_, Secrets>, op: String, model: Option<String>) -> AppResult<GeminiStatus> {
    db.with(|c| {
        media.set_model(c, &op, model.as_deref())?;
        media.gemini_status(c, secrets.as_ref())
    })
}

/// Native file dialog; returns a one-time token for the chosen file.
#[tauri::command]
pub async fn media_pick(app: tauri::AppHandle, picked: State<'_, PickedFiles>, kind: String) -> AppResult<Option<(String, String)>> {
    let (title, exts): (&str, &[&str]) = match kind.as_str() {
        "image" => ("Escolher imagem", &["png", "jpg", "jpeg", "webp", "gif"]),
        "audio" => ("Escolher áudio", &["wav", "mp3", "m4a", "aiff", "aif", "flac", "ogg"]),
        _ => return Err(AppError::Invalid("tipo inválido".into())),
    };
    let fp = blocking(move || Ok(app.dialog().file().set_title(title).add_filter(title, exts).blocking_pick_file())).await?;
    let Some(fp) = fp else { return Ok(None) };
    let path = fp.into_path().map_err(|e| AppError::Internal(e.to_string()))?;
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let token = crate::db::new_id();
    picked.0.lock().unwrap().insert(token.clone(), path);
    Ok(Some((token, name)))
}

fn done(db: &Db, a: &Asset, what: &str) {
    if let Some(ws) = &a.workspace_id {
        log_ws(db, ws, "media.created", what);
    }
}

#[tauri::command]
pub async fn media_generate_image(paths: State<'_, AppPaths>, db: State<'_, Db>, media: State<'_, Media>, secrets: State<'_, Secrets>, workspace_id: Option<String>, prompt: String) -> AppResult<Asset> {
    let (m, s, p) = (media.inner().clone(), secrets.inner().clone(), paths.db_path.clone());
    let a = blocking(move || { let c = &crate::db::connect(&p)?; m.generate_image(c, s.as_ref(), workspace_id.as_deref(), &prompt) }).await?;
    done(&db, &a, "Imagem gerada com o Gemini");
    Ok(a)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn media_analyze_image(paths: State<'_, AppPaths>, 
    db: State<'_, Db>,
    media: State<'_, Media>,
    secrets: State<'_, Secrets>,
    picked: State<'_, PickedFiles>,
    workspace_id: Option<String>,
    input: MediaInput,
    prompt: String,
    provider: String,
) -> AppResult<Asset> {
    let path = input_path(&db, &picked, &input)?;
    let (m, s, p) = (media.inner().clone(), secrets.inner().clone(), paths.db_path.clone());
    let a = blocking(move || { let c = &crate::db::connect(&p)?; m.analyze_image(c, s.as_ref(), workspace_id.as_deref(), &path, &prompt, &provider) }).await?;
    done(&db, &a, "Imagem analisada");
    Ok(a)
}

#[tauri::command]
pub async fn media_tts(paths: State<'_, AppPaths>, db: State<'_, Db>, media: State<'_, Media>, secrets: State<'_, Secrets>, workspace_id: Option<String>, text: String, voice: String) -> AppResult<Asset> {
    let (m, s, p) = (media.inner().clone(), secrets.inner().clone(), paths.db_path.clone());
    let a = blocking(move || { let c = &crate::db::connect(&p)?; m.tts(c, s.as_ref(), workspace_id.as_deref(), &text, &voice) }).await?;
    done(&db, &a, "Áudio gerado com o Gemini");
    Ok(a)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn media_transcribe(paths: State<'_, AppPaths>, 
    db: State<'_, Db>,
    media: State<'_, Media>,
    secrets: State<'_, Secrets>,
    voice: State<'_, Voice>,
    picked: State<'_, PickedFiles>,
    workspace_id: Option<String>,
    input: MediaInput,
    provider: String,
    locale: String,
) -> AppResult<Asset> {
    let path = input_path(&db, &picked, &input)?;
    let (m, s, p, v) = (media.inner().clone(), secrets.inner().clone(), paths.db_path.clone(), voice.inner().clone());
    let a = blocking(move || { let c = &crate::db::connect(&p)?; m.transcribe(c, s.as_ref(), v.as_ref(), workspace_id.as_deref(), &path, &provider, &locale) }).await?;
    done(&db, &a, "Áudio transcrito");
    Ok(a)
}

/// Starts a video job and polls it in the background (every 10 s, up to 15 min).
#[tauri::command]
pub async fn media_video_start(paths: State<'_, AppPaths>, app: tauri::AppHandle, media: State<'_, Media>, secrets: State<'_, Secrets>, workspace_id: Option<String>, prompt: String) -> AppResult<Asset> {
    let (m, s, p) = (media.inner().clone(), secrets.inner().clone(), paths.db_path.clone());
    let a = blocking(move || { let c = &crate::db::connect(&p)?; m.start_video(c, s.as_ref(), workspace_id.as_deref(), &prompt) }).await?;
    let (m, s, p, id) = (media.inner().clone(), secrets.inner().clone(), paths.db_path.clone(), a.id.clone());
    std::thread::spawn(move || {
        for _ in 0..90 {
            std::thread::sleep(Duration::from_secs(10));
            let r = crate::db::connect(&p).and_then(|c| m.poll_video(&c, s.as_ref(), &id));
            match r {
                Ok(false) => continue,
                _ => break,
            }
        }
        let _ = crate::db::connect(&p).and_then(|c| {
            if media::get(&c, &id)?.status == "running" {
                c.execute("UPDATE media_assets SET status='failed', error='tempo esgotado aguardando o vídeo (15 min)' WHERE id=?1", [&id])?;
            }
            Ok(())
        });
        let _ = app.emit("forge://media-changed", id);
    });
    Ok(a)
}

#[tauri::command]
pub fn media_list(db: State<'_, Db>, workspace_id: Option<String>, limit: Option<i64>) -> AppResult<Vec<Asset>> {
    db.with(|c| media::list(c, workspace_id.as_deref(), limit.unwrap_or(200)))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaData {
    pub mime: String,
    pub data: String,
}

/// File contents for preview (only files of assets; max 60 MB).
#[tauri::command]
pub async fn media_read(db: State<'_, Db>, id: String) -> AppResult<MediaData> {
    let a = db.with(|c| media::get(c, &id))?;
    let path = a.path.ok_or_else(|| AppError::Invalid("esse item não tem arquivo".into()))?;
    blocking(move || {
        let meta = std::fs::metadata(&path).map_err(|_| AppError::NotFound("arquivo não encontrado".into()))?;
        if meta.len() > 60 * 1024 * 1024 {
            return Err(AppError::Invalid("arquivo grande demais para pré-visualizar".into()));
        }
        let bytes = std::fs::read(&path)?;
        let mime = media::sniff(&bytes).map(|(m, _)| m.to_string()).or(a.mime).unwrap_or_else(|| "application/octet-stream".into());
        Ok(MediaData { mime, data: base64::engine::general_purpose::STANDARD.encode(bytes) })
    })
    .await
}

#[tauri::command]
pub fn media_delete(db: State<'_, Db>, media: State<'_, Media>, id: String) -> AppResult<()> {
    db.with(|c| media::delete(c, &media.media_dir(), &id))
}

#[tauri::command]
pub fn media_reveal(app: tauri::AppHandle, db: State<'_, Db>, id: String) -> AppResult<()> {
    let a = db.with(|c| media::get(c, &id))?;
    let p = a.path.ok_or_else(|| AppError::Invalid("esse item não tem arquivo".into()))?;
    app.opener().reveal_item_in_dir(&p).map_err(|e| AppError::Internal(e.to_string()))
}

#[tauri::command]
pub fn media_save_to_workspace(db: State<'_, Db>, media: State<'_, Media>, id: String, workspace_id: String, rel_dir: String) -> AppResult<String> {
    let root = workspace_root(&db, &workspace_id)?;
    let rel = db.with(|c| media.save_to_workspace(c, &id, &root, &rel_dir))?;
    log_ws(&db, &workspace_id, "media.saved", &format!("Mídia salva em {rel}"));
    Ok(rel)
}
