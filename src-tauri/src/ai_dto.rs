//! AI contract types (camelCase on the wire) and the key-case helpers used for task results.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Task modes the UI can start (`AiMode` in the contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiMode {
    /// Free chat (streamed text).
    Chat,
    /// Homebrew element drafts.
    Homebrew,
    /// Rules question with citations.
    RulesQa,
    /// Level-up advisor.
    BuildAdvisor,
    /// Backstory text.
    Backstory,
    /// "Explain this number".
    Explain,
    /// NPC generator.
    Npc,
}

/// One chat turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiMessage {
    /// `user` or `assistant`.
    pub role: String,
    /// Message text.
    pub text: String,
}

/// `AiTaskRequest`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTaskRequest {
    /// What to run.
    pub mode: AiMode,
    /// The user's text (question, goals, stat key, request text).
    #[serde(default)]
    pub prompt: String,
    /// The character the task is about.
    #[serde(default)]
    pub character_id: Option<String>,
    /// Earlier chat turns.
    #[serde(default)]
    pub history: Vec<AiMessage>,
    /// Mode specific parameters (`targetLevel`, `band`, `partyIds`, `constraints`, `tone`, ...).
    #[serde(default)]
    pub params: Option<Value>,
}

/// `AiTaskHandle`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTaskHandle {
    /// Id carried by every event of the task.
    pub task_id: String,
}

/// A provider found on this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDto {
    /// `claude` or `codex`.
    pub backend: String,
    /// Executable path.
    pub path: String,
    /// Reported version.
    pub version: Option<String>,
    /// How it was found (`override`, `path`, `known_location`).
    pub source: String,
}

/// `AiStatus`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatusDto {
    /// `ready`, `unavailable` or `needs_pairing`.
    pub availability: String,
    /// The provider that tasks will use (`none` when unavailable).
    pub provider: String,
    /// Its version string.
    pub version: Option<String>,
    /// `local` CLI or paired `desktop` host.
    pub host: Option<String>,
    /// Human readable explanation.
    pub detail: String,
    /// Every provider detected (desktop).
    pub providers: Vec<ProviderDto>,
}

/// An event on `borealis://ai-event`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum AiWireEvent {
    /// The task began.
    Started {
        /// Task id.
        task_id: String,
        /// Mode.
        mode: AiMode,
    },
    /// Appended text.
    Delta {
        /// Task id.
        task_id: String,
        /// Text chunk.
        text: String,
    },
    /// The model used a tool.
    Tool {
        /// Task id.
        task_id: String,
        /// Tool name.
        name: String,
        /// Short description.
        summary: String,
    },
    /// The typed result: `{ kind, data }`.
    Result {
        /// Task id.
        task_id: String,
        /// Result payload.
        result: Value,
    },
    /// The task failed.
    Error {
        /// Task id.
        task_id: String,
        /// Message.
        message: String,
    },
    /// Final event of every task.
    Done {
        /// Task id.
        task_id: String,
        /// Whether `ai_cancel_task` ended it.
        cancelled: bool,
    },
}

/// Receives AI events (Tauri emitter in the app, a recorder in tests).
pub trait AiSink: Send + Sync {
    /// Deliver one event.
    fn emit(&self, event: &AiWireEvent);
}

/// A paired device as shown to the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostDeviceDto {
    /// Device id (used to revoke).
    pub id: String,
    /// Name the device gave.
    pub name: String,
}

/// `AiHostStatus`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiHostStatus {
    /// Whether the LAN host is running.
    pub enabled: bool,
    /// Whether this build can host at all (false on mobile).
    pub supported: bool,
    /// `ip:port` the phone connects to.
    pub address: Option<String>,
    /// Eight digit code while a pairing window is open.
    pub pairing_code: Option<String>,
    /// Seconds the code stays valid.
    pub pairing_expires_in_secs: Option<u64>,
    /// The `borealis-pair:` string for the QR code, only while a pairing window is open.
    pub pairing_uri: Option<String>,
    /// Short certificate fingerprint to compare on the phone, while the host runs.
    pub fingerprint: Option<String>,
    /// Paired devices.
    pub devices: Vec<HostDeviceDto>,
}

fn is_snake(key: &str) -> bool {
    key.contains('_')
        && key.starts_with(|c: char| c.is_ascii_lowercase())
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn to_camel(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    let mut up = false;
    for c in key.chars() {
        if c == '_' {
            up = true;
        } else if up {
            out.push(c.to_ascii_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn to_snake(key: &str) -> String {
    let plain = key.chars().all(|c| c.is_ascii_alphanumeric())
        && key.starts_with(|c: char| c.is_ascii_lowercase());
    if !plain || !key.chars().any(|c| c.is_ascii_uppercase()) {
        return key.to_string();
    }
    let mut out = String::with_capacity(key.len() + 4);
    for c in key.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn map_keys(value: Value, f: &dyn Fn(&str) -> String) -> Value {
    match value {
        Value::Object(m) => Value::Object(
            m.into_iter()
                .map(|(k, v)| (f(&k), map_keys(v, f)))
                .collect::<Map<_, _>>(),
        ),
        Value::Array(a) => Value::Array(a.into_iter().map(|v| map_keys(v, f)).collect()),
        other => other,
    }
}

/// Rename `snake_case` object keys to camelCase (ids and other keys are left alone).
#[must_use]
pub fn camelize(value: Value) -> Value {
    map_keys(value, &|k| {
        if is_snake(k) {
            to_camel(k)
        } else {
            k.to_string()
        }
    })
}

/// Inverse of [`camelize`] for plain camelCase keys.
#[must_use]
pub fn snakeize(value: Value) -> Value {
    map_keys(value, &to_snake)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keys_round_trip_and_ids_survive() {
        let v = json!({"answer_markdown": "x", "by_id": {"ID_2024_X": 1, "fighter:slot": 2}, "list": [{"up_to": 3}]});
        let c = camelize(v.clone());
        assert_eq!(c["answerMarkdown"], "x");
        assert_eq!(c["byId"]["ID_2024_X"], 1);
        assert_eq!(c["list"][0]["upTo"], 3);
        assert_eq!(snakeize(c), v);
    }

    #[test]
    fn events_use_kind_and_camel_task_id() {
        let e = AiWireEvent::Done {
            task_id: "t1".into(),
            cancelled: true,
        };
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({"kind": "done", "taskId": "t1", "cancelled": true})
        );
    }
}
