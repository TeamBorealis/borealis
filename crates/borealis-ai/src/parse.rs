//! Line-by-line stream parsers that turn backend JSONL into [`AiEvent`]s.
//!
//! Parsers are pure state machines (no I/O) so they can be tested against recorded fixtures.
//! Unknown event types are ignored on purpose: both CLIs add event kinds between versions.

use std::collections::HashSet;

use serde_json::Value;

use crate::event::{AiEvent, ToolUseInfo, Usage};
use crate::{redact_paths, Backend};

/// Name Claude gives to its synthetic structured-output tool; never surfaced as a tool use.
const CLAUDE_STRUCTURED_TOOL: &str = "StructuredOutput";

/// Backend-specific JSONL parser.
pub trait StreamParser: Send {
    /// Feed one stdout line; returns the events it produced (possibly none).
    fn feed_line(&mut self, line: &str) -> Vec<AiEvent>;
    /// Called once at EOF to flush anything pending.
    fn finish(&mut self) -> Vec<AiEvent>;
}

/// Build the parser for a backend.
pub fn parser_for(backend: Backend) -> Box<dyn StreamParser> {
    match backend {
        Backend::Claude => Box::new(ClaudeParser::default()),
        Backend::Codex => Box::new(CodexParser::default()),
    }
}

/// Parse a complete recorded stream (test and tooling helper).
pub fn parse_stream(backend: Backend, input: &str) -> Vec<AiEvent> {
    let mut p = parser_for(backend);
    let mut out: Vec<AiEvent> = input.lines().flat_map(|l| p.feed_line(l)).collect();
    out.extend(p.finish());
    out
}

fn get_str<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn get_u64(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// Strip markdown fences and surrounding prose, then parse the first JSON object/array found.
pub fn extract_json(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        if v.is_object() || v.is_array() {
            return Some(v);
        }
    }
    let unfenced = strip_fence(trimmed);
    if let Ok(v) = serde_json::from_str::<Value>(unfenced) {
        if v.is_object() || v.is_array() {
            return Some(v);
        }
    }
    first_balanced(unfenced).and_then(|s| serde_json::from_str::<Value>(s).ok())
}

fn strip_fence(s: &str) -> &str {
    let Some(start) = s.find("```") else { return s };
    let after = &s[start + 3..];
    let body_start = after.find('\n').map_or(0, |i| i + 1);
    let body = &after[body_start..];
    match body.find("```") {
        Some(end) => body[..end].trim(),
        None => body.trim(),
    }
}

/// Find the first balanced `{...}` or `[...]` slice, respecting string literals.
fn first_balanced(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let start = bytes.iter().position(|b| matches!(b, b'{' | b'['))?;
    let (mut depth, mut in_str, mut esc) = (0usize, false, false);
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_str {
            match (esc, b) {
                (true, _) => esc = false,
                (false, b'\\') => esc = true,
                (false, b'"') => in_str = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

// ---------------------------------------------------------------------------------------------
// Claude Code: `claude -p --output-format stream-json --verbose --include-partial-messages`
// ---------------------------------------------------------------------------------------------

/// Parser for Claude Code `stream-json` output.
#[derive(Debug, Default)]
pub struct ClaudeParser {
    /// A text delta was streamed for the current message, so the consolidated `assistant`
    /// message must not repeat it.
    streamed_text: bool,
    finished: bool,
}

impl StreamParser for ClaudeParser {
    fn feed_line(&mut self, line: &str) -> Vec<AiEvent> {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return vec![];
        };
        match get_str(&v, "type") {
            Some("system") => self.on_system(&v),
            Some("stream_event") => self.on_stream_event(&v),
            Some("assistant") => self.on_assistant(&v),
            Some("result") => self.on_result(&v),
            _ => vec![],
        }
    }

    fn finish(&mut self) -> Vec<AiEvent> {
        vec![]
    }
}

impl ClaudeParser {
    fn on_system(&mut self, v: &Value) -> Vec<AiEvent> {
        if get_str(v, "subtype") != Some("init") {
            return vec![];
        }
        vec![AiEvent::Started {
            session_id: get_str(v, "session_id").map(str::to_string),
            model: get_str(v, "model").map(str::to_string),
        }]
    }

    fn on_stream_event(&mut self, v: &Value) -> Vec<AiEvent> {
        let Some(ev) = v.get("event") else {
            return vec![];
        };
        match get_str(ev, "type") {
            Some("message_start") => {
                self.streamed_text = false;
                vec![]
            }
            Some("content_block_delta") => {
                let delta = ev.get("delta").cloned().unwrap_or(Value::Null);
                match (get_str(&delta, "type"), get_str(&delta, "text")) {
                    (Some("text_delta"), Some(text)) if !text.is_empty() => {
                        self.streamed_text = true;
                        vec![AiEvent::TextDelta {
                            text: text.to_string(),
                        }]
                    }
                    _ => vec![],
                }
            }
            _ => vec![],
        }
    }

    fn on_assistant(&mut self, v: &Value) -> Vec<AiEvent> {
        let Some(blocks) = v.pointer("/message/content").and_then(Value::as_array) else {
            return vec![];
        };
        let mut out = Vec::new();
        for b in blocks {
            match get_str(b, "type") {
                Some("text") if !self.streamed_text => {
                    if let Some(t) = get_str(b, "text").filter(|t| !t.is_empty()) {
                        out.push(AiEvent::TextDelta {
                            text: t.to_string(),
                        });
                    }
                }
                Some("tool_use") => {
                    let name = get_str(b, "name").unwrap_or("tool");
                    if name == CLAUDE_STRUCTURED_TOOL {
                        continue;
                    }
                    let summary = b.get("input").map(Value::to_string).unwrap_or_default();
                    let id = get_str(b, "id").map(str::to_string);
                    out.push(AiEvent::ToolUse(ToolUseInfo::new(id, name, &summary)));
                }
                _ => {}
            }
        }
        out
    }

    fn on_result(&mut self, v: &Value) -> Vec<AiEvent> {
        if self.finished {
            return vec![];
        }
        self.finished = true;
        let mut out = vec![AiEvent::Usage(claude_usage(v))];
        let text = get_str(v, "result").unwrap_or_default();
        if v.get("is_error").and_then(Value::as_bool).unwrap_or(false) {
            let msg = if text.is_empty() {
                "Claude Code reported an error"
            } else {
                text
            };
            out.push(AiEvent::Error {
                message: redact_paths(msg),
            });
            return out;
        }
        let structured = v
            .get("structured_output")
            .filter(|s| !s.is_null())
            .cloned()
            .or_else(|| extract_json(text));
        out.push(AiEvent::Final {
            text: text.to_string(),
            structured,
        });
        out
    }
}

fn claude_usage(v: &Value) -> Usage {
    let u = v.get("usage").cloned().unwrap_or(Value::Null);
    let cache_read = get_u64(&u, "cache_read_input_tokens");
    let cache_create = get_u64(&u, "cache_creation_input_tokens");
    Usage {
        input_tokens: get_u64(&u, "input_tokens") + cache_read + cache_create,
        output_tokens: get_u64(&u, "output_tokens"),
        cached_input_tokens: Some(cache_read),
        cost_usd: v.get("total_cost_usd").and_then(Value::as_f64),
    }
}

// ---------------------------------------------------------------------------------------------
// Codex: `codex exec --json`
// ---------------------------------------------------------------------------------------------

/// Parser for `codex exec --json` JSONL output.
#[derive(Debug, Default)]
pub struct CodexParser {
    last_message: Option<String>,
    seen_tools: HashSet<String>,
    error_emitted: bool,
    finished: bool,
}

impl StreamParser for CodexParser {
    fn feed_line(&mut self, line: &str) -> Vec<AiEvent> {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return vec![];
        };
        match get_str(&v, "type") {
            Some("thread.started") => vec![AiEvent::Started {
                session_id: get_str(&v, "thread_id").map(str::to_string),
                model: None,
            }],
            Some("item.started") | Some("item.completed") => self.on_item(&v),
            Some("turn.completed") => self.on_turn_completed(&v),
            Some("turn.failed") => {
                let msg = v.pointer("/error/message").and_then(Value::as_str);
                self.fail(msg.unwrap_or("Codex turn failed"))
            }
            Some("error") => self.fail(get_str(&v, "message").unwrap_or("Codex reported an error")),
            _ => vec![],
        }
    }

    fn finish(&mut self) -> Vec<AiEvent> {
        vec![]
    }
}

impl CodexParser {
    fn fail(&mut self, message: &str) -> Vec<AiEvent> {
        if self.error_emitted || self.finished {
            return vec![];
        }
        self.error_emitted = true;
        vec![AiEvent::Error {
            message: redact_paths(message),
        }]
    }

    fn on_item(&mut self, v: &Value) -> Vec<AiEvent> {
        let Some(item) = v.get("item") else {
            return vec![];
        };
        let id = get_str(item, "id").unwrap_or_default().to_string();
        match get_str(item, "type") {
            Some("agent_message") => {
                let text = get_str(item, "text").unwrap_or_default();
                if get_str(v, "type") != Some("item.completed") || text.is_empty() {
                    return vec![];
                }
                self.last_message = Some(text.to_string());
                vec![AiEvent::TextDelta {
                    text: text.to_string(),
                }]
            }
            Some("error") => vec![AiEvent::Notice {
                message: redact_paths(get_str(item, "message").unwrap_or("Codex warning")),
            }],
            Some(kind @ ("command_execution" | "file_change" | "mcp_tool_call" | "web_search")) => {
                if !self.seen_tools.insert(id.clone()) {
                    return vec![];
                }
                let (name, summary) = codex_tool_summary(kind, item);
                vec![AiEvent::ToolUse(ToolUseInfo::new(
                    Some(id),
                    &name,
                    &summary,
                ))]
            }
            _ => vec![],
        }
    }

    fn on_turn_completed(&mut self, v: &Value) -> Vec<AiEvent> {
        if self.finished {
            return vec![];
        }
        self.finished = true;
        let u = v.get("usage").cloned().unwrap_or(Value::Null);
        let usage = Usage {
            input_tokens: get_u64(&u, "input_tokens"),
            output_tokens: get_u64(&u, "output_tokens"),
            cached_input_tokens: u.get("cached_input_tokens").and_then(Value::as_u64),
            cost_usd: None,
        };
        let text = self.last_message.clone().unwrap_or_default();
        let structured = extract_json(&text);
        vec![AiEvent::Usage(usage), AiEvent::Final { text, structured }]
    }
}

fn codex_tool_summary(kind: &str, item: &Value) -> (String, String) {
    match kind {
        "command_execution" => (
            "shell".into(),
            get_str(item, "command").unwrap_or("").into(),
        ),
        "file_change" => (
            "file_change".into(),
            item.get("changes")
                .map(Value::to_string)
                .unwrap_or_default(),
        ),
        "mcp_tool_call" => (
            format!(
                "mcp:{}/{}",
                get_str(item, "server").unwrap_or("?"),
                get_str(item, "tool").unwrap_or("?")
            ),
            String::new(),
        ),
        _ => (
            "web_search".into(),
            get_str(item, "query").unwrap_or("").into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    fn texts(events: &[AiEvent]) -> String {
        events
            .iter()
            .filter_map(|e| match e {
                AiEvent::TextDelta { text } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn claude_plain_text_stream() {
        let ev = parse_stream(Backend::Claude, &fixture("claude_text_ok.jsonl"));
        assert!(matches!(ev[0], AiEvent::Started { .. }));
        assert_eq!(
            texts(&ev),
            "OK",
            "streamed once, not duplicated by the assistant message"
        );
        match ev.last().unwrap() {
            AiEvent::Final { text, structured } => {
                assert_eq!(text, "OK");
                assert!(structured.is_none());
            }
            other => panic!("expected Final, got {other:?}"),
        }
        let usage = ev.iter().find_map(|e| {
            if let AiEvent::Usage(u) = e {
                Some(u)
            } else {
                None
            }
        });
        assert!(usage.unwrap().cost_usd.unwrap() > 0.0);
    }

    #[test]
    fn claude_structured_output_hides_internal_tool() {
        let ev = parse_stream(Backend::Claude, &fixture("claude_structured_ok.jsonl"));
        assert!(!ev.iter().any(|e| matches!(e, AiEvent::ToolUse(_))));
        match ev.last().unwrap() {
            AiEvent::Final { structured, .. } => {
                assert_eq!(structured.as_ref().unwrap()["answer"], "OK");
            }
            other => panic!("expected Final, got {other:?}"),
        }
    }

    #[test]
    fn claude_api_error_becomes_error_event() {
        let ev = parse_stream(Backend::Claude, &fixture("claude_model_error.jsonl"));
        assert!(!ev.iter().any(|e| matches!(e, AiEvent::Final { .. })));
        match ev.last().unwrap() {
            AiEvent::Error { message } => assert!(message.contains("nonexistent-model-xyz")),
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn claude_tool_use_is_reported() {
        let ev = parse_stream(Backend::Claude, &fixture("claude_tools_synthetic.jsonl"));
        let tools: Vec<_> = ev
            .iter()
            .filter_map(|e| {
                if let AiEvent::ToolUse(t) = e {
                    Some(t)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "Read");
        assert!(tools[0].summary.contains("notes.md"));
        assert_eq!(texts(&ev), "Done");
    }

    #[test]
    fn codex_auth_failure_emits_single_error() {
        let ev = parse_stream(Backend::Codex, &fixture("codex_auth_error.jsonl"));
        let errors: Vec<_> = ev
            .iter()
            .filter(|e| matches!(e, AiEvent::Error { .. }))
            .collect();
        assert_eq!(
            errors.len(),
            1,
            "error + turn.failed must be deduplicated: {ev:?}"
        );
        assert!(ev.iter().any(|e| matches!(e, AiEvent::Notice { .. })));
        assert!(!ev.iter().any(|e| matches!(e, AiEvent::Final { .. })));
    }

    #[test]
    fn codex_success_stream_with_tools_and_json() {
        let ev = parse_stream(Backend::Codex, &fixture("codex_ok_synthetic.jsonl"));
        let tools: Vec<_> = ev
            .iter()
            .filter_map(|e| {
                if let AiEvent::ToolUse(t) = e {
                    Some(t)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(tools.len(), 1, "started+completed must dedupe");
        assert_eq!(tools[0].name, "shell");
        match ev.last().unwrap() {
            AiEvent::Final { text, structured } => {
                assert!(text.contains("answer"));
                assert_eq!(structured.as_ref().unwrap()["answer"], "OK");
            }
            other => panic!("expected Final, got {other:?}"),
        }
        let usage = ev.iter().find_map(|e| {
            if let AiEvent::Usage(u) = e {
                Some(u)
            } else {
                None
            }
        });
        assert_eq!(usage.unwrap().output_tokens, 17);
    }

    #[test]
    fn garbage_and_unknown_lines_are_ignored() {
        for b in Backend::ALL {
            let ev = parse_stream(b, "not json\n{\"type\":\"future.event\"}\n\n[1,2]\n");
            assert!(ev.is_empty());
        }
    }

    #[test]
    fn extract_json_handles_fences_and_prose() {
        assert_eq!(extract_json("{\"a\":1}").unwrap()["a"], 1);
        assert_eq!(extract_json("```json\n{\"a\":2}\n```").unwrap()["a"], 2);
        let v = extract_json("Sure! Here you go: {\"a\":\"}{\",\"b\":[1,{\"c\":3}]} hope it helps")
            .unwrap();
        assert_eq!(v["b"][1]["c"], 3);
        assert!(extract_json("no json here").is_none());
        assert!(extract_json("\"just a string\"").is_none());
        assert!(extract_json("{unterminated").is_none());
    }
}
