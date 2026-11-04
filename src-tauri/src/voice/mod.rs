//! Voice (M15): macOS speech recognition through the native `forge-voice`
//! helper (Swift, Speech framework) and spoken replies through `say`.
//!
//! Audio is never written to disk by FORGE (the self-test synthesizes a
//! sample phrase into a temp file and deletes it). Transcripts are not logged.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, AppResult};

pub const SELF_TEST_PHRASE_PT: &str = "Forge, crie uma missão para implementar pagamentos";
pub const SELF_TEST_PHRASE_EN: &str = "Forge, create a mission to implement payments";
const MAX_SPEAK_CHARS: usize = 1200;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum VoiceEvent {
    Listening {
        locale: String,
        #[serde(rename = "onDevice")]
        on_device: bool,
    },
    Partial {
        text: String,
    },
    Final {
        text: String,
    },
    Level {
        rms: f64,
    },
    Ended {
        reason: String,
    },
    Error {
        code: String,
        message: String,
    },
    /// The helper process ended (always the last event of a session).
    Exit {
        code: Option<i32>,
    },
}

pub type Subscriber = Arc<dyn Fn(VoiceEvent) + Send + Sync>;

/// Parses one JSON line from the helper (unknown lines are ignored).
pub fn parse_line(line: &str) -> Option<VoiceEvent> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    if v.get("type")?.as_str()? == "exit" {
        return None;
    }
    serde_json::from_value(v).ok()
}

pub fn valid_locale(id: &str) -> bool {
    (2..=16).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') && id.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
}

/// Words of a phrase, lowercased, without accents or punctuation.
pub fn words(s: &str) -> Vec<String> {
    let folded: String = s
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        })
        .collect();
    folded.split_whitespace().map(String::from).collect()
}

/// Share of expected words present in the transcript (0..=1).
pub fn word_recall(expected: &str, got: &str) -> f64 {
    let e = words(expected);
    if e.is_empty() {
        return 0.0;
    }
    let g = words(got);
    e.iter().filter(|w| g.contains(w)).count() as f64 / e.len() as f64
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStatus {
    pub supported: bool,
    pub helper: Option<String>,
    /// authorized | denied | restricted | notDetermined | unknown
    pub speech: String,
    pub microphone: String,
    pub listening: bool,
    pub tts: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceLocale {
    pub id: String,
    pub on_device: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfTest {
    pub phrase: String,
    pub transcript: String,
    pub recall: f64,
    pub ok: bool,
    pub elapsed_ms: u64,
}

struct Session {
    token: u64,
    child: Arc<Mutex<Child>>,
    stdin: Option<ChildStdin>,
}

#[derive(Default)]
pub struct VoiceManager {
    helper_override: Option<PathBuf>,
    session: Arc<Mutex<Option<Session>>>,
    speaking: Mutex<Option<Child>>,
    tokens: AtomicU64,
}

impl VoiceManager {
    pub fn with_helper(path: PathBuf) -> Self {
        Self { helper_override: Some(path), ..Default::default() }
    }

    /// The bundled sidecar (next to the app binary), or the build output in dev/tests.
    pub fn helper(&self) -> Option<PathBuf> {
        if let Some(p) = &self.helper_override {
            return p.is_file().then(|| p.clone());
        }
        if let Some(p) = std::env::var_os("FORGE_VOICE_HELPER").map(PathBuf::from) {
            if p.is_file() {
                return Some(p);
            }
        }
        if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
            let p = dir.join("forge-voice");
            if p.is_file() {
                return Some(p);
            }
        }
        let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries").join(format!("forge-voice-{}", env!("FORGE_TARGET")));
        dev.is_file().then_some(dev)
    }

    fn require_helper(&self) -> AppResult<PathBuf> {
        if !cfg!(target_os = "macos") {
            return Err(AppError::Invalid("a voz do FORGE usa o reconhecimento de fala do macOS e ainda não está disponível neste sistema".into()));
        }
        self.helper().ok_or_else(|| AppError::NotFound("helper de voz (forge-voice) não encontrado no app".into()))
    }

    fn run_once(&self, args: &[&str], timeout: Duration) -> AppResult<Vec<VoiceEvent>> {
        let bin = self.require_helper()?;
        let mut child = Command::new(&bin).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
        let stdout = child.stdout.take().ok_or_else(|| AppError::Internal("sem stdout".into()))?;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut out = Vec::new();
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                out.push(line);
            }
            let _ = tx.send(out);
        });
        let lines = match rx.recv_timeout(timeout) {
            Ok(l) => l,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::Internal("o helper de voz não respondeu a tempo".into()));
            }
        };
        let _ = child.wait();
        Ok(lines.iter().filter_map(|l| parse_line(l)).collect())
    }

    fn raw_once(&self, args: &[&str], timeout: Duration) -> AppResult<Value> {
        let bin = self.require_helper()?;
        let out = Command::new(&bin).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output()?;
        let _ = timeout;
        let line = String::from_utf8_lossy(&out.stdout).lines().last().unwrap_or_default().to_string();
        serde_json::from_str(&line).map_err(|_| AppError::Internal("resposta inválida do helper de voz".into()))
    }

    pub fn status(&self) -> VoiceStatus {
        let helper = if cfg!(target_os = "macos") { self.helper() } else { None };
        let (speech, microphone) = match helper.as_ref().map(|_| self.raw_once(&["status"], Duration::from_secs(10))) {
            Some(Ok(v)) => (
                v.get("speech").and_then(Value::as_str).unwrap_or("unknown").to_string(),
                v.get("microphone").and_then(Value::as_str).unwrap_or("unknown").to_string(),
            ),
            _ => ("unknown".into(), "unknown".into()),
        };
        VoiceStatus {
            supported: cfg!(target_os = "macos"),
            helper: helper.map(|p| p.display().to_string()),
            speech,
            microphone,
            listening: self.session.lock().unwrap().is_some(),
            tts: Path::new("/usr/bin/say").is_file(),
        }
    }

    /// Asks macOS for microphone + speech permission (shows the system dialogs once).
    pub fn authorize(&self) -> AppResult<VoiceStatus> {
        let bin = self.require_helper()?;
        let mut child = Command::new(&bin).arg("authorize").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
        let deadline = Instant::now() + Duration::from_secs(180);
        while Instant::now() < deadline {
            if child.try_wait()?.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        let _ = child.kill();
        let _ = child.wait();
        Ok(self.status())
    }

    pub fn locales(&self) -> AppResult<Vec<VoiceLocale>> {
        let v = self.raw_once(&["locales"], Duration::from_secs(20))?;
        let list = v.get("locales").cloned().unwrap_or(Value::Array(vec![]));
        Ok(serde_json::from_value(list).unwrap_or_default())
    }

    pub fn transcribe_file(&self, path: &Path, locale: &str, on_device: bool) -> AppResult<String> {
        if !valid_locale(locale) {
            return Err(AppError::Invalid(format!("idioma inválido: {locale}")));
        }
        let p = path.display().to_string();
        let mut args = vec!["file", p.as_str(), "--locale", locale];
        if on_device {
            args.push("--on-device");
        }
        let events = self.run_once(&args, Duration::from_secs(60))?;
        for e in events {
            match e {
                VoiceEvent::Final { text } => return Ok(text),
                VoiceEvent::Error { message, .. } => return Err(AppError::Invalid(message)),
                _ => {}
            }
        }
        Err(AppError::Internal("o helper de voz terminou sem transcrição".into()))
    }

    /// Synthesizes a known phrase with `say`, transcribes it and compares.
    pub fn self_test(&self, locale: &str, on_device: bool, scratch: &Path) -> AppResult<SelfTest> {
        let started = Instant::now();
        let pt = locale.to_lowercase().starts_with("pt");
        let phrase = if pt { SELF_TEST_PHRASE_PT } else { SELF_TEST_PHRASE_EN };
        std::fs::create_dir_all(scratch)?;
        let file = scratch.join(format!("voice-self-test-{}.aiff", uuid::Uuid::new_v4()));
        let mut cmd = Command::new("/usr/bin/say");
        if pt {
            cmd.args(["-v", "Luciana"]);
        }
        let st = cmd.arg("-o").arg(&file).arg(phrase).stdout(Stdio::null()).stderr(Stdio::null()).status()?;
        if !st.success() || !file.is_file() {
            return Err(AppError::Internal("não foi possível sintetizar a frase de teste com o say".into()));
        }
        let res = self.transcribe_file(&file, locale, on_device);
        let _ = std::fs::remove_file(&file);
        let transcript = res?;
        let recall = word_recall(phrase, &transcript);
        Ok(SelfTest { phrase: phrase.into(), transcript, recall, ok: recall >= 0.7, elapsed_ms: started.elapsed().as_millis() as u64 })
    }

    /// Starts listening (one session at a time). Events go to `subscriber`;
    /// the session ends by itself on silence, or via `stop`/`cancel`.
    pub fn listen(&self, locale: &str, on_device: bool, silence_secs: f64, subscriber: Subscriber) -> AppResult<u64> {
        if !valid_locale(locale) {
            return Err(AppError::Invalid(format!("idioma inválido: {locale}")));
        }
        let bin = self.require_helper()?;
        let mut guard = self.session.lock().unwrap();
        if guard.is_some() {
            return Err(AppError::Conflict("já existe uma escuta em andamento".into()));
        }
        let silence = format!("{:.1}", silence_secs.clamp(0.8, 5.0));
        let mut args = vec!["listen", "--locale", locale, "--silence", silence.as_str(), "--max", "90"];
        if on_device {
            args.push("--on-device");
        }
        let mut child = Command::new(&bin).args(&args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
        let stdout = child.stdout.take().ok_or_else(|| AppError::Internal("sem stdout".into()))?;
        let stdin = child.stdin.take();
        let token = self.tokens.fetch_add(1, Ordering::SeqCst) + 1;
        let child = Arc::new(Mutex::new(child));
        *guard = Some(Session { token, child: child.clone(), stdin });
        drop(guard);
        let session = self.session.clone();
        std::thread::Builder::new().name("forge-voice".into()).spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(ev) = parse_line(&line) {
                    subscriber(ev);
                }
            }
            let code = child.lock().unwrap().wait().ok().and_then(|s| s.code());
            {
                let mut g = session.lock().unwrap();
                if g.as_ref().is_some_and(|s| s.token == token) {
                    *g = None;
                }
            }
            subscriber(VoiceEvent::Exit { code });
        })?;
        Ok(token)
    }

    /// Stops capturing; the final transcript still arrives.
    pub fn stop(&self) {
        if let Some(s) = self.session.lock().unwrap().as_mut() {
            if let Some(mut i) = s.stdin.take() {
                let _ = i.write_all(b"stop\n");
            }
        }
    }

    /// Aborts the session without waiting for a transcript.
    pub fn cancel(&self) {
        if let Some(s) = self.session.lock().unwrap().take() {
            let _ = s.child.lock().unwrap().kill();
        }
    }

    pub fn speak(&self, text: &str) -> AppResult<()> {
        self.stop_speaking();
        let text: String = text.chars().filter(|c| !c.is_control() || *c == '\n').take(MAX_SPEAK_CHARS).collect();
        if text.trim().is_empty() {
            return Ok(());
        }
        // Text goes through stdin, never as an argument.
        let mut child = Command::new("/usr/bin/say").stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
        if let Some(mut i) = child.stdin.take() {
            let _ = i.write_all(text.as_bytes());
        }
        *self.speaking.lock().unwrap() = Some(child);
        Ok(())
    }

    pub fn stop_speaking(&self) {
        if let Some(mut c) = self.speaking.lock().unwrap().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    pub fn shutdown(&self) {
        self.cancel();
        self.stop_speaking();
    }
}

// ── "Pause all agents" (Jarvis) ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PauseTarget {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PausePlan {
    /// Running missions → paused (in-flight tasks finish, no new ones start).
    pub missions: Vec<PauseTarget>,
    /// Running standalone agent runs (not owned by a mission) → stopped.
    pub runs: Vec<PauseTarget>,
}

pub fn pause_plan(conn: &rusqlite::Connection) -> AppResult<PausePlan> {
    let mut plan = PausePlan::default();
    let mut st = conn.prepare("SELECT id, title FROM missions WHERE status = 'running' ORDER BY updated_at DESC")?;
    plan.missions = st.query_map([], |r| Ok(PauseTarget { id: r.get(0)?, label: r.get(1)? }))?.collect::<Result<_, _>>()?;
    let mut st = conn.prepare(
        "SELECT id, prompt FROM agent_runs WHERE status = 'running' AND id NOT IN (SELECT run_id FROM mission_runs) ORDER BY started_at DESC",
    )?;
    plan.runs = st
        .query_map([], |r| {
            let prompt: String = r.get(1)?;
            Ok(PauseTarget { id: r.get(0)?, label: prompt.chars().take(80).collect() })
        })?
        .collect::<Result<_, _>>()?;
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_plan_separates_missions_and_standalone_runs() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let plan = db
            .with(|c| {
                c.execute_batch(
                    "INSERT INTO workspaces (id, name, path, created_at) VALUES ('w','w','/tmp/w',0);
                     INSERT INTO missions (id, workspace_id, title, briefing, mode, status, created_at, updated_at) VALUES
                       ('m1','w','Pagamentos','b','free','running',0,1), ('m2','w','Parada','b','free','paused',0,0);
                     INSERT INTO agent_runs (id, workspace_id, provider_id, permission, prompt, status, cwd, started_at) VALUES
                       ('r1','w','claude','read','avulsa','running','/tmp',2),
                       ('r2','w','claude','read','da missão','running','/tmp',1),
                       ('r3','w','claude','read','terminada','completed','/tmp',0);
                     INSERT INTO mission_runs (mission_id, task_id, run_id, kind, created_at) VALUES ('m1', NULL, 'r2', 'task', 0);",
                )?;
                pause_plan(c)
            })
            .unwrap();
        assert_eq!(plan.missions, vec![PauseTarget { id: "m1".into(), label: "Pagamentos".into() }]);
        assert_eq!(plan.runs, vec![PauseTarget { id: "r1".into(), label: "avulsa".into() }]);
    }

    #[test]
    fn parses_helper_lines() {
        assert_eq!(parse_line(r#"{"type":"partial","text":"forge abra"}"#), Some(VoiceEvent::Partial { text: "forge abra".into() }));
        assert_eq!(parse_line(r#"{"type":"final","text":"Forge, abra o projeto."}"#), Some(VoiceEvent::Final { text: "Forge, abra o projeto.".into() }));
        assert_eq!(parse_line(r#"{"locale":"pt-BR","onDevice":true,"type":"listening"}"#), Some(VoiceEvent::Listening { locale: "pt-BR".into(), on_device: true }));
        assert_eq!(
            parse_line(r#"{"type":"error","code":"mic-permission","message":"permissão de microfone: denied"}"#),
            Some(VoiceEvent::Error { code: "mic-permission".into(), message: "permissão de microfone: denied".into() })
        );
        assert!(matches!(parse_line(r#"{"type":"level","rms":0.4}"#), Some(VoiceEvent::Level { .. })));
        assert_eq!(parse_line("2026-11-03 forge-voice[1] No Assistant asset"), None);
        assert_eq!(parse_line(r#"{"type":"unknown"}"#), None);
    }

    #[test]
    fn validates_locales() {
        assert!(valid_locale("pt-BR") && valid_locale("en_US") && valid_locale("yue-CN"));
        assert!(!valid_locale("") && !valid_locale("-x") && !valid_locale("pt BR") && !valid_locale("pt;rm"));
    }

    #[test]
    fn word_recall_ignores_case_accents_and_punctuation() {
        assert_eq!(word_recall("Forge, crie uma missão para implementar pagamentos", "forge crie uma missao para implementar pagamentos."), 1.0);
        let r = word_recall("Forge, crie uma missão para implementar pagamentos", "Forj crie uma missão pra implementar pagamento");
        assert!(r > 0.4 && r < 0.8, "{r}");
        assert_eq!(word_recall("", "x"), 0.0);
    }

    #[test]
    fn missing_helper_is_reported() {
        let m = VoiceManager::with_helper(PathBuf::from("/nonexistent/forge-voice"));
        assert!(m.helper().is_none());
        let err = m.listen("pt-BR", false, 1.5, Arc::new(|_| {})).unwrap_err();
        assert!(matches!(err, AppError::NotFound(_) | AppError::Invalid(_)), "{err:?}");
        assert!(m.listen("pt;BR", false, 1.5, Arc::new(|_| {})).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn helper_is_built_and_reports_status() {
        let m = VoiceManager::default();
        let h = m.helper().expect("build.rs compiles forge-voice");
        assert!(h.is_file());
        let st = m.status();
        assert!(["authorized", "denied", "restricted", "notDetermined"].contains(&st.speech.as_str()), "{st:?}");
        assert!(["authorized", "denied", "restricted", "notDetermined"].contains(&st.microphone.as_str()), "{st:?}");
        let locales = m.locales().unwrap();
        assert!(locales.iter().any(|l| l.id == "pt-BR"), "pt-BR must be supported");
    }

    /// Needs speech-recognition permission for the process running the test.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "needs speech recognition permission"]
    fn live_self_test_transcribes_synthesized_speech() {
        let m = VoiceManager::default();
        let dir = tempfile::tempdir().unwrap();
        let r = m.self_test("pt-BR", false, dir.path()).unwrap();
        eprintln!("{r:?}");
        assert!(r.ok, "{r:?}");
    }
}
