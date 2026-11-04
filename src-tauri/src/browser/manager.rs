//! Browser tabs: one headless Chrome process (launched lazily, closed when the
//! last tab closes) and one CDP session per tab, multiplexed over a single
//! browser WebSocket (`Target.attachToTarget` with `flatten`).
//!
//! Console entries live in memory only (never persisted); the verify report
//! is redacted before it leaves this module.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::cdp::{CdpConn, EventHandler};
use super::{chrome, entry_from_event, normalize_url, ConsoleEntry, MAX_ENTRIES};
use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::redact::redact_secrets;

pub const MAX_TABS: usize = 16;
const CALL: Duration = Duration::from_secs(15);
const MAX_TRACKED_REQUESTS: usize = 4000;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BrowserEvent {
    Frame { data: String, width: u32, height: u32 },
    Nav { url: String, title: String, loading: bool },
    Console { entry: ConsoleEntry },
    Closed { reason: String },
}

pub type Subscriber = Arc<dyn Fn(BrowserEvent) + Send + Sync>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabInfo {
    pub id: String,
    pub workspace_id: String,
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub width: u32,
    pub height: u32,
    pub mobile: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachSnapshot {
    pub info: TabInfo,
    pub entries: Vec<ConsoleEntry>,
    pub token: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavState {
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shot {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStatus {
    pub available: bool,
    pub name: Option<String>,
    pub path: Option<String>,
    pub running: bool,
    pub version: Option<String>,
    pub tabs: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyReport {
    pub url: String,
    pub title: String,
    pub ok: bool,
    pub loaded: bool,
    pub elapsed_ms: u64,
    pub errors: Vec<ConsoleEntry>,
    pub warnings: Vec<ConsoleEntry>,
    pub screenshot: Option<Shot>,
    /// Markdown summary, redacted, ready to hand to an agent.
    pub summary: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MouseInput {
    /// mousePressed | mouseReleased | mouseMoved | mouseWheel
    pub kind: String,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub button: Option<String>,
    #[serde(default)]
    pub click_count: Option<u32>,
    #[serde(default)]
    pub delta_x: Option<f64>,
    #[serde(default)]
    pub delta_y: Option<f64>,
    #[serde(default)]
    pub modifiers: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyInput {
    /// keyDown | keyUp | rawKeyDown | char
    pub kind: String,
    pub key: String,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub key_code: Option<u32>,
    #[serde(default)]
    pub modifiers: Option<u32>,
}

struct TabState {
    url: String,
    title: String,
    loading: bool,
    width: u32,
    height: u32,
    mobile: bool,
    entries: VecDeque<ConsoleEntry>,
    requests: HashMap<String, String>,
    loads: u64,
    subscriber: Option<(u64, Subscriber)>,
    screencast: bool,
}

pub struct Tab {
    pub id: String,
    pub workspace_id: String,
    target_id: String,
    session_id: String,
    created_at: i64,
    state: Mutex<TabState>,
}

impl Tab {
    fn info(&self) -> TabInfo {
        let s = self.state.lock().unwrap();
        TabInfo {
            id: self.id.clone(),
            workspace_id: self.workspace_id.clone(),
            url: s.url.clone(),
            title: s.title.clone(),
            loading: s.loading,
            width: s.width,
            height: s.height,
            mobile: s.mobile,
            created_at: self.created_at,
        }
    }

    fn emit(&self, ev: BrowserEvent) {
        let sub = self.state.lock().unwrap().subscriber.as_ref().map(|(_, f)| f.clone());
        if let Some(f) = sub {
            f(ev);
        }
    }

    fn emit_nav(&self) {
        let (url, title, loading) = {
            let s = self.state.lock().unwrap();
            (s.url.clone(), s.title.clone(), s.loading)
        };
        self.emit(BrowserEvent::Nav { url, title, loading });
    }

    fn push_entry(&self, level: &str, source: &str, text: String, url: Option<String>) {
        let entry = {
            let mut s = self.state.lock().unwrap();
            let seq = s.entries.back().map(|e| e.seq + 1).unwrap_or(1);
            let e = ConsoleEntry { seq, ts: now_ms(), level: level.into(), source: source.into(), text, url };
            s.entries.push_back(e.clone());
            while s.entries.len() > MAX_ENTRIES {
                s.entries.pop_front();
            }
            e
        };
        self.emit(BrowserEvent::Console { entry });
    }

    fn last_seq(&self) -> u64 {
        self.state.lock().unwrap().entries.back().map(|e| e.seq).unwrap_or(0)
    }
}

struct Engine {
    conn: Arc<CdpConn>,
    child: Mutex<Child>,
    version: String,
}

impl Engine {
    fn shutdown(&self) {
        let _ = self.conn.call("Browser.close", json!({}), Duration::from_secs(2));
        self.conn.close();
        let mut c = self.child.lock().unwrap();
        for _ in 0..20 {
            if matches!(c.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        #[cfg(unix)]
        {
            let _ = std::process::Command::new("kill").args(["-KILL", &format!("-{}", c.id())]).status();
        }
        let _ = c.kill();
        let _ = c.wait();
    }
}

type Tabs = Arc<Mutex<HashMap<String, Arc<Tab>>>>;

pub struct BrowserManager {
    data_dir: PathBuf,
    chrome_override: Option<PathBuf>,
    engine: Mutex<Option<Arc<Engine>>>,
    tabs: Tabs,
    tokens: AtomicU64,
}

impl BrowserManager {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir, chrome_override: None, engine: Mutex::default(), tabs: Arc::default(), tokens: AtomicU64::new(1) }
    }

    /// For tests: use a specific browser binary.
    pub fn with_chrome(mut self, bin: PathBuf) -> Self {
        self.chrome_override = Some(bin);
        self
    }

    fn chrome_bin(&self) -> Option<PathBuf> {
        self.chrome_override.clone().or_else(chrome::find)
    }

    pub fn shots_dir(&self) -> PathBuf {
        self.data_dir.join("browser").join("shots")
    }

    /// PID of the running headless browser, if any.
    pub fn pid(&self) -> Option<u32> {
        self.engine.lock().unwrap().as_ref().filter(|e| !e.conn.is_closed()).map(|e| e.child.lock().unwrap().id())
    }

    pub fn status(&self) -> BrowserStatus {
        let bin = self.chrome_bin();
        let engine = self.engine.lock().unwrap().clone().filter(|e| !e.conn.is_closed());
        BrowserStatus {
            available: bin.is_some(),
            name: bin.as_deref().map(chrome::display_name),
            path: bin.map(|b| b.display().to_string()),
            running: engine.is_some(),
            version: engine.map(|e| e.version.clone()),
            tabs: self.tabs.lock().unwrap().len(),
        }
    }

    fn engine(&self) -> AppResult<Arc<Engine>> {
        let mut guard = self.engine.lock().unwrap();
        if let Some(e) = guard.as_ref() {
            if !e.conn.is_closed() {
                return Ok(e.clone());
            }
            e.shutdown();
            *guard = None;
        }
        let bin = self.chrome_bin().ok_or_else(|| {
            AppError::NotFound("nenhum navegador compatível encontrado (Google Chrome, Chromium, Brave ou Edge)".into())
        })?;
        let profile = self.data_dir.join("browser").join("profile");
        let launched = chrome::launch(&bin, &profile, Duration::from_secs(20))?;
        let conn_slot: Arc<OnceLock<Weak<CdpConn>>> = Arc::default();
        let handler = make_handler(self.tabs.clone(), conn_slot.clone());
        let conn = match CdpConn::connect(&launched.ws_url, handler) {
            Ok(c) => Arc::new(c),
            Err(e) => {
                let mut child = launched.child;
                let _ = child.kill();
                return Err(e);
            }
        };
        let _ = conn_slot.set(Arc::downgrade(&conn));
        let version = conn
            .call("Browser.getVersion", json!({}), CALL)
            .ok()
            .and_then(|v| v.get("product").and_then(Value::as_str).map(String::from))
            .unwrap_or_default();
        conn.call("Target.setDiscoverTargets", json!({ "discover": true }), CALL)?;
        let engine = Arc::new(Engine { conn, child: Mutex::new(launched.child), version });
        *guard = Some(engine.clone());
        Ok(engine)
    }

    fn tab(&self, id: &str) -> AppResult<Arc<Tab>> {
        self.tabs.lock().unwrap().get(id).cloned().ok_or_else(|| AppError::NotFound("aba do navegador não encontrada".into()))
    }

    fn send(&self, tab: &Tab, method: &str, params: Value) -> AppResult<Value> {
        let engine = self.engine.lock().unwrap().clone().ok_or_else(|| AppError::Conflict("o navegador foi encerrado".into()))?;
        engine.conn.call_in(Some(&tab.session_id), method, params, CALL)
    }

    fn notify(&self, tab: &Tab, method: &str, params: Value) {
        if let Some(engine) = self.engine.lock().unwrap().clone() {
            engine.conn.notify_in(Some(&tab.session_id), method, params);
        }
    }

    pub fn open(&self, workspace_id: &str, url: Option<&str>, width: u32, height: u32, mobile: bool) -> AppResult<TabInfo> {
        if self.tabs.lock().unwrap().len() >= MAX_TABS {
            return Err(AppError::Conflict(format!("limite de {MAX_TABS} abas do navegador atingido")));
        }
        let target = match url {
            Some(u) if !u.trim().is_empty() => Some(normalize_url(u)?),
            _ => None,
        };
        let engine = self.engine()?;
        let created = engine.conn.call("Target.createTarget", json!({ "url": "about:blank" }), CALL)?;
        let target_id = created.get("targetId").and_then(Value::as_str).ok_or_else(|| AppError::Internal("Chrome não criou a aba".into()))?.to_string();
        let attached = engine.conn.call("Target.attachToTarget", json!({ "targetId": target_id, "flatten": true }), CALL)?;
        let session_id = attached.get("sessionId").and_then(Value::as_str).ok_or_else(|| AppError::Internal("Chrome não anexou a aba".into()))?.to_string();
        let (w, h) = clamp_viewport(width, height);
        let tab = Arc::new(Tab {
            id: uuid::Uuid::new_v4().to_string(),
            workspace_id: workspace_id.to_string(),
            target_id,
            session_id,
            created_at: now_ms(),
            state: Mutex::new(TabState {
                url: "about:blank".into(),
                title: String::new(),
                loading: false,
                width: w,
                height: h,
                mobile,
                entries: VecDeque::new(),
                requests: HashMap::new(),
                loads: 0,
                subscriber: None,
                screencast: false,
            }),
        });
        self.tabs.lock().unwrap().insert(tab.id.clone(), tab.clone());
        let setup = (|| -> AppResult<()> {
            for domain in ["Page.enable", "Runtime.enable", "Log.enable", "Network.enable"] {
                self.send(&tab, domain, json!({}))?;
            }
            self.apply_viewport(&tab, w, h, mobile)?;
            if let Some(u) = &target {
                self.navigate_tab(&tab, u)?;
            }
            Ok(())
        })();
        if let Err(e) = setup {
            let _ = self.close(&tab.id);
            return Err(e);
        }
        Ok(tab.info())
    }

    pub fn list(&self, workspace_id: Option<&str>) -> Vec<TabInfo> {
        let tabs: Vec<Arc<Tab>> = self.tabs.lock().unwrap().values().cloned().collect();
        let mut out: Vec<TabInfo> = tabs.iter().filter(|t| workspace_id.is_none_or(|w| t.workspace_id == w)).map(|t| t.info()).collect();
        out.sort_by_key(|t| t.created_at);
        out
    }

    pub fn attach(&self, id: &str, subscriber: Subscriber) -> AppResult<AttachSnapshot> {
        let tab = self.tab(id)?;
        let token = self.tokens.fetch_add(1, Ordering::SeqCst);
        let entries = {
            let mut s = tab.state.lock().unwrap();
            s.subscriber = Some((token, subscriber));
            s.entries.iter().cloned().collect()
        };
        self.start_screencast(&tab)?;
        Ok(AttachSnapshot { info: tab.info(), entries, token })
    }

    pub fn detach(&self, id: &str, token: u64) -> AppResult<()> {
        let tab = self.tab(id)?;
        let stop = {
            let mut s = tab.state.lock().unwrap();
            if s.subscriber.as_ref().map(|(t, _)| *t) == Some(token) {
                s.subscriber = None;
                std::mem::replace(&mut s.screencast, false)
            } else {
                false
            }
        };
        // Nobody is watching: stop streaming frames to save CPU.
        if stop {
            let _ = self.send(&tab, "Page.stopScreencast", json!({}));
        }
        Ok(())
    }

    fn start_screencast(&self, tab: &Tab) -> AppResult<()> {
        let (w, h) = {
            let mut s = tab.state.lock().unwrap();
            s.screencast = true;
            (s.width, s.height)
        };
        let _ = self.send(tab, "Page.stopScreencast", json!({}));
        let params = json!({ "format": "jpeg", "quality": 75, "maxWidth": w, "maxHeight": h, "everyNthFrame": 1 });
        // During a cross-process navigation commit Chrome briefly reports
        // "Not attached to an active page": retry for a short while.
        let mut last = None;
        for _ in 0..30 {
            match self.send(tab, "Page.startScreencast", params.clone()) {
                Ok(_) => return Ok(()),
                Err(AppError::Invalid(m)) if m.contains("Not attached") => {
                    last = Some(AppError::Invalid(m));
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => return Err(e),
            }
        }
        Err(last.unwrap_or_else(|| AppError::Internal("screencast indisponível".into())))
    }

    fn navigate_tab(&self, tab: &Tab, url: &str) -> AppResult<()> {
        {
            let mut s = tab.state.lock().unwrap();
            s.loading = true;
            s.url = url.to_string();
        }
        tab.emit_nav();
        let r = self.send(tab, "Page.navigate", json!({ "url": url }))?;
        if let Some(err) = r.get("errorText").and_then(Value::as_str).filter(|e| !e.is_empty()) {
            tab.push_entry("error", "network", format!("Não foi possível abrir a página: {err}"), Some(url.to_string()));
            tab.state.lock().unwrap().loading = false;
            tab.emit_nav();
        }
        Ok(())
    }

    pub fn navigate(&self, id: &str, url: &str) -> AppResult<String> {
        let url = normalize_url(url)?;
        let tab = self.tab(id)?;
        self.navigate_tab(&tab, &url)?;
        Ok(url)
    }

    pub fn nav_state(&self, id: &str) -> AppResult<NavState> {
        let tab = self.tab(id)?;
        let h = self.send(&tab, "Page.getNavigationHistory", json!({}))?;
        let idx = h.get("currentIndex").and_then(Value::as_i64).unwrap_or(0);
        let n = h.get("entries").and_then(Value::as_array).map(|a| a.len() as i64).unwrap_or(0);
        Ok(NavState { can_go_back: idx > 0, can_go_forward: idx + 1 < n })
    }

    pub fn go(&self, id: &str, delta: i64) -> AppResult<()> {
        let tab = self.tab(id)?;
        let h = self.send(&tab, "Page.getNavigationHistory", json!({}))?;
        let idx = h.get("currentIndex").and_then(Value::as_i64).unwrap_or(0) + delta;
        let entry = h
            .get("entries")
            .and_then(Value::as_array)
            .and_then(|a| usize::try_from(idx).ok().and_then(|i| a.get(i)))
            .and_then(|e| e.get("id"))
            .cloned()
            .ok_or_else(|| AppError::Invalid("não há página nessa direção do histórico".into()))?;
        self.send(&tab, "Page.navigateToHistoryEntry", json!({ "entryId": entry }))?;
        Ok(())
    }

    pub fn reload(&self, id: &str, ignore_cache: bool) -> AppResult<()> {
        let tab = self.tab(id)?;
        tab.state.lock().unwrap().loading = true;
        tab.emit_nav();
        self.send(&tab, "Page.reload", json!({ "ignoreCache": ignore_cache }))?;
        Ok(())
    }

    fn apply_viewport(&self, tab: &Tab, w: u32, h: u32, mobile: bool) -> AppResult<()> {
        self.send(tab, "Emulation.setDeviceMetricsOverride", json!({ "width": w, "height": h, "deviceScaleFactor": 1, "mobile": mobile }))?;
        self.send(tab, "Emulation.setTouchEmulationEnabled", json!({ "enabled": mobile }))?;
        Ok(())
    }

    pub fn set_viewport(&self, id: &str, width: u32, height: u32, mobile: bool) -> AppResult<TabInfo> {
        let tab = self.tab(id)?;
        let (w, h) = clamp_viewport(width, height);
        let streaming = {
            let mut s = tab.state.lock().unwrap();
            s.width = w;
            s.height = h;
            s.mobile = mobile;
            s.screencast
        };
        self.apply_viewport(&tab, w, h, mobile)?;
        if streaming {
            self.start_screencast(&tab)?;
        }
        Ok(tab.info())
    }

    pub fn mouse(&self, id: &str, m: &MouseInput) -> AppResult<()> {
        let tab = self.tab(id)?;
        if !matches!(m.kind.as_str(), "mousePressed" | "mouseReleased" | "mouseMoved" | "mouseWheel") {
            return Err(AppError::Invalid("evento de mouse inválido".into()));
        }
        let mut p = json!({
            "type": m.kind, "x": m.x, "y": m.y,
            "modifiers": m.modifiers.unwrap_or(0),
            "button": m.button.as_deref().unwrap_or("none"),
            "clickCount": m.click_count.unwrap_or(0),
        });
        if m.kind == "mouseWheel" {
            p["deltaX"] = json!(m.delta_x.unwrap_or(0.0));
            p["deltaY"] = json!(m.delta_y.unwrap_or(0.0));
        }
        self.notify(&tab, "Input.dispatchMouseEvent", p);
        Ok(())
    }

    pub fn key(&self, id: &str, k: &KeyInput) -> AppResult<()> {
        let tab = self.tab(id)?;
        if !matches!(k.kind.as_str(), "keyDown" | "keyUp" | "rawKeyDown" | "char") {
            return Err(AppError::Invalid("evento de teclado inválido".into()));
        }
        let mut p = json!({ "type": k.kind, "key": k.key, "modifiers": k.modifiers.unwrap_or(0) });
        if let Some(c) = &k.code {
            p["code"] = json!(c);
        }
        if let Some(t) = &k.text {
            p["text"] = json!(t);
            p["unmodifiedText"] = json!(t);
        }
        if let Some(kc) = k.key_code {
            p["windowsVirtualKeyCode"] = json!(kc);
            p["nativeVirtualKeyCode"] = json!(kc);
        }
        self.notify(&tab, "Input.dispatchKeyEvent", p);
        Ok(())
    }

    pub fn insert_text(&self, id: &str, text: &str) -> AppResult<()> {
        if text.len() > 64 * 1024 {
            return Err(AppError::Invalid("texto grande demais".into()));
        }
        let tab = self.tab(id)?;
        self.send(&tab, "Input.insertText", json!({ "text": text }))?;
        Ok(())
    }

    pub fn entries(&self, id: &str) -> AppResult<Vec<ConsoleEntry>> {
        Ok(self.tab(id)?.state.lock().unwrap().entries.iter().cloned().collect())
    }

    pub fn clear_entries(&self, id: &str) -> AppResult<()> {
        self.tab(id)?.state.lock().unwrap().entries.clear();
        Ok(())
    }

    pub fn screenshot(&self, id: &str, full_page: bool) -> AppResult<Shot> {
        let tab = self.tab(id)?;
        let (vw, vh) = {
            let s = tab.state.lock().unwrap();
            (s.width, s.height)
        };
        let mut params = json!({ "format": "png", "captureBeyondViewport": full_page });
        let (mut w, mut h) = (vw, vh);
        if full_page {
            let m = self.send(&tab, "Page.getLayoutMetrics", json!({}))?;
            let cw = m.pointer("/cssContentSize/width").and_then(Value::as_f64).unwrap_or(vw as f64).ceil();
            let ch = m.pointer("/cssContentSize/height").and_then(Value::as_f64).unwrap_or(vh as f64).ceil().min(16_000.0);
            params["clip"] = json!({ "x": 0, "y": 0, "width": cw, "height": ch, "scale": 1 });
            (w, h) = (cw as u32, ch as u32);
        }
        let r = self.send(&tab, "Page.captureScreenshot", params)?;
        let data = r.get("data").and_then(Value::as_str).ok_or_else(|| AppError::Internal("captura vazia".into()))?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| AppError::Internal(format!("captura inválida: {e}")))?;
        let dir = self.shots_dir();
        std::fs::create_dir_all(&dir)?;
        let name = format!("{}-{}.png", now_ms(), &tab.id[..8]);
        let path = dir.join(name);
        std::fs::write(&path, &bytes)?;
        Ok(Shot { path: path.display().to_string(), width: w, height: h, bytes: bytes.len() as u64 })
    }

    /// Reloads the page, waits for load + a settle period, and reports every
    /// error/warning observed since the reload, plus a screenshot.
    pub fn verify(&self, id: &str, settle: Duration) -> AppResult<VerifyReport> {
        let tab = self.tab(id)?;
        let started = Instant::now();
        let mark = tab.last_seq();
        let loads_before = tab.state.lock().unwrap().loads;
        self.reload(id, true)?;
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut loaded = false;
        while Instant::now() < deadline {
            if tab.state.lock().unwrap().loads > loads_before {
                loaded = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        std::thread::sleep(settle);
        let screenshot = self.screenshot(id, false).ok();
        let (url, title, errors, warnings) = {
            let s = tab.state.lock().unwrap();
            let new: Vec<ConsoleEntry> = s
                .entries
                .iter()
                .filter(|e| e.seq > mark)
                .map(|e| ConsoleEntry { text: redact_secrets(&e.text), ..e.clone() })
                .collect();
            let errors: Vec<_> = new.iter().filter(|e| e.level == "error").cloned().collect();
            let warnings: Vec<_> = new.iter().filter(|e| e.level == "warning").cloned().collect();
            (s.url.clone(), s.title.clone(), errors, warnings)
        };
        let ok = loaded && errors.is_empty();
        let elapsed_ms = started.elapsed().as_millis() as u64;
        let summary = report_markdown(&url, &title, loaded, &errors, &warnings, screenshot.as_ref());
        Ok(VerifyReport { url, title, ok, loaded, elapsed_ms, errors, warnings, screenshot, summary })
    }

    pub fn close(&self, id: &str) -> AppResult<()> {
        let tab = self.tabs.lock().unwrap().remove(id);
        let Some(tab) = tab else { return Ok(()) };
        if let Some(engine) = self.engine.lock().unwrap().clone() {
            let _ = engine.conn.call("Target.closeTarget", json!({ "targetId": tab.target_id }), Duration::from_secs(3));
        }
        tab.emit(BrowserEvent::Closed { reason: "closed".into() });
        if self.tabs.lock().unwrap().is_empty() {
            self.shutdown();
        }
        Ok(())
    }

    pub fn close_workspace(&self, workspace_id: &str) {
        let ids: Vec<String> = self.tabs.lock().unwrap().values().filter(|t| t.workspace_id == workspace_id).map(|t| t.id.clone()).collect();
        for id in ids {
            let _ = self.close(&id);
        }
    }

    /// Stops Chrome (no-op when not running).
    pub fn shutdown(&self) {
        if let Some(e) = self.engine.lock().unwrap().take() {
            e.shutdown();
        }
    }

    pub fn kill_all(&self) {
        let tabs: Vec<Arc<Tab>> = self.tabs.lock().unwrap().drain().map(|(_, t)| t).collect();
        for t in tabs {
            t.emit(BrowserEvent::Closed { reason: "shutdown".into() });
        }
        self.shutdown();
    }
}

impl Drop for BrowserManager {
    fn drop(&mut self) {
        self.kill_all();
    }
}

pub fn clamp_viewport(w: u32, h: u32) -> (u32, u32) {
    (w.clamp(240, 3840), h.clamp(240, 2400))
}

fn report_markdown(url: &str, title: &str, loaded: bool, errors: &[ConsoleEntry], warnings: &[ConsoleEntry], shot: Option<&Shot>) -> String {
    let mut s = String::new();
    s.push_str(&format!("## Verificação de página\n\n- URL: {url}\n"));
    if !title.is_empty() {
        s.push_str(&format!("- Título: {}\n", redact_secrets(title)));
    }
    s.push_str(&format!("- Carregou: {}\n", if loaded { "sim" } else { "não (tempo esgotado)" }));
    s.push_str(&format!("- Erros: {} · Avisos: {}\n", errors.len(), warnings.len()));
    if let Some(sh) = shot {
        s.push_str(&format!("- Captura: {}\n", sh.path));
    }
    let list = |s: &mut String, head: &str, items: &[ConsoleEntry]| {
        if items.is_empty() {
            return;
        }
        s.push_str(&format!("\n### {head}\n\n"));
        for e in items.iter().take(50) {
            let text = e.text.lines().take(6).collect::<Vec<_>>().join("\n  ");
            match &e.url {
                Some(u) => s.push_str(&format!("- [{}] {text} — {u}\n", e.source)),
                None => s.push_str(&format!("- [{}] {text}\n", e.source)),
            }
        }
        if items.len() > 50 {
            s.push_str(&format!("- … e mais {}\n", items.len() - 50));
        }
    };
    list(&mut s, "Erros", errors);
    list(&mut s, "Avisos", warnings);
    if errors.is_empty() && loaded {
        s.push_str("\nNenhum erro de console, exceção ou falha de rede observado.\n");
    }
    s
}

fn make_handler(tabs: Tabs, conn: Arc<OnceLock<Weak<CdpConn>>>) -> EventHandler {
    Arc::new(move |method: &str, p: &Value, session: Option<&str>| {
        if method == "Forge.disconnected" {
            let all: Vec<Arc<Tab>> = tabs.lock().unwrap().drain().map(|(_, t)| t).collect();
            for t in all {
                t.emit(BrowserEvent::Closed { reason: "browser-exited".into() });
            }
            return;
        }
        // Browser-level target events.
        if session.is_none() {
            let target_id = p.pointer("/targetInfo/targetId").or_else(|| p.get("targetId")).and_then(Value::as_str);
            let Some(target_id) = target_id else { return };
            let tab = tabs.lock().unwrap().values().find(|t| t.target_id == target_id).cloned();
            let Some(tab) = tab else { return };
            match method {
                "Target.targetInfoChanged" => {
                    {
                        let mut s = tab.state.lock().unwrap();
                        if let Some(u) = p.pointer("/targetInfo/url").and_then(Value::as_str) {
                            s.url = u.to_string();
                        }
                        if let Some(t) = p.pointer("/targetInfo/title").and_then(Value::as_str) {
                            // Chrome uses the URL as title until <title> loads.
                            s.title = if t == s.url { String::new() } else { t.to_string() };
                        }
                    }
                    tab.emit_nav();
                }
                "Target.targetDestroyed" | "Target.targetCrashed" => {
                    tabs.lock().unwrap().remove(&tab.id);
                    let reason = if method == "Target.targetCrashed" { "crashed" } else { "closed" };
                    tab.emit(BrowserEvent::Closed { reason: reason.into() });
                }
                _ => {}
            }
            return;
        }
        let session = session.unwrap_or_default();
        let tab = tabs.lock().unwrap().values().find(|t| t.session_id == session).cloned();
        let Some(tab) = tab else { return };
        match method {
            "Page.screencastFrame" => {
                if let (Some(c), Some(ack)) = (conn.get().and_then(Weak::upgrade), p.get("sessionId")) {
                    c.notify_in(Some(session), "Page.screencastFrameAck", json!({ "sessionId": ack }));
                }
                let data = p.get("data").and_then(Value::as_str).unwrap_or_default().to_string();
                let width = p.pointer("/metadata/deviceWidth").and_then(Value::as_f64).unwrap_or(0.0) as u32;
                let height = p.pointer("/metadata/deviceHeight").and_then(Value::as_f64).unwrap_or(0.0) as u32;
                tab.emit(BrowserEvent::Frame { data, width, height });
            }
            "Page.frameStartedLoading" | "Page.frameStoppedLoading" => {
                if p.get("frameId").and_then(Value::as_str) == Some(tab.target_id.as_str()) {
                    tab.state.lock().unwrap().loading = method == "Page.frameStartedLoading";
                    tab.emit_nav();
                }
            }
            "Page.loadEventFired" => {
                {
                    let mut s = tab.state.lock().unwrap();
                    s.loads += 1;
                    s.loading = false;
                }
                tab.emit_nav();
            }
            "Page.frameNavigated" => {
                let main = p.pointer("/frame/parentId").is_none();
                if let (true, Some(u)) = (main, p.pointer("/frame/url").and_then(Value::as_str)) {
                    let mut s = tab.state.lock().unwrap();
                    s.url = u.to_string();
                    s.requests.clear();
                }
            }
            "Network.requestWillBeSent" => {
                if let (Some(id), Some(u)) = (p.get("requestId").and_then(Value::as_str), p.pointer("/request/url").and_then(Value::as_str)) {
                    let mut s = tab.state.lock().unwrap();
                    if s.requests.len() >= MAX_TRACKED_REQUESTS {
                        s.requests.clear();
                    }
                    // data: URLs can be huge and are never useful in a report.
                    let u = if u.starts_with("data:") { "data:…" } else { u };
                    s.requests.insert(id.to_string(), u.to_string());
                }
            }
            _ => {
                let raw = {
                    let s = tab.state.lock().unwrap();
                    entry_from_event(method, p, |rid| s.requests.get(rid).cloned())
                };
                if let Some(r) = raw {
                    tab.push_entry(r.level, r.source, r.text, r.url);
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    #[test]
    fn viewport_is_clamped() {
        assert_eq!(clamp_viewport(10, 99_999), (240, 2400));
        assert_eq!(clamp_viewport(1280, 800), (1280, 800));
    }

    #[test]
    fn report_lists_errors_and_redacts_title() {
        let e = ConsoleEntry { seq: 1, ts: 0, level: "error".into(), source: "network".into(), text: "HTTP 500".into(), url: Some("http://l/api".into()) };
        let md = report_markdown("http://l/", "Home", true, &[e], &[], None);
        assert!(md.contains("Erros: 1"));
        assert!(md.contains("- [network] HTTP 500 — http://l/api"));
        assert!(!md.contains("Nenhum erro"));
        let ok = report_markdown("http://l/", "", true, &[], &[], None);
        assert!(ok.contains("Nenhum erro"));
    }

    #[test]
    fn missing_browser_is_reported_not_faked() {
        let dir = tempfile::tempdir().unwrap();
        let m = BrowserManager::new(dir.path().into()).with_chrome(dir.path().join("no-such-chrome"));
        let err = m.open("ws", Some("localhost:1"), 800, 600, false).unwrap_err();
        assert!(matches!(err, AppError::Internal(_)), "{err:?}");
        assert!(!m.status().running);
    }

    fn chrome_or_skip() -> Option<PathBuf> {
        let c = chrome::find();
        if c.is_none() {
            eprintln!("skipping: no Chrome installed");
        }
        c
    }

    /// Tiny HTTP server: `/` = page with a console error, an exception and a
    /// 404 fetch; `/missing.js` = 404.
    fn serve() -> (String, std::thread::JoinHandle<()>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = format!("http://127.0.0.1:{}/", l.local_addr().unwrap().port());
        let h = std::thread::spawn(move || {
            for stream in l.incoming().take(40) {
                let Ok(mut s) = stream else { continue };
                let mut first = String::new();
                let mut r = BufReader::new(s.try_clone().unwrap());
                let _ = r.read_line(&mut first);
                loop {
                    let mut line = String::new();
                    if r.read_line(&mut line).unwrap_or(0) <= 2 {
                        break;
                    }
                }
                let path = first.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (status, body) = if path == "/" {
                    ("200 OK", "<!doctype html><title>Forge Test</title><body><h1>hello</h1><input id=q><script>console.error('forge-console-error');fetch('/missing.json');setTimeout(()=>{throw new Error('forge-exception')},10)</script></body>")
                } else {
                    ("404 Not Found", "nope")
                };
                let _ = write!(s, "HTTP/1.1 {status}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        (addr, h)
    }

    #[test]
    #[ignore = "launches the local Chrome"]
    fn live_open_capture_verify_and_input() {
        let Some(bin) = chrome_or_skip() else { return };
        let dir = tempfile::tempdir().unwrap();
        let m = BrowserManager::new(dir.path().into()).with_chrome(bin);
        let (url, _srv) = serve();
        let tab = m.open("ws1", Some(&url), 1024, 700, false).unwrap();
        let frames = Arc::new(AtomicU64::new(0));
        let f2 = frames.clone();
        let snap = m
            .attach(&tab.id, Arc::new(move |ev| {
                if let BrowserEvent::Frame { width, .. } = ev {
                    assert!(width > 0);
                    f2.fetch_add(1, Ordering::SeqCst);
                }
            }))
            .unwrap();
        let st = m.status();
        assert!(st.running && st.version.as_deref().unwrap_or("").contains("Chrome"), "{st:?}");
        let report = m.verify(&tab.id, Duration::from_millis(800)).unwrap();
        eprintln!("{}", report.summary);
        assert!(report.loaded);
        assert_eq!(report.title, "Forge Test");
        assert!(!report.ok);
        let texts: Vec<&str> = report.errors.iter().map(|e| e.text.as_str()).collect();
        assert!(texts.iter().any(|t| t.contains("forge-console-error")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("forge-exception")), "{texts:?}");
        assert!(report.errors.iter().any(|e| e.source == "network" && e.text.contains("404")), "{texts:?}");
        let shot = report.screenshot.unwrap();
        assert!(std::path::Path::new(&shot.path).is_file() && shot.bytes > 1000);
        assert!(frames.load(Ordering::SeqCst) > 0, "no screencast frames");
        // Input reaches the page: click the input and type.
        m.mouse(&tab.id, &MouseInput { kind: "mousePressed".into(), x: 20.0, y: 80.0, button: Some("left".into()), click_count: Some(1), delta_x: None, delta_y: None, modifiers: None }).unwrap();
        m.mouse(&tab.id, &MouseInput { kind: "mouseReleased".into(), x: 20.0, y: 80.0, button: Some("left".into()), click_count: Some(1), delta_x: None, delta_y: None, modifiers: None }).unwrap();
        m.send(&m.tab(&tab.id).unwrap(), "Runtime.evaluate", json!({"expression":"document.getElementById('q').focus()"})).unwrap();
        m.insert_text(&tab.id, "forge").unwrap();
        let v = m.send(&m.tab(&tab.id).unwrap(), "Runtime.evaluate", json!({"expression":"document.getElementById('q').value","returnByValue":true})).unwrap();
        assert_eq!(v.pointer("/result/value").and_then(Value::as_str), Some("forge"));
        // Viewport + full-page screenshot + history.
        let info = m.set_viewport(&tab.id, 390, 844, true).unwrap();
        assert_eq!((info.width, info.height, info.mobile), (390, 844, true));
        let full = m.screenshot(&tab.id, true).unwrap();
        assert!(full.bytes > 500);
        m.navigate(&tab.id, &format!("{url}other")).unwrap();
        std::thread::sleep(Duration::from_millis(800));
        assert!(m.nav_state(&tab.id).unwrap().can_go_back);
        m.go(&tab.id, -1).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        assert!(m.nav_state(&tab.id).unwrap().can_go_forward);
        assert!(m.navigate(&tab.id, "file:///etc/passwd").is_err());
        m.detach(&tab.id, snap.token).unwrap();
        m.close(&tab.id).unwrap();
        assert!(!m.status().running, "Chrome must stop with the last tab");
    }
}
