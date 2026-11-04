//! Google Gemini API client (Generative Language API, v1beta).
//!
//! - The API key goes only in the `x-goog-api-key` header (never in URLs,
//!   logs or errors).
//! - Models are discovered from the account (`models.list`); nothing is
//!   hard-coded, so renamed/new models keep working.

use std::time::Duration;

use base64::Engine as _;
use serde::Serialize;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};

pub const DEFAULT_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";
pub const MAX_INLINE_BYTES: usize = 20 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    /// Without the "models/" prefix.
    pub id: String,
    pub display_name: String,
    pub methods: Vec<String>,
}

pub struct Gemini {
    base: String,
    key: String,
    http: reqwest::blocking::Client,
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn unb64(s: &str) -> AppResult<Vec<u8>> {
    base64::engine::general_purpose::STANDARD.decode(s).map_err(|e| AppError::Internal(format!("resposta com base64 inválido: {e}")))
}

/// Validates a model id coming from the UI (it is interpolated in a URL path).
pub fn valid_model(id: &str) -> bool {
    !id.is_empty() && id.len() <= 120 && id.chars().all(|c| c.is_ascii_alphanumeric() || "-._".contains(c))
}

impl Gemini {
    pub fn new(key: &str) -> AppResult<Self> {
        Self::with_base(DEFAULT_BASE, key)
    }

    pub fn with_base(base: &str, key: &str) -> AppResult<Self> {
        if key.trim().is_empty() {
            return Err(AppError::Invalid("chave da API do Gemini não definida".into()));
        }
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(300))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(Self { base: base.trim_end_matches('/').to_string(), key: key.trim().to_string(), http })
    }

    fn scrub(&self, s: &str) -> String {
        crate::redact::redact_secrets(&s.replace(&self.key, "[REDACTED]"))
    }

    fn handle(&self, resp: reqwest::Result<reqwest::blocking::Response>) -> AppResult<Value> {
        let resp = resp.map_err(|e| AppError::Internal(self.scrub(&format!("falha de rede com a API do Gemini: {e}"))))?;
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if !status.is_success() {
            let msg = v.pointer("/error/message").and_then(Value::as_str).unwrap_or(text.as_str());
            let msg = self.scrub(&msg.chars().take(600).collect::<String>());
            return Err(match status.as_u16() {
                400 if msg.contains("API key") => AppError::Forbidden(format!("chave do Gemini inválida: {msg}")),
                401 | 403 => AppError::Forbidden(format!("Gemini recusou o acesso ({status}): {msg}")),
                404 => AppError::NotFound(format!("Gemini: {msg}")),
                429 => AppError::Conflict(format!("Gemini: limite de uso atingido (429): {msg}")),
                _ => AppError::Invalid(format!("Gemini ({status}): {msg}")),
            });
        }
        Ok(v)
    }

    fn get(&self, path: &str) -> AppResult<Value> {
        self.handle(self.http.get(format!("{}/{path}", self.base)).header("x-goog-api-key", &self.key).send())
    }

    fn post(&self, path: &str, body: &Value) -> AppResult<Value> {
        self.handle(self.http.post(format!("{}/{path}", self.base)).header("x-goog-api-key", &self.key).json(body).send())
    }

    pub fn list_models(&self) -> AppResult<Vec<ModelInfo>> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        for _ in 0..20 {
            let path = match &token {
                Some(t) => format!("models?pageSize=200&pageToken={}", url::form_urlencoded::byte_serialize(t.as_bytes()).collect::<String>()),
                None => "models?pageSize=200".to_string(),
            };
            let v = self.get(&path)?;
            for m in v.get("models").and_then(Value::as_array).cloned().unwrap_or_default() {
                let name = m.get("name").and_then(Value::as_str).unwrap_or_default();
                let id = name.strip_prefix("models/").unwrap_or(name).to_string();
                if id.is_empty() {
                    continue;
                }
                out.push(ModelInfo {
                    display_name: m.get("displayName").and_then(Value::as_str).unwrap_or(&id).to_string(),
                    methods: m
                        .get("supportedGenerationMethods")
                        .and_then(Value::as_array)
                        .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
                        .unwrap_or_default(),
                    id,
                });
            }
            token = v.get("nextPageToken").and_then(Value::as_str).filter(|t| !t.is_empty()).map(String::from);
            if token.is_none() {
                break;
            }
        }
        Ok(out)
    }

    fn model_path(model: &str, method: &str) -> AppResult<String> {
        if !valid_model(model) {
            return Err(AppError::Invalid(format!("modelo inválido: {model}")));
        }
        Ok(format!("models/{model}:{method}"))
    }

    pub fn generate_content(&self, model: &str, body: &Value) -> AppResult<Value> {
        self.post(&Self::model_path(model, "generateContent")?, body)
    }

    /// Image generation: Imagen models use `:predict`, Gemini image models
    /// use `generateContent` with the IMAGE modality.
    pub fn generate_image(&self, model: &str, prompt: &str) -> AppResult<(String, Vec<u8>, Option<String>)> {
        if model.starts_with("imagen") {
            let v = self.post(&Self::model_path(model, "predict")?, &json!({ "instances": [{ "prompt": prompt }], "parameters": { "sampleCount": 1 } }))?;
            let p = v.pointer("/predictions/0").ok_or_else(|| blocked_or(&v, "o modelo não devolveu imagem"))?;
            let data = p.get("bytesBase64Encoded").and_then(Value::as_str).ok_or_else(|| blocked_or(&v, "o modelo não devolveu imagem"))?;
            let mime = p.get("mimeType").and_then(Value::as_str).unwrap_or("image/png").to_string();
            return Ok((mime, unb64(data)?, None));
        }
        let v = self.generate_content(
            model,
            &json!({
                "contents": [{ "role": "user", "parts": [{ "text": prompt }] }],
                "generationConfig": { "responseModalities": ["TEXT", "IMAGE"] }
            }),
        )?;
        let (mime, data) = first_inline(&v, "image/").ok_or_else(|| blocked_or(&v, "o modelo não devolveu imagem"))?;
        Ok((mime, data, text_of(&v)))
    }

    pub fn understand(&self, model: &str, instruction: &str, mime: &str, bytes: &[u8]) -> AppResult<String> {
        if bytes.len() > MAX_INLINE_BYTES {
            return Err(AppError::Invalid("arquivo maior que 20 MB (limite do envio direto à API)".into()));
        }
        let v = self.generate_content(
            model,
            &json!({ "contents": [{ "role": "user", "parts": [ { "inline_data": { "mime_type": mime, "data": b64(bytes) } }, { "text": instruction } ] }] }),
        )?;
        text_of(&v).ok_or_else(|| blocked_or(&v, "o modelo não devolveu texto"))
    }

    /// Text-to-speech: returns a WAV file.
    pub fn tts(&self, model: &str, text: &str, voice: &str) -> AppResult<Vec<u8>> {
        let v = self.generate_content(
            model,
            &json!({
                "contents": [{ "role": "user", "parts": [{ "text": text }] }],
                "generationConfig": {
                    "responseModalities": ["AUDIO"],
                    "speechConfig": { "voiceConfig": { "prebuiltVoiceConfig": { "voiceName": voice } } }
                }
            }),
        )?;
        let (mime, data) = first_inline(&v, "audio/").ok_or_else(|| blocked_or(&v, "o modelo não devolveu áudio"))?;
        if mime.contains("wav") {
            return Ok(data);
        }
        // Raw 16-bit little-endian PCM, e.g. "audio/L16;codec=pcm;rate=24000".
        let rate = mime.split(';').find_map(|p| p.trim().strip_prefix("rate=")).and_then(|r| r.parse().ok()).unwrap_or(24_000);
        Ok(super::wav_from_pcm16(&data, rate, 1))
    }

    /// Starts a long-running video generation; returns the operation name.
    pub fn video_start(&self, model: &str, prompt: &str) -> AppResult<String> {
        let v = self.post(&Self::model_path(model, "predictLongRunning")?, &json!({ "instances": [{ "prompt": prompt }] }))?;
        v.get("name").and_then(Value::as_str).map(String::from).ok_or_else(|| AppError::Internal("a API não devolveu a operação do vídeo".into()))
    }

    /// Polls an operation: `None` while running, `Some(uri)` when done.
    pub fn video_poll(&self, operation: &str) -> AppResult<Option<String>> {
        if !operation.starts_with("models/") && !operation.starts_with("operations/") || operation.contains("..") {
            return Err(AppError::Invalid("operação inválida".into()));
        }
        let v = self.get(operation)?;
        if v.get("done").and_then(Value::as_bool) != Some(true) {
            return Ok(None);
        }
        if let Some(e) = v.pointer("/error/message").and_then(Value::as_str) {
            return Err(AppError::Invalid(format!("Gemini: {}", self.scrub(e))));
        }
        let uri = v
            .pointer("/response/generateVideoResponse/generatedSamples/0/video/uri")
            .or_else(|| v.pointer("/response/generatedVideos/0/video/uri"))
            .and_then(Value::as_str)
            .ok_or_else(|| blocked_or(v.get("response").unwrap_or(&Value::Null), "a operação terminou sem vídeo"))?;
        Ok(Some(uri.to_string()))
    }

    /// Downloads a generated file (Google-hosted URI, authenticated by key).
    pub fn download(&self, uri: &str) -> AppResult<Vec<u8>> {
        let u = url::Url::parse(uri).map_err(|_| AppError::Invalid("URI de download inválida".into()))?;
        let base_host = url::Url::parse(&self.base).ok().and_then(|b| b.host_str().map(String::from));
        let host = u.host_str().unwrap_or_default();
        // The key is only ever sent to Google (or the configured base in tests).
        if Some(host.to_string()) != base_host && !host.ends_with(".googleapis.com") {
            return Err(AppError::Forbidden(format!("download recusado de host não-Google: {host}")));
        }
        let resp = self
            .http
            .get(u)
            .header("x-goog-api-key", &self.key)
            .send()
            .map_err(|e| AppError::Internal(self.scrub(&format!("falha ao baixar o vídeo: {e}"))))?;
        if !resp.status().is_success() {
            return Err(AppError::Invalid(format!("download do vídeo falhou ({})", resp.status())));
        }
        Ok(resp.bytes().map_err(|e| AppError::Internal(e.to_string()))?.to_vec())
    }
}

/// Text parts of the first candidate.
pub fn text_of(v: &Value) -> Option<String> {
    let parts = v.pointer("/candidates/0/content/parts")?.as_array()?;
    let t: Vec<&str> = parts.iter().filter_map(|p| p.get("text").and_then(Value::as_str)).collect();
    let joined = t.join("").trim().to_string();
    (!joined.is_empty()).then_some(joined)
}

/// First inline blob whose mime starts with `prefix`.
pub fn first_inline(v: &Value, prefix: &str) -> Option<(String, Vec<u8>)> {
    let parts = v.pointer("/candidates/0/content/parts")?.as_array()?;
    for p in parts {
        let Some(d) = p.get("inlineData").or_else(|| p.get("inline_data")) else { continue };
        let mime = d.get("mimeType").or_else(|| d.get("mime_type")).and_then(Value::as_str).unwrap_or_default();
        if mime.starts_with(prefix) {
            let data = d.get("data").and_then(Value::as_str)?;
            return unb64(data).ok().map(|b| (mime.to_string(), b));
        }
    }
    None
}

/// Explains an empty answer (safety block / finish reason) when possible.
fn blocked_or(v: &Value, fallback: &str) -> AppError {
    let reason = v
        .pointer("/promptFeedback/blockReason")
        .or_else(|| v.pointer("/candidates/0/finishReason"))
        .and_then(Value::as_str)
        .filter(|r| *r != "STOP");
    match reason {
        Some(r) => AppError::Invalid(format!("{fallback} (motivo informado pela API: {r})")),
        None => AppError::Invalid(fallback.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    type Seen = Arc<Mutex<Vec<(String, String, String)>>>;

    /// Minimal HTTP server replaying canned responses by path prefix and
    /// recording (request line, api-key header, body).
    fn server(routes: Vec<(&'static str, u16, String)>) -> (String, Seen) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}/v1beta", l.local_addr().unwrap().port());
        let seen: Seen = Arc::default();
        let s2 = seen.clone();
        std::thread::spawn(move || {
            for stream in l.incoming() {
                let Ok(mut s) = stream else { continue };
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut first = String::new();
                r.read_line(&mut first).unwrap();
                let mut len = 0usize;
                let mut key = String::new();
                loop {
                    let mut h = String::new();
                    r.read_line(&mut h).unwrap();
                    if h.trim().is_empty() {
                        break;
                    }
                    let lower = h.to_lowercase();
                    if let Some(v) = lower.strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                    if lower.starts_with("x-goog-api-key:") {
                        key = h.split_once(':').unwrap().1.trim().to_string();
                    }
                }
                let mut body = vec![0; len];
                r.read_exact(&mut body).unwrap();
                let path = first.split_whitespace().nth(1).unwrap_or("").to_string();
                s2.lock().unwrap().push((first.trim().to_string(), key, String::from_utf8_lossy(&body).to_string()));
                let (status, resp) = routes
                    .iter()
                    .find(|(p, _, _)| path.contains(p))
                    .map(|(_, st, b)| (*st, b.clone()))
                    .unwrap_or((404, r#"{"error":{"message":"not found"}}"#.into()));
                let _ = write!(s, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{resp}", resp.len());
            }
        });
        (base, seen)
    }

    #[test]
    fn lists_models_with_pagination_and_key_in_header_only() {
        let (base, seen) = server(vec![
            ("pageToken=p2", 200, r#"{"models":[{"name":"models/imagen-x","displayName":"Imagen X","supportedGenerationMethods":["predict"]}]}"#.into()),
            ("models?pageSize", 200, r#"{"models":[{"name":"models/gemini-flash-x","displayName":"Flash","supportedGenerationMethods":["generateContent","countTokens"]}],"nextPageToken":"p2"}"#.into()),
        ]);
        let g = Gemini::with_base(&base, "SECRET-KEY-123").unwrap();
        let models = g.list_models().unwrap();
        assert_eq!(models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["gemini-flash-x", "imagen-x"]);
        assert_eq!(models[0].methods, vec!["generateContent", "countTokens"]);
        for (line, key, _) in seen.lock().unwrap().iter() {
            assert_eq!(key, "SECRET-KEY-123");
            assert!(!line.contains("SECRET"), "key must never be in the URL: {line}");
        }
    }

    #[test]
    fn errors_are_mapped_and_never_echo_the_key() {
        let (base, _) = server(vec![("models", 400, r#"{"error":{"message":"API key not valid. Please pass a valid API key. SECRET-KEY-123"}}"#.into())]);
        let g = Gemini::with_base(&base, "SECRET-KEY-123").unwrap();
        let e = g.list_models().unwrap_err();
        assert!(matches!(e, AppError::Forbidden(_)), "{e:?}");
        assert!(!format!("{e:?}").contains("SECRET-KEY-123"));
        assert!(Gemini::with_base(&base, "  ").is_err());
        assert!(g.generate_content("../evil", &json!({})).is_err());
    }

    #[test]
    fn image_generation_via_generate_content_and_imagen_predict() {
        let png = b64(b"\x89PNG\r\n\x1a\nfake");
        let (base, seen) = server(vec![
            (":generateContent", 200, format!(r#"{{"candidates":[{{"content":{{"parts":[{{"text":"Here you go"}},{{"inlineData":{{"mimeType":"image/png","data":"{png}"}}}}]}}}}]}}"#)),
            (":predict", 200, format!(r#"{{"predictions":[{{"bytesBase64Encoded":"{png}","mimeType":"image/png"}}]}}"#)),
        ]);
        let g = Gemini::with_base(&base, "k").unwrap();
        let (mime, bytes, text) = g.generate_image("gemini-img", "a forge").unwrap();
        assert_eq!((mime.as_str(), &bytes[..4], text.as_deref()), ("image/png", &b"\x89PNG"[..], Some("Here you go")));
        let (_, b2, _) = g.generate_image("imagen-4", "a forge").unwrap();
        assert_eq!(&b2[..4], b"\x89PNG");
        let bodies: Vec<String> = seen.lock().unwrap().iter().map(|s| s.2.clone()).collect();
        assert!(bodies[0].contains("\"responseModalities\":[\"TEXT\",\"IMAGE\"]"));
        assert!(bodies[1].contains("\"instances\""));
    }

    #[test]
    fn blocked_responses_explain_the_reason() {
        let (base, _) = server(vec![(":generateContent", 200, r#"{"promptFeedback":{"blockReason":"SAFETY"}}"#.into())]);
        let g = Gemini::with_base(&base, "k").unwrap();
        let e = g.generate_image("gemini-img", "x").unwrap_err();
        assert!(format!("{e:?}").contains("SAFETY"));
    }

    #[test]
    fn understanding_and_tts_to_wav() {
        let pcm = b64(&[0u8, 1, 2, 3, 4, 5]);
        let (base, seen) = server(vec![
            ("gemini-tts:generateContent", 200, format!(r#"{{"candidates":[{{"content":{{"parts":[{{"inlineData":{{"mimeType":"audio/L16;codec=pcm;rate=24000","data":"{pcm}"}}}}]}}}}]}}"#)),
            (":generateContent", 200, r#"{"candidates":[{"content":{"parts":[{"text":"Um logo "},{"text":"azul."}]}}]}"#.into()),
        ]);
        let g = Gemini::with_base(&base, "k").unwrap();
        assert_eq!(g.understand("gemini-flash", "Descreva", "image/png", b"img").unwrap(), "Um logo azul.");
        assert!(seen.lock().unwrap()[0].2.contains("\"inline_data\""));
        let wav = g.tts("gemini-tts", "Olá", "Kore").unwrap();
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 24_000);
        assert_eq!(wav.len(), 44 + 6);
        assert!(g.understand("m", "x", "image/png", &vec![0; MAX_INLINE_BYTES + 1]).is_err());
    }

    #[test]
    fn video_operation_lifecycle_and_download_host_check() {
        let (base, _) = server(vec![
            (":predictLongRunning", 200, r#"{"name":"models/veo-x/operations/op1"}"#.into()),
            ("operations/op1", 200, r#"{"done":true,"response":{"generateVideoResponse":{"generatedSamples":[{"video":{"uri":"PLACEHOLDER"}}]}}}"#.into()),
            ("/files/v1", 200, "VIDEOBYTES".into()),
        ]);
        let g = Gemini::with_base(&base, "k").unwrap();
        let op = g.video_start("veo-x", "a forge").unwrap();
        assert_eq!(op, "models/veo-x/operations/op1");
        assert!(g.video_poll("../../etc").is_err());
        let uri = g.video_poll(&op).unwrap().unwrap();
        assert_eq!(uri, "PLACEHOLDER");
        assert_eq!(g.download(&format!("{base}/files/v1")).unwrap(), b"VIDEOBYTES");
        assert!(matches!(g.download("https://evil.example.com/x").unwrap_err(), AppError::Forbidden(_)));
    }

    /// Real API — needs GEMINI_API_KEY in the environment.
    #[test]
    #[ignore = "needs GEMINI_API_KEY"]
    fn live_list_models() {
        let key = std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY");
        let g = Gemini::new(&key).unwrap();
        let models = g.list_models().unwrap();
        eprintln!("{}", models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>().join(", "));
        assert!(!models.is_empty());
    }
}
