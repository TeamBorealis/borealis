//! LAN "AI host": lets a paired phone/tablet use the desktop's local CLI.
//!
//! Disabled by default: nothing listens until [`AiHost::start`] is called. Serves HTTPS (TLS 1.3)
//! with a self-signed certificate ([`HostIdentity`]) whose SHA-256 fingerprint travels in the
//! pairing string, so clients pin it. Clients pair with a short-lived eight-digit code and receive
//! a random bearer token that expires; only a SHA-256 hash of each token is kept.

mod identity;
mod serve;

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use axum::extract::{ConnectInfo, DefaultBodyLimit, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use borealis_link::limits::{Guess, GuessLimits, PairingState, RateLimiter};
use borealis_link::random::{hex, sha256_hex};
use futures_util::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, Semaphore};
use tokio::task::JoinHandle;
use tower_http::timeout::TimeoutLayer;

pub use identity::{write_private_file, HostIdentity, CERT_FILE, KEY_FILE};

use crate::protocol::{
    CertFingerprint, ErrorBody, InfoResponse, PairRequest, PairResponse, PairingUri, PATH_INFO,
    PATH_PAIR, PATH_TASKS, PATH_UNPAIR, PROTOCOL_VERSION, SSE_EVENT_AI, SSE_EVENT_ERROR,
    SSE_EVENT_RESULT,
};
use crate::redact_paths;
use crate::task::request::{TaskRequest, TaskResponse};
use crate::task::{AiRunner, ContentValidator, RunOptions, DEFAULT_MAX_REPAIRS};
use crate::{AiError, AiEvent};

const SSE_KEEPALIVE: Duration = Duration::from_secs(15);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(3);
const MAX_DEVICE_NAME_CHARS: usize = 64;
const MAX_IDLE_CHECK: Duration = Duration::from_secs(30);
const MIN_IDLE_CHECK: Duration = Duration::from_millis(10);
const DAY: u64 = 24 * 60 * 60;

/// Host settings.
#[derive(Debug, Clone)]
pub struct HostConfig {
    /// Explicit bind address (opt-in override, e.g. `0.0.0.0:41234`). `None` (default) binds the
    /// detected LAN IPv4 ([`detect_lan_ip`]) on [`port`](Self::port) and fails if there is none.
    pub bind: Option<SocketAddr>,
    /// Port used with the detected LAN address (0 = OS-assigned).
    pub port: u16,
    /// Display name returned by `/v1/info`.
    pub name: String,
    /// Concurrent task sessions (clamped to 1..=2).
    pub max_concurrent: usize,
    /// Task requests per device per minute.
    pub rate_limit_per_minute: u32,
    /// Maximum request body size.
    pub max_body_bytes: usize,
    /// How long a pairing code stays valid.
    pub pairing_ttl: Duration,
    /// Wrong guesses one source IP may make before it is locked out of the current code.
    pub max_pairing_attempts: u32,
    /// Wrong guesses from all sources before the current code is destroyed.
    pub max_pairing_failures_total: u32,
    /// Pairing requests per source IP per minute.
    pub pair_requests_per_ip_per_minute: u32,
    /// Pairing requests per minute from everyone (generous ceiling).
    pub pair_requests_global_per_minute: u32,
    /// Maximum paired devices.
    pub max_devices: usize,
    /// Repair attempts per task.
    pub max_repairs: u32,
    /// Lifetime of a device token; expired tokens are rejected and pruned.
    pub token_ttl: Duration,
    /// Stop serving after this long without requests or running sessions (`ZERO` = never).
    pub idle_timeout: Duration,
    /// Time allowed until response headers (SSE bodies stream past it).
    pub request_timeout: Duration,
    /// Open connections, total.
    pub max_connections: usize,
    /// Open connections per source IP.
    pub max_connections_per_ip: usize,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            bind: None,
            port: 0,
            name: "Borealis desktop".to_string(),
            max_concurrent: 1,
            rate_limit_per_minute: 30,
            max_body_bytes: 256 * 1024,
            pairing_ttl: Duration::from_secs(300),
            max_pairing_attempts: 5,
            max_pairing_failures_total: 50,
            pair_requests_per_ip_per_minute: 10,
            pair_requests_global_per_minute: 120,
            max_devices: 8,
            max_repairs: DEFAULT_MAX_REPAIRS,
            token_ttl: Duration::from_secs(30 * DAY),
            idle_timeout: Duration::from_secs(30 * 60),
            request_timeout: Duration::from_secs(30),
            max_connections: 32,
            max_connections_per_ip: 8,
        }
    }
}

/// A pairing window to display to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingCode {
    /// Eight digits.
    pub code: String,
    /// Validity window.
    pub valid_for: Duration,
    /// Full pairing string (render as QR and as copyable text).
    pub uri: PairingUri,
}

/// Persistable record of a paired device (token hash only, never the token).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRecord {
    /// Short device id.
    pub id: String,
    /// Name the device gave when pairing.
    pub name: String,
    /// Hex SHA-256 of the bearer token.
    pub token_hash: String,
    /// Token expiry (unix seconds). Records without it are treated as expired.
    #[serde(default)]
    pub expires_at: u64,
}

impl DeviceRecord {
    /// Whether the token is past its expiry.
    pub fn is_expired(&self) -> bool {
        unix_now() >= self.expires_at
    }
}

/// Called with the full device list whenever it changes (pair, unpair, revoke, expiry prune).
pub type DevicesHook = Arc<dyn Fn(&[DeviceRecord]) + Send + Sync>;

struct HostState {
    config: HostConfig,
    runner: Arc<dyn AiRunner>,
    validator: Option<Arc<dyn ContentValidator>>,
    public_addr: SocketAddr,
    fingerprint: CertFingerprint,
    pairing: Mutex<Option<PairingState>>,
    devices: Mutex<HashMap<String, DeviceRecord>>, // keyed by token hash
    devices_hook: Mutex<Option<DevicesHook>>,
    sessions: Arc<Semaphore>,
    max_sessions: usize,
    task_limiter: RateLimiter,
    pair_ip_limiter: RateLimiter,
    pair_global_limiter: RateLimiter,
    last_activity: Mutex<Instant>,
    stopped: AtomicBool,
}

impl HostState {
    fn touch(&self) {
        *lock(&self.last_activity) = Instant::now();
    }

    fn idle_expired(&self) -> bool {
        let limit = self.config.idle_timeout;
        if limit.is_zero() {
            return false;
        }
        if self.sessions.available_permits() < self.max_sessions {
            self.touch(); // a task is running
            return false;
        }
        lock(&self.last_activity).elapsed() >= limit
    }

    fn mark_stopped(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }

    fn sorted_devices(&self) -> Vec<DeviceRecord> {
        let mut v: Vec<_> = lock(&self.devices).values().cloned().collect();
        v.sort_by(|a, b| a.id.cmp(&b.id));
        v
    }

    /// Run the persistence hook with the current list (never while holding the devices lock).
    fn devices_changed(&self) {
        let hook = lock(&self.devices_hook).clone();
        if let Some(hook) = hook {
            hook(&self.sorted_devices());
        }
    }

    /// Drop expired tokens; returns whether anything was removed (the hook has then run).
    fn prune_expired(&self) -> bool {
        let removed = {
            let mut d = lock(&self.devices);
            let before = d.len();
            d.retain(|_, rec| !rec.is_expired());
            d.len() != before
        };
        if removed {
            self.devices_changed();
        }
        removed
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn random_bytes<const N: usize>() -> Result<[u8; N], AiError> {
    borealis_link::random::random_bytes::<N>().map_err(|e| AiError::Transport(e.to_string()))
}

/// Uniform eight-digit code via rejection sampling.
fn random_code() -> Result<String, AiError> {
    borealis_link::pairing::random_pairing_code().map_err(|e| AiError::Transport(e.to_string()))
}

/// Best-effort LAN IPv4 address of this machine (no packets are sent).
pub fn detect_lan_ip() -> Option<IpAddr> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("192.0.2.1:9").ok()?; // TEST-NET-1; connect() only selects a route
    let ip = sock.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

fn resolve_bind(config: &HostConfig) -> Result<SocketAddr, AiError> {
    if let Some(addr) = config.bind {
        return Ok(addr);
    }
    let ip = detect_lan_ip().ok_or_else(|| {
        AiError::Transport("no LAN address found; connect to a network first".into())
    })?;
    Ok(SocketAddr::new(ip, config.port))
}

/// Entry point; construct a running host with [`AiHost::start`].
pub struct AiHost;

impl AiHost {
    /// Bind and start serving HTTPS. Nothing is paired yet; call [`HostHandle::begin_pairing`].
    /// Expired `known_devices` are dropped.
    ///
    /// # Errors
    /// No LAN address (default bind), bind failure, or an unusable identity.
    pub async fn start(
        config: HostConfig,
        identity: &HostIdentity,
        runner: Arc<dyn AiRunner>,
        validator: Option<Arc<dyn ContentValidator>>,
        known_devices: Vec<DeviceRecord>,
    ) -> Result<HostHandle, AiError> {
        let acceptor = tokio_rustls::TlsAcceptor::from(identity.server_config()?);
        let bind = resolve_bind(&config)?;
        let listener = tokio::net::TcpListener::bind(bind)
            .await
            .map_err(AiError::io)?;
        let addr = listener.local_addr().map_err(AiError::io)?;
        let public_ip = if addr.ip().is_unspecified() {
            detect_lan_ip().unwrap_or(addr.ip())
        } else {
            addr.ip()
        };
        let max_sessions = config.max_concurrent.clamp(1, 2);
        let state = Arc::new(HostState {
            runner,
            validator,
            public_addr: SocketAddr::new(public_ip, addr.port()),
            fingerprint: identity.fingerprint(),
            pairing: Mutex::new(None),
            devices: Mutex::new(
                known_devices
                    .into_iter()
                    .filter(|d| !d.is_expired())
                    .map(|d| (d.token_hash.clone(), d))
                    .collect(),
            ),
            devices_hook: Mutex::new(None),
            sessions: Arc::new(Semaphore::new(max_sessions)),
            max_sessions,
            task_limiter: RateLimiter::new(config.rate_limit_per_minute),
            pair_ip_limiter: RateLimiter::new(config.pair_requests_per_ip_per_minute),
            pair_global_limiter: RateLimiter::new(config.pair_requests_global_per_minute),
            last_activity: Mutex::new(Instant::now()),
            stopped: AtomicBool::new(false),
            config,
        });
        let app = Router::new()
            .route(PATH_INFO, get(get_info))
            .route(PATH_PAIR, post(post_pair))
            .route(PATH_TASKS, post(post_tasks))
            .route(PATH_UNPAIR, post(post_unpair))
            .layer(DefaultBodyLimit::max(state.config.max_body_bytes))
            .layer(TimeoutLayer::with_status_code(
                StatusCode::REQUEST_TIMEOUT,
                state.config.request_timeout,
            ))
            .with_state(Arc::clone(&state));
        let limits = serve::ConnLimits {
            total: state.config.max_connections,
            per_ip: state.config.max_connections_per_ip.max(1),
            idle_check: if state.config.idle_timeout.is_zero() {
                MAX_IDLE_CHECK
            } else {
                (state.config.idle_timeout / 4).clamp(MIN_IDLE_CHECK, MAX_IDLE_CHECK)
            },
        };
        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        // Dropping `stop_tx` (handle dropped) also ends the loop.
        let server = tokio::spawn(serve::run(
            listener,
            acceptor,
            app,
            Arc::clone(&state),
            limits,
            stop_rx,
        ));
        tracing::info!(%addr, "ai host listening (tls)");
        Ok(HostHandle {
            addr,
            state,
            stop: Some(stop_tx),
            server: Some(server),
        })
    }
}

/// Control handle for a running host. Dropping it stops the server.
pub struct HostHandle {
    addr: SocketAddr,
    state: Arc<HostState>,
    stop: Option<oneshot::Sender<()>>,
    server: Option<JoinHandle<()>>,
}

impl HostHandle {
    /// The bound address (port is the OS-assigned one when port 0 was requested).
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Address advertised in pairing strings (the LAN IP when bound to an unspecified address).
    pub fn public_addr(&self) -> SocketAddr {
        self.state.public_addr
    }

    /// The pinned certificate fingerprint.
    pub fn fingerprint(&self) -> CertFingerprint {
        self.state.fingerprint
    }

    /// `false` once the server stopped (shutdown or idle timeout).
    pub fn is_running(&self) -> bool {
        !self.state.stopped.load(Ordering::SeqCst)
            && self.server.as_ref().is_some_and(|s| !s.is_finished())
    }

    /// Persist the device list on every change. Set it right after [`AiHost::start`].
    pub fn set_devices_hook(&self, hook: DevicesHook) {
        *lock(&self.state.devices_hook) = Some(hook);
    }

    /// Open a pairing window and return the code and pairing string. Replaces any earlier code.
    pub fn begin_pairing(&self) -> Result<PairingCode, AiError> {
        let code = random_code()?;
        let ttl = self.state.config.pairing_ttl;
        *lock(&self.state.pairing) = Some(PairingState::new(code.clone(), ttl));
        self.state.touch();
        let uri = PairingUri {
            addr: self.state.public_addr,
            code: code.clone(),
            fingerprint: self.state.fingerprint,
        };
        Ok(PairingCode {
            code,
            valid_for: ttl,
            uri,
        })
    }

    /// Close the pairing window.
    pub fn cancel_pairing(&self) {
        *lock(&self.state.pairing) = None;
    }

    /// Paired, unexpired devices (safe to persist; contains hashes only).
    pub fn devices(&self) -> Vec<DeviceRecord> {
        self.state.prune_expired();
        self.state.sorted_devices()
    }

    /// Revoke a device by id. Returns whether it existed. Runs the devices hook.
    pub fn revoke(&self, device_id: &str) -> bool {
        let removed = {
            let mut d = lock(&self.state.devices);
            let before = d.len();
            d.retain(|_, rec| rec.id != device_id);
            d.len() != before
        };
        if removed {
            self.state.devices_changed();
        }
        removed
    }

    /// Stop serving and wait (briefly) for open streams to end.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
        if let Some(mut server) = self.server.take() {
            let wait = SHUTDOWN_GRACE + Duration::from_secs(1);
            if tokio::time::timeout(wait, &mut server).await.is_err() {
                server.abort();
            }
        }
    }
}

impl Drop for HostHandle {
    fn drop(&mut self) {
        // Dropping `stop` ends the accept loop, which drains then aborts its connections.
        self.stop.take();
        if let Some(server) = self.server.take() {
            // Dropped outside a Tokio runtime (e.g. during unwind): spawning would panic, so
            // abort immediately instead.
            match tokio::runtime::Handle::try_current() {
                Ok(rt) => {
                    rt.spawn(async move {
                        tokio::time::sleep(SHUTDOWN_GRACE + Duration::from_secs(1)).await;
                        server.abort();
                    });
                }
                Err(_) => server.abort(),
            }
        }
    }
}

// ----------------------------------------------------------------------------- handlers

fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    (status, Json(ErrorBody::new(code, message))).into_response()
}

fn unauthorized(message: &str) -> Response {
    error_response(StatusCode::UNAUTHORIZED, "unauthorized", message)
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
}

/// Valid, unexpired device for the request's bearer token; expired tokens are pruned.
/// The error is the `unauthorized` message.
fn authenticate(state: &HostState, headers: &HeaderMap) -> Result<DeviceRecord, &'static str> {
    let token = bearer(headers).ok_or("missing token")?;
    let hash = sha256_hex(token);
    let dev = lock(&state.devices).get(&hash).cloned();
    match dev {
        Some(d) if d.is_expired() => {
            state.prune_expired();
            Err("token expired; pair again")
        }
        Some(d) => Ok(d),
        None => Err("unknown token"),
    }
}

/// Unauthenticated: protocol version and name only (no pairing state).
async fn get_info(State(st): State<Arc<HostState>>) -> Json<InfoResponse> {
    Json(InfoResponse {
        protocol: PROTOCOL_VERSION,
        name: st.config.name.clone(),
    })
}

fn pairing_failed() -> Response {
    error_response(
        StatusCode::FORBIDDEN,
        "pairing_failed",
        "pairing code invalid or expired",
    )
}

/// Check the code; consumes the window on success, the global ceiling, or expiry.
fn check_code(st: &HostState, ip: IpAddr, code: &str) -> Guess {
    let lim = GuessLimits {
        per_ip: st.config.max_pairing_attempts,
        total: st.config.max_pairing_failures_total,
    };
    let mut guard = lock(&st.pairing);
    let Some(p) = guard.as_mut() else {
        return Guess::Rejected;
    };
    let (outcome, close) = p.guess(ip, code, lim);
    if close {
        *guard = None;
    }
    outcome
}

fn issue_token(st: &HostState, device_name: &str) -> Result<PairResponse, AiError> {
    let token = hex(&random_bytes::<32>()?);
    let hash = sha256_hex(&token);
    let device_id = hash[..8].to_string();
    let name: String = device_name
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_DEVICE_NAME_CHARS)
        .collect();
    let ttl = st.config.token_ttl.as_secs();
    lock(&st.devices).insert(
        hash.clone(),
        DeviceRecord {
            id: device_id.clone(),
            name,
            token_hash: hash,
            expires_at: unix_now().saturating_add(ttl),
        },
    );
    st.devices_changed();
    Ok(PairResponse {
        token,
        device_id,
        expires_in_secs: ttl,
    })
}

async fn post_pair(
    State(st): State<Arc<HostState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    body: Bytes,
) -> Response {
    let ip = peer.ip().to_canonical(); // IPv4-mapped IPv6 counts as the IPv4 sender
    let ip_ok = st.pair_ip_limiter.allow(&ip.to_string());
    if !ip_ok || !st.pair_global_limiter.allow("all") {
        return error_response(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "too many pairing attempts",
        );
    }
    let Ok(req) = serde_json::from_slice::<PairRequest>(&body) else {
        return error_response(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "expected {code, device_name}",
        );
    };
    if check_code(&st, ip, req.code.trim()) != Guess::Accepted {
        return pairing_failed();
    }
    st.prune_expired();
    if lock(&st.devices).len() >= st.config.max_devices {
        return error_response(
            StatusCode::CONFLICT,
            "pairing_failed",
            "maximum number of paired devices reached",
        );
    }
    match issue_token(&st, &req.device_name) {
        Ok(r) => Json(r).into_response(),
        Err(e) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "task_failed",
            &e.to_string(),
        ),
    }
}

async fn post_unpair(State(st): State<Arc<HostState>>, headers: HeaderMap) -> Response {
    let dev = match authenticate(&st, &headers) {
        Ok(d) => d,
        Err(msg) => return unauthorized(msg),
    };
    lock(&st.devices).remove(&dev.token_hash);
    st.devices_changed();
    StatusCode::NO_CONTENT.into_response()
}

/// Messages from the task runner to the SSE writer.
enum Msg {
    Ai(AiEvent),
    Result(Box<TaskResponse>),
    Failed(ErrorBody),
}

async fn post_tasks(State(st): State<Arc<HostState>>, headers: HeaderMap, body: Bytes) -> Response {
    let dev = match authenticate(&st, &headers) {
        Ok(d) => d,
        Err(msg) => return unauthorized(msg),
    };
    if !st.task_limiter.allow(&dev.id) {
        let mut r = error_response(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "too many requests",
        );
        r.headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
        return r;
    }
    let req = match serde_json::from_slice::<TaskRequest>(&body) {
        Ok(r) => r,
        Err(e) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "bad_request",
                &format!("invalid task request: {e}"),
            )
        }
    };
    let Ok(permit) = Arc::clone(&st.sessions).try_acquire_owned() else {
        let mut r = error_response(
            StatusCode::TOO_MANY_REQUESTS,
            "busy",
            "the desktop is already running an AI session",
        );
        r.headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
        return r;
    };
    tracing::info!(task = req.name(), device = %dev.id, "remote ai task");

    let (tx, rx) = mpsc::unbounded_channel::<Msg>();
    let runner = Arc::clone(&st.runner);
    let validator = st.validator.clone();
    let max_repairs = st.config.max_repairs;
    let job = tokio::spawn(async move {
        let _permit = permit;
        let (ev_tx, mut ev_rx) = mpsc::unbounded_channel::<AiEvent>();
        let opts = RunOptions {
            max_repairs,
            sink: Some(&ev_tx),
        };
        let fut = req.execute(runner.as_ref(), validator, &opts);
        tokio::pin!(fut);
        let outcome = loop {
            tokio::select! {
                Some(ev) = ev_rx.recv() => { let _ = tx.send(Msg::Ai(ev)); }
                res = &mut fut => break res,
            }
        };
        while let Ok(ev) = ev_rx.try_recv() {
            let _ = tx.send(Msg::Ai(ev));
        }
        let _ = tx.send(match outcome {
            Ok(r) => Msg::Result(Box::new(r)),
            Err(e) => Msg::Failed(ErrorBody::new("task_failed", redact_paths(&e.to_string()))),
        });
    });
    Sse::new(sse_stream(rx, job))
        .keep_alive(KeepAlive::new().interval(SSE_KEEPALIVE))
        .into_response()
}

/// Aborts the task when the SSE stream is dropped (client disconnected), which kills the CLI.
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn sse_stream(
    rx: mpsc::UnboundedReceiver<Msg>,
    job: JoinHandle<()>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    stream::unfold(
        (rx, AbortOnDrop(job), false),
        |(mut rx, guard, done)| async move {
            if done {
                return None;
            }
            let msg = rx.recv().await?;
            let (event, terminal) = match msg {
                Msg::Ai(AiEvent::Final { .. } | AiEvent::Error { .. }) => {
                    // Attempt-level terminals are not final for the task (repair may follow);
                    // the task's own `result`/`error` event is authoritative.
                    return Some((Ok(Event::default().comment("attempt")), (rx, guard, false)));
                }
                Msg::Ai(ev) => (json_event(SSE_EVENT_AI, &ev), false),
                Msg::Result(r) => (json_event(SSE_EVENT_RESULT, &*r), true),
                Msg::Failed(e) => (json_event(SSE_EVENT_ERROR, &e), true),
            };
            Some((Ok(event), (rx, guard, terminal)))
        },
    )
}

fn json_event<T: Serialize>(name: &str, payload: &T) -> Event {
    Event::default()
        .event(name)
        .json_data(payload)
        .unwrap_or_else(|_| Event::default().event(SSE_EVENT_ERROR).data("{}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_eight_digits() {
        for _ in 0..200 {
            let c = random_code().unwrap();
            assert_eq!(c.len(), 8);
            assert!(c.bytes().all(|b| b.is_ascii_digit()));
        }
    }

    #[test]
    fn hashing_is_stable_hex() {
        assert_eq!(
            sha256_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn tokens_are_64_hex_chars_and_unique() {
        let a = hex(&random_bytes::<32>().unwrap());
        let b = hex(&random_bytes::<32>().unwrap());
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }

    #[test]
    fn records_without_expiry_are_expired() {
        let rec: DeviceRecord =
            serde_json::from_str(r#"{"id":"a","name":"n","token_hash":"h"}"#).unwrap();
        assert!(rec.is_expired());
    }

    #[test]
    fn default_bind_is_not_all_interfaces() {
        let cfg = HostConfig::default();
        assert!(cfg.bind.is_none());
        if let Ok(addr) = resolve_bind(&cfg) {
            assert!(!addr.ip().is_unspecified());
        }
    }
}
