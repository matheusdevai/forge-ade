//! Minimal Chrome DevTools Protocol client over a synchronous WebSocket.
//!
//! One background thread owns the socket: it drains the outgoing queue and
//! reads incoming messages (short read timeout), resolving pending calls by
//! id and forwarding events to a handler. Event handlers run on that thread,
//! so they must never block on `call` — use `notify` (fire-and-forget).

use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

use crate::error::{AppError, AppResult};

pub type EventHandler = Arc<dyn Fn(&str, &Value, Option<&str>) + Send + Sync>;

type Pending = Arc<Mutex<HashMap<u64, Sender<Result<Value, String>>>>>;

pub struct CdpConn {
    tx: Sender<String>,
    pending: Pending,
    next: AtomicU64,
    closed: Arc<AtomicBool>,
}

/// Only loopback DevTools endpoints are accepted.
pub fn check_ws_url(url: &str) -> AppResult<()> {
    let rest = url.strip_prefix("ws://").ok_or_else(|| AppError::Invalid("endpoint CDP inválido".into()))?;
    let host = rest.split('/').next().unwrap_or("");
    let host = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host);
    if host == "127.0.0.1" || host == "localhost" || host == "[::1]" {
        Ok(())
    } else {
        Err(AppError::Forbidden("o CDP só aceita conexões locais".into()))
    }
}

impl CdpConn {
    pub fn connect(ws_url: &str, on_event: EventHandler) -> AppResult<Self> {
        check_ws_url(ws_url)?;
        let (mut socket, _) =
            tungstenite::connect(ws_url).map_err(|e| AppError::Internal(format!("falha ao conectar ao Chrome: {e}")))?;
        if let MaybeTlsStream::Plain(s) = socket.get_mut() {
            s.set_read_timeout(Some(Duration::from_millis(15)))?;
            let _ = s.set_nodelay(true);
        }
        let (tx, rx) = mpsc::channel::<String>();
        let pending: Pending = Arc::default();
        let closed = Arc::new(AtomicBool::new(false));
        {
            let pending = pending.clone();
            let closed = closed.clone();
            std::thread::Builder::new()
                .name("forge-cdp".into())
                .spawn(move || pump(socket, rx, pending, closed, on_event))?;
        }
        Ok(Self { tx, pending, next: AtomicU64::new(1), closed })
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn encode(&self, id: u64, method: &str, params: Value, session: Option<&str>) -> String {
        let mut msg = json!({ "id": id, "method": method, "params": params });
        if let Some(s) = session {
            msg["sessionId"] = json!(s);
        }
        msg.to_string()
    }

    /// Sends a command and waits for its result.
    pub fn call(&self, method: &str, params: Value, timeout: Duration) -> AppResult<Value> {
        self.call_in(None, method, params, timeout)
    }

    pub fn call_in(&self, session: Option<&str>, method: &str, params: Value, timeout: Duration) -> AppResult<Value> {
        if self.is_closed() {
            return Err(AppError::Conflict("a conexão com o navegador foi encerrada".into()));
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (rtx, rrx) = mpsc::channel();
        self.pending.lock().unwrap().insert(id, rtx);
        if self.tx.send(self.encode(id, method, params, session)).is_err() {
            self.pending.lock().unwrap().remove(&id);
            return Err(AppError::Conflict("a conexão com o navegador foi encerrada".into()));
        }
        match rrx.recv_timeout(timeout) {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => Err(AppError::Invalid(format!("{method}: {e}"))),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(AppError::Internal(format!("{method}: o navegador não respondeu a tempo")))
            }
        }
    }

    /// Sends a command without waiting (safe inside event handlers).
    pub fn notify(&self, method: &str, params: Value) {
        self.notify_in(None, method, params);
    }

    pub fn notify_in(&self, session: Option<&str>, method: &str, params: Value) {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let _ = self.tx.send(self.encode(id, method, params, session));
    }

    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

impl Drop for CdpConn {
    fn drop(&mut self) {
        self.close();
    }
}

fn pump(
    mut socket: WebSocket<MaybeTlsStream<TcpStream>>,
    rx: Receiver<String>,
    pending: Pending,
    closed: Arc<AtomicBool>,
    on_event: EventHandler,
) {
    loop {
        if closed.load(Ordering::SeqCst) {
            let _ = socket.close(None);
            let _ = socket.flush();
            break;
        }
        // Outgoing first (non-blocking drain).
        let mut wrote = false;
        loop {
            match rx.recv_timeout(Duration::from_millis(0)) {
                Ok(text) => {
                    if socket.write(Message::text(text)).is_err() {
                        closed.store(true, Ordering::SeqCst);
                        break;
                    }
                    wrote = true;
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    closed.store(true, Ordering::SeqCst);
                    break;
                }
            }
        }
        if wrote && socket.flush().is_err() {
            closed.store(true, Ordering::SeqCst);
        }
        match socket.read() {
            Ok(Message::Text(t)) => dispatch(t.as_str(), &pending, &on_event),
            Ok(Message::Binary(b)) => {
                if let Ok(s) = std::str::from_utf8(&b) {
                    dispatch(s, &pending, &on_event)
                }
            }
            Ok(Message::Close(_)) => {
                closed.store(true, Ordering::SeqCst);
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(_) => {
                closed.store(true, Ordering::SeqCst);
            }
        }
        if closed.load(Ordering::SeqCst) {
            break;
        }
    }
    // Fail every pending call and tell the handler we are gone.
    for (_, tx) in pending.lock().unwrap().drain() {
        let _ = tx.send(Err("conexão encerrada".into()));
    }
    on_event("Forge.disconnected", &Value::Null, None);
}

fn dispatch(text: &str, pending: &Pending, on_event: &EventHandler) {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return };
    if let Some(id) = v.get("id").and_then(Value::as_u64) {
        if let Some(tx) = pending.lock().unwrap().remove(&id) {
            let res = match v.get("error") {
                Some(err) => Err(err.get("message").and_then(Value::as_str).unwrap_or("erro").to_string()),
                None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
            };
            let _ = tx.send(res);
        }
        return;
    }
    if let Some(method) = v.get("method").and_then(Value::as_str) {
        let params = v.get("params").cloned().unwrap_or(Value::Null);
        on_event(method, &params, v.get("sessionId").and_then(Value::as_str));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_loopback_endpoints() {
        assert!(check_ws_url("ws://127.0.0.1:9222/devtools/browser/x").is_ok());
        assert!(check_ws_url("ws://localhost:1/devtools/page/y").is_ok());
        assert!(check_ws_url("ws://[::1]:5/devtools/page/y").is_ok());
        assert!(check_ws_url("ws://evil.com:9222/devtools").is_err());
        assert!(check_ws_url("wss://127.0.0.1:9222/").is_err());
        assert!(check_ws_url("http://127.0.0.1:9222/").is_err());
    }

    #[test]
    fn dispatch_routes_results_errors_and_events() {
        let pending: Pending = Arc::default();
        let (tx1, rx1) = mpsc::channel();
        let (tx2, rx2) = mpsc::channel();
        pending.lock().unwrap().insert(1, tx1);
        pending.lock().unwrap().insert(2, tx2);
        let seen = Arc::new(Mutex::new(Vec::<(String, Option<String>)>::new()));
        let s2 = seen.clone();
        let h: EventHandler = Arc::new(move |m, _p, s| s2.lock().unwrap().push((m.into(), s.map(String::from))));
        dispatch(r#"{"id":1,"result":{"ok":true}}"#, &pending, &h);
        dispatch(r#"{"id":2,"error":{"code":-32000,"message":"nope"}}"#, &pending, &h);
        dispatch(r#"{"method":"Page.loadEventFired","params":{},"sessionId":"S"}"#, &pending, &h);
        dispatch("not json", &pending, &h);
        assert_eq!(rx1.recv().unwrap().unwrap()["ok"], true);
        assert_eq!(rx2.recv().unwrap().unwrap_err(), "nope");
        assert_eq!(seen.lock().unwrap().as_slice(), &[("Page.loadEventFired".to_string(), Some("S".to_string()))]);
        assert!(pending.lock().unwrap().is_empty());
    }
}
