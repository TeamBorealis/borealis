//! Unified event model produced by every backend parser.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Maximum characters kept from a tool input/command summary.
const SUMMARY_MAX_CHARS: usize = 200;

/// A tool invocation the agent made (informational; tools are off by default).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolUseInfo {
    /// Backend tool-call id, when provided.
    pub id: Option<String>,
    /// Tool name (`Read`, `shell`, `mcp:server/tool`...).
    pub name: String,
    /// Short, truncated description of the input.
    pub summary: String,
}

impl ToolUseInfo {
    pub(crate) fn new(id: Option<String>, name: &str, summary: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            summary: truncate_chars(summary, SUMMARY_MAX_CHARS),
        }
    }
}

/// Token and cost accounting for a run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    /// Prompt tokens (including cache reads where the backend reports them separately).
    pub input_tokens: u64,
    /// Completion tokens.
    pub output_tokens: u64,
    /// Cached prompt tokens, when reported.
    pub cached_input_tokens: Option<u64>,
    /// Cost in USD, when the backend reports it (Claude does, Codex does not).
    pub cost_usd: Option<f64>,
}

impl Usage {
    /// Add another usage record (used to total repair-loop attempts).
    pub fn add(&self, other: &Usage) -> Usage {
        Usage {
            input_tokens: self.input_tokens + other.input_tokens,
            output_tokens: self.output_tokens + other.output_tokens,
            cached_input_tokens: match (self.cached_input_tokens, other.cached_input_tokens) {
                (None, None) => None,
                (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
            },
            cost_usd: match (self.cost_usd, other.cost_usd) {
                (None, None) => None,
                (a, b) => Some(a.unwrap_or(0.0) + b.unwrap_or(0.0)),
            },
        }
    }
}

/// One step of an AI session, identical for every backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AiEvent {
    /// The backend session began.
    Started {
        /// Backend session/thread id.
        session_id: Option<String>,
        /// Model in use, when reported.
        model: Option<String>,
    },
    /// A chunk of assistant text (a token-level delta for Claude, a whole message for Codex).
    TextDelta {
        /// The text.
        text: String,
    },
    /// The agent used a tool.
    ToolUse(ToolUseInfo),
    /// Token/cost accounting (emitted once, just before `Final`).
    Usage(Usage),
    /// Non-fatal warning from the CLI.
    Notice {
        /// The message.
        message: String,
    },
    /// The run finished successfully.
    Final {
        /// Full final text.
        text: String,
        /// Structured output when a schema was requested and the output parsed as JSON.
        structured: Option<Value>,
    },
    /// The run failed.
    Error {
        /// Redacted message.
        message: String,
    },
}

impl AiEvent {
    /// Redact user paths in the parts the UI shows as status or errors (notices, errors, tool
    /// names and summaries). Model output (`TextDelta`, `Final`) is left exactly as produced.
    pub fn redacted(self) -> AiEvent {
        use crate::redact_paths as r;
        match self {
            AiEvent::Notice { message } => AiEvent::Notice {
                message: r(&message),
            },
            AiEvent::Error { message } => AiEvent::Error {
                message: r(&message),
            },
            AiEvent::ToolUse(t) => AiEvent::ToolUse(ToolUseInfo {
                id: t.id,
                name: r(&t.name),
                summary: r(&t.summary),
            }),
            other => other,
        }
    }
}

pub(crate) fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('\u{2026}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redaction_covers_status_but_not_model_output() {
        let tool = AiEvent::ToolUse(ToolUseInfo::new(
            Some("1".into()),
            "shell",
            "cat /Users/victim/.ssh/id_ed25519",
        ));
        match tool.redacted() {
            AiEvent::ToolUse(t) => assert_eq!(t.summary, "cat ~/.ssh/id_ed25519"),
            _ => unreachable!(),
        }
        let notice = AiEvent::Notice {
            message: "reading /home/victim/x".into(),
        };
        assert_eq!(
            notice.redacted(),
            AiEvent::Notice {
                message: "reading ~/x".into()
            }
        );
        let text = AiEvent::TextDelta {
            text: "/Users/victim/a".into(),
        };
        assert_eq!(text.clone().redacted(), text);
    }

    #[test]
    fn truncates_on_char_boundaries() {
        let s = "\u{e9}".repeat(300);
        let t = truncate_chars(&s, 10);
        assert_eq!(t.chars().count(), 11);
    }

    #[test]
    fn usage_add_sums_and_keeps_none() {
        let a = Usage {
            input_tokens: 1,
            output_tokens: 2,
            cached_input_tokens: None,
            cost_usd: None,
        };
        let b = Usage {
            input_tokens: 3,
            output_tokens: 4,
            cached_input_tokens: Some(5),
            cost_usd: Some(0.5),
        };
        let c = a.add(&b);
        assert_eq!((c.input_tokens, c.output_tokens), (4, 6));
        assert_eq!(c.cached_input_tokens, Some(5));
        assert_eq!(c.cost_usd, Some(0.5));
        assert_eq!(a.add(&a).cost_usd, None);
    }

    #[test]
    fn events_serialize_with_type_tag() {
        let e = AiEvent::TextDelta { text: "hi".into() };
        assert_eq!(serde_json::to_value(&e).unwrap()["type"], "text_delta");
        let e = AiEvent::ToolUse(ToolUseInfo::new(None, "Read", "x"));
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["type"], "tool_use");
        assert_eq!(v["name"], "Read");
        let back: AiEvent = serde_json::from_value(v).unwrap();
        assert_eq!(back, e);
    }
}
