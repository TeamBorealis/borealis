//! Wire types and helpers shared by the LAN AI host and its remote client.

use serde::{Deserialize, Serialize};

mod pairing;
pub use pairing::{
    CertFingerprint, PairingUri, PairingUriError, PAIRING_CODE_LEN, PAIRING_URI_PREFIX,
};

/// Protocol major version, returned by `GET /v1/info`. v2: TLS with a pinned certificate.
pub const PROTOCOL_VERSION: u32 = 2;
/// `GET`: unauthenticated host discovery.
pub const PATH_INFO: &str = "/v1/info";
/// `POST`: exchange a pairing code for a bearer token.
pub const PATH_PAIR: &str = "/v1/pair";
/// `POST`: run a task, response is an SSE stream.
pub const PATH_TASKS: &str = "/v1/tasks";
/// `POST`: revoke the calling token.
pub const PATH_UNPAIR: &str = "/v1/unpair";

/// SSE event carrying an [`crate::AiEvent`] (progress; never terminal).
pub const SSE_EVENT_AI: &str = "ai";
/// SSE event carrying the final [`crate::task::request::TaskResponse`] (terminal).
pub const SSE_EVENT_RESULT: &str = "result";
/// SSE event carrying an [`ErrorBody`] (terminal).
pub const SSE_EVENT_ERROR: &str = "error";

/// `GET /v1/info` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfoResponse {
    /// Protocol version.
    pub protocol: u32,
    /// Host display name.
    pub name: String,
}

/// `POST /v1/pair` request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairRequest {
    /// The eight-digit code from the pairing string.
    pub code: String,
    /// Name of the pairing device, shown in the desktop's device list.
    pub device_name: String,
}

/// `POST /v1/pair` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairResponse {
    /// Bearer token; store it in the platform keychain.
    pub token: String,
    /// Opaque device id (first 8 hex chars of the token hash).
    pub device_id: String,
    /// Seconds until the token expires; pair again afterwards.
    pub expires_in_secs: u64,
}

/// Error payload for HTTP errors and the terminal SSE `error` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    /// Machine-readable code: `unauthorized`, `bad_request`, `busy`, `rate_limited`,
    /// `pairing_failed`, `task_failed`.
    pub error: String,
    /// Human-readable, already path-redacted message.
    pub message: String,
}

impl ErrorBody {
    /// Build an error body.
    pub fn new(error: &str, message: impl Into<String>) -> Self {
        Self {
            error: error.to_string(),
            message: message.into(),
        }
    }
}

/// One parsed SSE frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseFrame {
    /// `event:` field (defaults to `message` per the SSE spec).
    pub event: String,
    /// Joined `data:` lines.
    pub data: String,
}

/// Incremental Server-Sent-Events parser (handles chunk splits, CRLF, comments).
#[derive(Debug, Default)]
pub struct SseParser {
    buf: String,
    event: Option<String>,
    data: Vec<String>,
}

impl SseParser {
    /// Feed bytes; returns every frame completed by them.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<SseFrame> {
        self.buf.push_str(&String::from_utf8_lossy(chunk));
        let mut frames = Vec::new();
        while let Some(pos) = self.buf.find('\n') {
            let mut line: String = self.buf.drain(..=pos).collect();
            line.truncate(line.trim_end_matches(['\n', '\r']).len());
            if line.is_empty() {
                if !self.data.is_empty() || self.event.is_some() {
                    frames.push(SseFrame {
                        event: self.event.take().unwrap_or_else(|| "message".to_string()),
                        data: std::mem::take(&mut self.data).join("\n"),
                    });
                }
            } else if line.starts_with(':') {
                // comment / keep-alive
            } else if let Some(v) = line.strip_prefix("event:") {
                self.event = Some(v.trim_start().to_string());
            } else if let Some(v) = line.strip_prefix("data:") {
                self.data.push(v.strip_prefix(' ').unwrap_or(v).to_string());
            }
        }
        frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frames_split_across_chunks_and_crlf() {
        let mut p = SseParser::default();
        let mut out = p.push(b"event: ai\r\ndata: {\"a\"");
        assert!(out.is_empty());
        out.extend(p.push(b":1}\r\n\r\n: keep-alive\n\nevent: result\ndata: x\ndata: y\n\n"));
        assert_eq!(out.len(), 2);
        assert_eq!(
            out[0],
            SseFrame {
                event: "ai".into(),
                data: "{\"a\":1}".into()
            }
        );
        assert_eq!(
            out[1],
            SseFrame {
                event: "result".into(),
                data: "x\ny".into()
            }
        );
    }

    #[test]
    fn default_event_name_is_message() {
        let mut p = SseParser::default();
        let out = p.push(b"data: hi\n\n");
        assert_eq!(out[0].event, "message");
    }

    #[test]
    fn error_body_roundtrip() {
        let b = ErrorBody::new("busy", "try later");
        let back: ErrorBody = serde_json::from_str(&serde_json::to_string(&b).unwrap()).unwrap();
        assert_eq!(back, b);
    }
}
