//! Normalizes each provider's streaming output into a common event model.
//!
//! Formats (verified against the installed CLIs where noted):
//! - Claude Code `-p --output-format stream-json --verbose --include-partial-messages`
//!   — verified with real output (fixtures/claude_tools.jsonl).
//! - Gemini `-o stream-json` — event shapes taken from the installed CLI's source
//!   (init/message/tool_use/tool_result/error/result).
//! - Codex `exec --json` — envelope verified (thread/turn/error events); item
//!   payloads follow the documented schema until a logged-in run confirms them.

use serde::Serialize;
use serde_json::Value;

use crate::redact::sanitize;

const MAX_TEXT: usize = 64 * 1024;
const MAX_TOOL_IO: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvent {
    Session { session_id: Option<String>, model: Option<String> },
    /// Live text chunk (streamed to the UI, not persisted).
    TextDelta { text: String },
    Text { text: String },
    ToolUse { id: Option<String>, name: String, input: String },
    ToolResult { id: Option<String>, ok: bool, output: String },
    Warning { message: String },
    Error { message: String },
    Result {
        ok: bool,
        text: Option<String>,
        error: Option<String>,
        input_tokens: Option<i64>,
        output_tokens: Option<i64>,
        cache_read_tokens: Option<i64>,
        cost_usd: Option<f64>,
        duration_ms: Option<i64>,
        turns: Option<i64>,
    },
}

impl AgentEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            AgentEvent::Session { .. } => "session",
            AgentEvent::TextDelta { .. } => "text_delta",
            AgentEvent::Text { .. } => "text",
            AgentEvent::ToolUse { .. } => "tool_use",
            AgentEvent::ToolResult { .. } => "tool_result",
            AgentEvent::Warning { .. } => "warning",
            AgentEvent::Error { .. } => "error",
            AgentEvent::Result { .. } => "result",
        }
    }
    pub fn persisted(&self) -> bool {
        !matches!(self, AgentEvent::TextDelta { .. })
    }
}

/// Accumulated outcome of a run.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Summary {
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub ok: Option<bool>,
    pub text: Option<String>,
    pub error: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<i64>,
}

fn s(v: &Value, ptr: &str) -> Option<String> {
    v.pointer(ptr).and_then(Value::as_str).map(str::to_string)
}
fn i(v: &Value, ptr: &str) -> Option<i64> {
    v.pointer(ptr).and_then(Value::as_i64)
}
fn compact(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub struct StreamParser {
    provider: String,
    pub summary: Summary,
    /// Gemini streams assistant text as deltas only; buffer to emit full text.
    pending_text: String,
    last_text: Option<String>,
}

impl StreamParser {
    pub fn new(provider: &str) -> Self {
        Self { provider: provider.to_string(), summary: Summary::default(), pending_text: String::new(), last_text: None }
    }

    pub fn feed(&mut self, line: &str) -> Vec<AgentEvent> {
        let line = line.trim();
        if !line.starts_with('{') {
            return vec![];
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return vec![] };
        let mut out = match self.provider.as_str() {
            "claude" => self.claude(&v),
            "codex" => self.codex(&v),
            "gemini" => self.gemini(&v),
            _ => vec![],
        };
        for ev in &mut out {
            sanitize_event(ev);
        }
        out
    }

    /// Flushes buffered text at end of stream.
    pub fn finish(&mut self) -> Vec<AgentEvent> {
        let mut out = vec![];
        if !self.pending_text.is_empty() {
            let text = std::mem::take(&mut self.pending_text);
            self.last_text = Some(text.clone());
            out.push(AgentEvent::Text { text });
        }
        if self.summary.text.is_none() {
            self.summary.text = self.last_text.clone();
        }
        for ev in &mut out {
            sanitize_event(ev);
        }
        out
    }

    fn set_session(&mut self, session_id: Option<String>, model: Option<String>) -> AgentEvent {
        if session_id.is_some() {
            self.summary.session_id = session_id.clone();
        }
        if model.is_some() {
            self.summary.model = model.clone();
        }
        AgentEvent::Session { session_id, model }
    }

    fn result_event(&self) -> AgentEvent {
        let s = &self.summary;
        AgentEvent::Result {
            ok: s.ok.unwrap_or(false),
            text: s.text.clone(),
            error: s.error.clone(),
            input_tokens: s.input_tokens,
            output_tokens: s.output_tokens,
            cache_read_tokens: s.cache_read_tokens,
            cost_usd: s.cost_usd,
            duration_ms: s.duration_ms,
            turns: None,
        }
    }

    // ── Claude Code ───────────────────────────────────────────────────────
    fn claude(&mut self, v: &Value) -> Vec<AgentEvent> {
        match v.get("type").and_then(Value::as_str) {
            Some("system") if v.get("subtype").and_then(Value::as_str) == Some("init") => {
                vec![self.set_session(s(v, "/session_id"), s(v, "/model"))]
            }
            Some("stream_event") => {
                let ev = &v["event"];
                if ev.get("type").and_then(Value::as_str) == Some("content_block_delta")
                    && ev.pointer("/delta/type").and_then(Value::as_str) == Some("text_delta")
                {
                    if let Some(t) = s(ev, "/delta/text").filter(|t| !t.is_empty()) {
                        return vec![AgentEvent::TextDelta { text: t }];
                    }
                }
                vec![]
            }
            Some("assistant") => {
                let mut out = vec![];
                for b in v.pointer("/message/content").and_then(Value::as_array).into_iter().flatten() {
                    match b.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            if let Some(t) = s(b, "/text").filter(|t| !t.trim().is_empty()) {
                                self.last_text = Some(t.clone());
                                out.push(AgentEvent::Text { text: t });
                            }
                        }
                        Some("tool_use") => out.push(AgentEvent::ToolUse {
                            id: s(b, "/id"),
                            name: s(b, "/name").unwrap_or_else(|| "tool".into()),
                            input: compact(b.get("input").unwrap_or(&Value::Null)),
                        }),
                        _ => {}
                    }
                }
                out
            }
            Some("user") => {
                let mut out = vec![];
                for b in v.pointer("/message/content").and_then(Value::as_array).into_iter().flatten() {
                    if b.get("type").and_then(Value::as_str) == Some("tool_result") {
                        let output = match b.get("content") {
                            Some(Value::String(s)) => s.clone(),
                            Some(Value::Array(parts)) => parts.iter().filter_map(|p| s(p, "/text")).collect::<Vec<_>>().join("\n"),
                            Some(other) => other.to_string(),
                            None => String::new(),
                        };
                        out.push(AgentEvent::ToolResult {
                            id: s(b, "/tool_use_id"),
                            ok: !b.get("is_error").and_then(Value::as_bool).unwrap_or(false),
                            output,
                        });
                    }
                }
                out
            }
            Some("result") => {
                let is_error = v.get("is_error").and_then(Value::as_bool).unwrap_or(true);
                let ok = !is_error && v.get("subtype").and_then(Value::as_str) == Some("success");
                let sm = &mut self.summary;
                sm.ok = Some(ok);
                let res = s(v, "/result");
                if ok {
                    sm.text = res.or_else(|| self.last_text.clone());
                } else {
                    sm.error = res.or_else(|| s(v, "/subtype"));
                }
                sm.input_tokens = i(v, "/usage/input_tokens");
                sm.output_tokens = i(v, "/usage/output_tokens");
                sm.cache_read_tokens = i(v, "/usage/cache_read_input_tokens");
                sm.cost_usd = v.get("total_cost_usd").and_then(Value::as_f64);
                sm.duration_ms = i(v, "/duration_ms");
                let mut ev = self.result_event();
                if let AgentEvent::Result { turns, .. } = &mut ev {
                    *turns = i(v, "/num_turns");
                }
                vec![ev]
            }
            _ => vec![],
        }
    }

    // ── Codex ─────────────────────────────────────────────────────────────
    fn codex(&mut self, v: &Value) -> Vec<AgentEvent> {
        match v.get("type").and_then(Value::as_str) {
            Some("thread.started") => vec![self.set_session(s(v, "/thread_id"), None)],
            Some("item.started") => {
                let item = &v["item"];
                match item.get("type").and_then(Value::as_str) {
                    Some("command_execution") => vec![AgentEvent::ToolUse {
                        id: s(item, "/id"),
                        name: "shell".into(),
                        input: s(item, "/command").unwrap_or_default(),
                    }],
                    _ => vec![],
                }
            }
            Some("item.completed") => {
                let item = &v["item"];
                match item.get("type").and_then(Value::as_str) {
                    Some("agent_message") => {
                        let t = s(item, "/text").unwrap_or_default();
                        self.last_text = Some(t.clone());
                        vec![AgentEvent::Text { text: t }]
                    }
                    Some("command_execution") => vec![AgentEvent::ToolResult {
                        id: s(item, "/id"),
                        ok: i(item, "/exit_code") == Some(0) || item.get("status").and_then(Value::as_str) == Some("completed"),
                        output: s(item, "/aggregated_output").unwrap_or_default(),
                    }],
                    Some("file_change") => {
                        let changes = item
                            .get("changes")
                            .and_then(Value::as_array)
                            .map(|c| c.iter().map(|x| format!("{} {}", s(x, "/kind").unwrap_or_default(), s(x, "/path").unwrap_or_default())).collect::<Vec<_>>().join("\n"))
                            .unwrap_or_default();
                        vec![
                            AgentEvent::ToolUse { id: s(item, "/id"), name: "edit".into(), input: changes },
                            AgentEvent::ToolResult { id: s(item, "/id"), ok: item.get("status").and_then(Value::as_str) != Some("failed"), output: String::new() },
                        ]
                    }
                    Some("mcp_tool_call") => vec![AgentEvent::ToolUse {
                        id: s(item, "/id"),
                        name: format!("{}:{}", s(item, "/server").unwrap_or_default(), s(item, "/tool").unwrap_or_default()),
                        input: compact(item.get("arguments").unwrap_or(&Value::Null)),
                    }],
                    Some("web_search") => vec![AgentEvent::ToolUse { id: s(item, "/id"), name: "web_search".into(), input: s(item, "/query").unwrap_or_default() }],
                    Some("error") => vec![AgentEvent::Warning { message: s(item, "/message").unwrap_or_default() }],
                    _ => vec![],
                }
            }
            Some("turn.completed") => {
                let sm = &mut self.summary;
                sm.ok = Some(self.last_text.is_some());
                sm.text = self.last_text.clone();
                sm.input_tokens = i(v, "/usage/input_tokens");
                sm.output_tokens = i(v, "/usage/output_tokens");
                sm.cache_read_tokens = i(v, "/usage/cached_input_tokens");
                vec![self.result_event()]
            }
            Some("turn.failed") => {
                self.summary.ok = Some(false);
                self.summary.error = s(v, "/error/message").or(Some("o turno falhou".into()));
                vec![self.result_event()]
            }
            Some("error") => {
                let m = s(v, "/message").unwrap_or_default();
                if m.starts_with("Reconnecting") {
                    vec![AgentEvent::Warning { message: m }]
                } else {
                    self.summary.error = Some(m.clone());
                    vec![AgentEvent::Error { message: m }]
                }
            }
            _ => vec![],
        }
    }

    // ── Gemini ────────────────────────────────────────────────────────────
    fn flush_gemini_text(&mut self) -> Option<AgentEvent> {
        if self.pending_text.trim().is_empty() {
            self.pending_text.clear();
            return None;
        }
        let text = std::mem::take(&mut self.pending_text);
        self.last_text = Some(text.clone());
        Some(AgentEvent::Text { text })
    }

    fn gemini(&mut self, v: &Value) -> Vec<AgentEvent> {
        match v.get("type").and_then(Value::as_str) {
            Some("init") => vec![self.set_session(s(v, "/session_id"), s(v, "/model"))],
            Some("message") if v.get("role").and_then(Value::as_str) == Some("assistant") => {
                let t = s(v, "/content").unwrap_or_default();
                if t.is_empty() {
                    return vec![];
                }
                self.pending_text.push_str(&t);
                vec![AgentEvent::TextDelta { text: t }]
            }
            Some("tool_use") => {
                let mut out: Vec<AgentEvent> = self.flush_gemini_text().into_iter().collect();
                out.push(AgentEvent::ToolUse {
                    id: s(v, "/tool_id"),
                    name: s(v, "/tool_name").unwrap_or_else(|| "tool".into()),
                    input: compact(v.get("parameters").unwrap_or(&Value::Null)),
                });
                out
            }
            Some("tool_result") => {
                let ok = v.get("status").and_then(Value::as_str) == Some("success");
                let output = if ok { s(v, "/output").unwrap_or_default() } else { s(v, "/error/message").or_else(|| s(v, "/output")).unwrap_or_default() };
                vec![AgentEvent::ToolResult { id: s(v, "/tool_id"), ok, output }]
            }
            Some("error") => {
                let m = s(v, "/message").or_else(|| s(v, "/error/message")).unwrap_or_default();
                self.summary.error = Some(m.clone());
                vec![AgentEvent::Error { message: m }]
            }
            Some("result") => {
                let mut out: Vec<AgentEvent> = self.flush_gemini_text().into_iter().collect();
                let ok = v.get("status").and_then(Value::as_str) == Some("success");
                let sm = &mut self.summary;
                sm.ok = Some(ok);
                sm.text = self.last_text.clone();
                if !ok && sm.error.is_none() {
                    sm.error = s(v, "/error/message").or(Some("a execução falhou".into()));
                }
                sm.input_tokens = i(v, "/stats/input_tokens");
                sm.output_tokens = i(v, "/stats/output_tokens");
                sm.cache_read_tokens = i(v, "/stats/cached");
                sm.duration_ms = i(v, "/stats/duration_ms");
                out.push(self.result_event());
                out
            }
            _ => vec![],
        }
    }
}

/// Redacts secrets and caps sizes in every event before it leaves the parser.
fn sanitize_event(ev: &mut AgentEvent) {
    match ev {
        AgentEvent::TextDelta { text } | AgentEvent::Text { text } => *text = sanitize(text, MAX_TEXT),
        AgentEvent::ToolUse { input, .. } => *input = sanitize(input, MAX_TOOL_IO),
        AgentEvent::ToolResult { output, .. } => *output = sanitize(output, MAX_TOOL_IO),
        AgentEvent::Warning { message } | AgentEvent::Error { message } => *message = sanitize(message, MAX_TOOL_IO),
        AgentEvent::Result { text, error, .. } => {
            if let Some(t) = text {
                *t = sanitize(t, MAX_TEXT);
            }
            if let Some(e) = error {
                *e = sanitize(e, MAX_TOOL_IO);
            }
        }
        AgentEvent::Session { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(provider: &str, input: &str) -> (Vec<AgentEvent>, Summary) {
        let mut p = StreamParser::new(provider);
        let mut out: Vec<AgentEvent> = input.lines().flat_map(|l| p.feed(l)).collect();
        out.extend(p.finish());
        (out, p.summary)
    }

    #[test]
    fn claude_real_tool_run() {
        let (events, sum) = run("claude", include_str!("fixtures/claude_tools.jsonl"));
        let kinds: Vec<_> = events.iter().map(|e| e.kind()).collect();
        assert_eq!(kinds.first(), Some(&"session"));
        let tool = events.iter().find_map(|e| match e {
            AgentEvent::ToolUse { name, input, .. } => Some((name.clone(), input.clone())),
            _ => None,
        });
        assert_eq!(tool.as_ref().map(|t| t.0.as_str()), Some("Bash"));
        assert!(tool.unwrap().1.contains("ls -A"));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::ToolResult { ok: true, output, .. } if output.contains("a.txt"))));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::TextDelta { text } if text.contains("a.txt"))));
        assert!(matches!(events.last(), Some(AgentEvent::Result { ok: true, turns: Some(2), .. })));
        assert_eq!(sum.text.as_deref(), Some("a.txt, b.md"));
        assert_eq!(sum.session_id.as_deref(), Some("00000000-0000-0000-0000-000000000000"));
        assert!(sum.cost_usd.is_some());
        // Deltas are streamed but never persisted.
        assert!(!AgentEvent::TextDelta { text: "x".into() }.persisted());
    }

    #[test]
    fn claude_simple_real_stream_and_noise() {
        let (events, sum) = run("claude", include_str!("../providers/fixtures/claude_stream.jsonl"));
        assert_eq!(sum.ok, Some(true));
        assert_eq!(sum.text.as_deref(), Some("FORGE_OK"));
        assert_eq!(sum.model.as_deref(), Some("claude-haiku-5-5"));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::Text { text } if text == "FORGE_OK")));
    }

    #[test]
    fn codex_real_unauthorized_stream() {
        let (events, sum) = run("codex", include_str!("../providers/fixtures/codex_unauthorized.jsonl"));
        assert_eq!(sum.ok, Some(false));
        assert!(sum.error.unwrap().contains("401"));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::Warning { .. })), "reconnects are warnings");
        assert!(matches!(events.last(), Some(AgentEvent::Result { ok: false, .. })));
        assert_eq!(sum.session_id.as_deref(), Some("00000000-0000-0000-0000-000000000000"));
    }

    #[test]
    fn codex_documented_item_shapes() {
        let s = r#"{"type":"thread.started","thread_id":"th1"}
{"type":"item.started","item":{"id":"i1","type":"command_execution","command":"ls","status":"in_progress"}}
{"type":"item.completed","item":{"id":"i1","type":"command_execution","command":"ls","aggregated_output":"a.txt\n","exit_code":0,"status":"completed"}}
{"type":"item.completed","item":{"id":"i2","type":"file_change","changes":[{"path":"a.txt","kind":"update"}],"status":"completed"}}
{"type":"item.completed","item":{"id":"i3","type":"agent_message","text":"done"}}
{"type":"turn.completed","usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}"#;
        let (events, sum) = run("codex", s);
        assert_eq!(events.iter().filter(|e| e.kind() == "tool_use").count(), 2);
        assert!(events.iter().any(|e| matches!(e, AgentEvent::ToolUse { name, input, .. } if name == "edit" && input.contains("update a.txt"))));
        assert_eq!((sum.ok, sum.text.as_deref(), sum.input_tokens), (Some(true), Some("done"), Some(10)));
        assert_eq!(sum.cost_usd, None);
    }

    #[test]
    fn gemini_source_derived_shapes() {
        let s = r#"{"type":"init","timestamp":"t","session_id":"s1","model":"gemini-2.5-pro"}
{"type":"message","timestamp":"t","role":"user","content":"hi"}
{"type":"message","timestamp":"t","role":"assistant","content":"Vou listar ","delta":true}
{"type":"message","timestamp":"t","role":"assistant","content":"os arquivos.","delta":true}
{"type":"tool_use","timestamp":"t","tool_name":"list_directory","tool_id":"t1","parameters":{"path":"."}}
{"type":"tool_result","timestamp":"t","tool_id":"t1","status":"success","output":"a.txt"}
{"type":"message","timestamp":"t","role":"assistant","content":"a.txt","delta":true}
{"type":"result","timestamp":"t","status":"success","stats":{"total_tokens":20,"input_tokens":15,"output_tokens":5,"cached":0,"duration_ms":900,"tool_calls":1}}"#;
        let (events, sum) = run("gemini", s);
        let texts: Vec<_> = events.iter().filter_map(|e| if let AgentEvent::Text { text } = e { Some(text.as_str()) } else { None }).collect();
        assert_eq!(texts, ["Vou listar os arquivos.", "a.txt"]);
        assert_eq!(events.iter().filter(|e| e.kind() == "text_delta").count(), 3);
        assert_eq!((sum.ok, sum.text.as_deref(), sum.input_tokens, sum.duration_ms), (Some(true), Some("a.txt"), Some(15), Some(900)));
        assert_eq!(sum.model.as_deref(), Some("gemini-2.5-pro"));
    }

    #[test]
    fn secrets_never_leave_the_parser() {
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t","content":"OPENAI_API_KEY=sk-proj-abcdefghijklmnopqrstuvwxyz123","is_error":false}]}}"#;
        let (events, _) = run("claude", line);
        let AgentEvent::ToolResult { output, .. } = &events[0] else { panic!() };
        assert!(!output.contains("sk-proj-abc"));
        assert!(output.contains("REDACTED"));
    }
}
