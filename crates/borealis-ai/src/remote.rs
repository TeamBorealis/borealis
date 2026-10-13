//! Client for the LAN AI host; usable from iOS/Android builds (no process spawning).
//!
//! Speaks HTTPS with rustls and trusts exactly one certificate: the SHA-256 fingerprint from the
//! pairing string (`borealis-pair:v2?...`). Store [`PairedHost`] (pin + token) after pairing and
//! rebuild the client with [`RemoteClient::from_paired`].

mod pin;

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

use crate::protocol::{
    CertFingerprint, ErrorBody, InfoResponse, PairRequest, PairResponse, PairingUri,
    PairingUriError, SseParser, PATH_INFO, PATH_PAIR, PATH_TASKS, PATH_UNPAIR, SSE_EVENT_AI,
    SSE_EVENT_ERROR, SSE_EVENT_RESULT,
};
use crate::redact_paths;
use crate::task::request::{TaskRequest, TaskResponse};
use crate::{AiError, AiEvent};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// The host sends a keep-alive every 15 s, so silence longer than this means a dead link.
const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const STREAM_TOTAL_TIMEOUT: Duration = Duration::from_secs(900);

/// Errors from the remote client.
#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    /// The host presented a certificate other than the pinned one (possible impersonation, or
    /// the desktop's identity was reset). Pair again.
    #[error("the host's certificate does not match the paired fingerprint")]
    PinMismatch,
    /// The pairing string could not be parsed.
    #[error(transparent)]
    PairingUri(#[from] PairingUriError),
    /// Transport, protocol or host-side failure.
    #[error(transparent)]
    Ai(#[from] AiError),
}

impl From<RemoteError> for AiError {
    fn from(e: RemoteError) -> Self {
        match e {
            RemoteError::Ai(e) => e,
            other => AiError::Transport(other.to_string()),
        }
    }
}

/// What a phone stores after pairing (keep `token` in the platform keychain).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedHost {
    /// Host address.
    pub addr: SocketAddr,
    /// Pinned certificate fingerprint.
    pub fingerprint: CertFingerprint,
    /// Bearer token.
    pub token: String,
    /// Device id assigned by the host.
    pub device_id: String,
    /// Token expiry (unix seconds); pair again afterwards.
    pub expires_at: u64,
}

/// HTTPS client bound to one pinned host.
#[derive(Debug, Clone)]
pub struct RemoteClient {
    addr: SocketAddr,
    pin: CertFingerprint,
    base: String,
    token: Option<String>,
    http: Client,
    mismatch: Arc<AtomicBool>,
}

fn transport(e: impl std::fmt::Display) -> AiError {
    AiError::Transport(redact_paths(&e.to_string()))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// A rustls client config that trusts only `pin` (TLS 1.3, aws-lc-rs). For callers that need
/// their own `reqwest` client: `Client::builder().tls_backend_preconfigured(cfg)`.
///
/// # Errors
/// Only if the crypto provider lacks TLS 1.3 (never with aws-lc-rs).
pub fn pinned_tls_config(pin: CertFingerprint) -> Result<rustls::ClientConfig, AiError> {
    pin::pinned_config(pin)
        .map(|(cfg, _)| cfg)
        .map_err(transport)
}

impl RemoteClient {
    /// Client for `https://<addr>` that accepts only the certificate with fingerprint `pin`.
    ///
    /// # Errors
    /// TLS or HTTP client construction failure.
    pub fn new(addr: SocketAddr, pin: CertFingerprint) -> Result<Self, RemoteError> {
        let (tls, mismatch) = pin::pinned_config(pin).map_err(transport)?;
        let http = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .tls_backend_preconfigured(tls)
            .build()
            .map_err(transport)?;
        Ok(Self {
            addr,
            pin,
            base: format!("https://{addr}"),
            token: None,
            http,
            mismatch,
        })
    }

    /// Rebuild a client from a stored pairing.
    ///
    /// # Errors
    /// As [`new`](Self::new).
    pub fn from_paired(p: &PairedHost) -> Result<Self, RemoteError> {
        Ok(Self::new(p.addr, p.fingerprint)?.with_token(p.token.clone()))
    }

    /// Parse a pairing string, connect to the pinned host and pair. Returns the client and the
    /// record to store.
    ///
    /// # Errors
    /// [`RemoteError::PairingUri`], [`RemoteError::PinMismatch`] or a host error.
    pub async fn pair_with_uri(
        uri: &str,
        device_name: &str,
    ) -> Result<(Self, PairedHost), RemoteError> {
        let uri: PairingUri = uri.parse()?;
        let mut client = Self::new(uri.addr, uri.fingerprint)?;
        let resp = client.pair(&uri.code, device_name).await?;
        let paired = PairedHost {
            addr: uri.addr,
            fingerprint: uri.fingerprint,
            token: resp.token,
            device_id: resp.device_id,
            expires_at: unix_now().saturating_add(resp.expires_in_secs),
        };
        Ok((client, paired))
    }

    /// Use a previously stored token.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// The token obtained from [`pair`](Self::pair), if any.
    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// The pinned fingerprint.
    pub fn fingerprint(&self) -> CertFingerprint {
        self.pin
    }

    /// The host address.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn authed(&self, rb: reqwest::RequestBuilder) -> Result<reqwest::RequestBuilder, RemoteError> {
        match &self.token {
            Some(t) => Ok(rb.bearer_auth(t)),
            None => Err(AiError::Invalid("not paired: no token".into()).into()),
        }
    }

    /// Map a send error, reporting a pin mismatch seen during the handshake as a typed error.
    fn send_error(&self, e: reqwest::Error) -> RemoteError {
        if self.mismatch.swap(false, Ordering::SeqCst) {
            RemoteError::PinMismatch
        } else {
            transport(e).into()
        }
    }

    async fn send(&self, rb: reqwest::RequestBuilder) -> Result<reqwest::Response, RemoteError> {
        rb.send().await.map_err(|e| self.send_error(e))
    }

    /// Unauthenticated discovery (protocol version and name).
    pub async fn info(&self) -> Result<InfoResponse, RemoteError> {
        let resp = self
            .send(self.http.get(self.url(PATH_INFO)).timeout(REQUEST_TIMEOUT))
            .await?;
        Ok(read_json(resp).await?)
    }

    /// Exchange a pairing code for a token and remember it.
    pub async fn pair(
        &mut self,
        code: &str,
        device_name: &str,
    ) -> Result<PairResponse, RemoteError> {
        let body = PairRequest {
            code: code.trim().to_string(),
            device_name: device_name.to_string(),
        };
        let rb = self
            .http
            .post(self.url(PATH_PAIR))
            .timeout(REQUEST_TIMEOUT)
            .json(&body);
        let out: PairResponse = read_json(self.send(rb).await?).await?;
        self.token = Some(out.token.clone());
        Ok(out)
    }

    /// Revoke this device's token on the host.
    pub async fn unpair(&mut self) -> Result<(), RemoteError> {
        let rb = self.authed(
            self.http
                .post(self.url(PATH_UNPAIR))
                .timeout(REQUEST_TIMEOUT),
        )?;
        let resp = self.send(rb).await?;
        if resp.status() == StatusCode::NO_CONTENT {
            self.token = None;
            return Ok(());
        }
        Err(error_from(resp).await.into())
    }

    /// Run a task on the host, forwarding progress events to `sink`.
    /// Dropping the returned future disconnects, which makes the host kill the CLI.
    pub async fn run_task(
        &self,
        req: &TaskRequest,
        sink: Option<&UnboundedSender<AiEvent>>,
    ) -> Result<TaskResponse, RemoteError> {
        let rb = self.authed(self.http.post(self.url(PATH_TASKS)).json(req))?;
        let resp = self.send(rb).await?;
        if !resp.status().is_success() {
            return Err(error_from(resp).await.into());
        }
        let out = tokio::time::timeout(STREAM_TOTAL_TIMEOUT, read_stream(resp, sink))
            .await
            .map_err(|_| AiError::Timeout {
                kind: crate::error::TimeoutKind::Overall,
                after: STREAM_TOTAL_TIMEOUT,
            })?;
        Ok(out?)
    }
}

async fn read_stream(
    resp: reqwest::Response,
    sink: Option<&UnboundedSender<AiEvent>>,
) -> Result<TaskResponse, AiError> {
    let mut parser = SseParser::default();
    let mut bytes = resp.bytes_stream();
    loop {
        let chunk = match tokio::time::timeout(STREAM_IDLE_TIMEOUT, bytes.next()).await {
            Err(_) => {
                return Err(AiError::Timeout {
                    kind: crate::error::TimeoutKind::Idle,
                    after: STREAM_IDLE_TIMEOUT,
                })
            }
            Ok(None) => {
                return Err(AiError::Transport(
                    "the host closed the stream without a result".into(),
                ))
            }
            Ok(Some(c)) => c.map_err(transport)?,
        };
        for frame in parser.push(&chunk) {
            match frame.event.as_str() {
                SSE_EVENT_AI => {
                    if let (Some(s), Ok(ev)) = (sink, serde_json::from_str::<AiEvent>(&frame.data))
                    {
                        let _ = s.send(ev);
                    }
                }
                SSE_EVENT_RESULT => {
                    return serde_json::from_str(&frame.data)
                        .map_err(|e| AiError::Transport(format!("bad result: {e}")));
                }
                SSE_EVENT_ERROR => {
                    let body: ErrorBody = serde_json::from_str(&frame.data)
                        .unwrap_or_else(|_| ErrorBody::new("task_failed", frame.data.clone()));
                    return Err(AiError::Transport(format!(
                        "{}: {}",
                        body.error, body.message
                    )));
                }
                _ => {}
            }
        }
    }
}

async fn read_json<T: serde::de::DeserializeOwned>(resp: reqwest::Response) -> Result<T, AiError> {
    if !resp.status().is_success() {
        return Err(error_from(resp).await);
    }
    resp.json::<T>().await.map_err(transport)
}

async fn error_from(resp: reqwest::Response) -> AiError {
    let status = resp.status();
    match resp.json::<ErrorBody>().await {
        Ok(b) => AiError::Transport(format!("{} ({}): {}", b.error, status.as_u16(), b.message)),
        Err(_) => AiError::Transport(format!("host returned HTTP {}", status.as_u16())),
    }
}
