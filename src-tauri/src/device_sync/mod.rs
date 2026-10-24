//! Device transfer service (B2 + B3, feature `device-link`): Send/Receive sessions over the
//! device link, with QR or typed-code authentication, preview, accept and apply.
//!
//! The listener exists only while a Send/Receive dialog is open (SY3, manual only): it starts
//! with `start_send` / `start_receive`, accepts **one** authentication attempt, and closes when
//! the window expires (5 minutes), on cancel, or once a session started. No plaintext fallback.

mod bundle;
mod devices;
mod host;
pub mod inbox;
mod run;
pub mod validate;

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use borealis_link::auth::AuthWindow;
use borealis_link::SecureStore;
use borealis_transfer::Cancel;
use tokio::sync::oneshot;

use crate::content_packs::EventSink;
use crate::error::{ApiError, ApiResult, ErrorCode};
use crate::state::AppState;
use crate::sync_dto::{LinkPhase, LinkSelection, LinkStatus};

pub use bundle::{export_dto, incoming_dir, suggested_file_name};
pub use host::{sync_dir, OutgoingBundle};

/// Event name of session status updates.
pub const EVENT_LINK_STATUS: &str = "borealis://link-status";
/// Most sessions kept (finished ones are pruned first).
const MAX_SESSIONS: usize = 8;

/// Access to the app state from background tasks (the Tauri handle in the app, an `Arc` in tests).
pub trait StateRef: Send + Sync + 'static {
    /// The state.
    fn state(&self) -> &AppState;
}

impl StateRef for AppState {
    fn state(&self) -> &AppState {
        self
    }
}

/// Where session updates go.
pub trait SyncEvents: Send + Sync + 'static {
    /// A session changed.
    fn status(&self, status: &LinkStatus);
    /// Content reload notifications after packs arrived.
    fn content(&self) -> &dyn EventSink;
}

/// Static settings of the service.
#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// Address to listen on; `None` = the detected LAN address.
    pub bind_ip: Option<IpAddr>,
    /// This device's name as shown to the other device.
    pub device_name: String,
    /// Advertise / browse the nameplate over mDNS (desktop).
    pub discovery: bool,
}

type StoreFactory = Box<dyn Fn() -> Result<Arc<dyn SecureStore>, String> + Send + Sync>;

/// One session's controls and last status.
pub(crate) struct SessionCtl {
    pub cancel: Cancel,
    pub decision: Mutex<Option<oneshot::Sender<bool>>>,
    pub status: Mutex<LinkStatus>,
    pub window: Option<Arc<AuthWindow>>,
}

/// The service (managed by Tauri as `Arc<DeviceSync>`).
pub struct DeviceSync {
    pub(crate) state: Arc<dyn StateRef>,
    pub(crate) events: Arc<dyn SyncEvents>,
    pub(crate) config: SyncConfig,
    store_factory: StoreFactory,
    store: OnceLock<Arc<dyn SecureStore>>,
    sessions: Mutex<HashMap<String, Arc<SessionCtl>>>,
    bundles: bundle::OpenBundles,
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn new_session_id() -> ApiResult<String> {
    borealis_link::random::random_bytes::<8>()
        .map(|b| format!("s_{}", borealis_link::random::hex(&b)))
        .map_err(ApiError::internal)
}

impl DeviceSync {
    /// A service; `store` opens the secure store on first use (the OS keychain in the app).
    pub fn new(
        state: Arc<dyn StateRef>,
        events: Arc<dyn SyncEvents>,
        config: SyncConfig,
        store: impl Fn() -> Result<Arc<dyn SecureStore>, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            state,
            events,
            config,
            store_factory: Box::new(store),
            store: OnceLock::new(),
            sessions: Mutex::new(HashMap::new()),
            bundles: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn store(&self) -> ApiResult<Arc<dyn SecureStore>> {
        if let Some(s) = self.store.get() {
            return Ok(Arc::clone(s));
        }
        let store = (self.store_factory)().map_err(|e| {
            ApiError::new(
                ErrorCode::IoError,
                format!("Device transfer needs the system keychain, which is not available: {e}"),
            )
        })?;
        Ok(Arc::clone(self.store.get_or_init(|| store)))
    }

    fn insert(&self, ctl: Arc<SessionCtl>) -> ApiResult<()> {
        let mut sessions = lock(&self.sessions);
        if sessions.len() >= MAX_SESSIONS {
            sessions.retain(|_, s| !run::is_final(lock(&s.status).phase));
        }
        if sessions.len() >= MAX_SESSIONS {
            return Err(ApiError::invalid(
                "Too many transfers are running; finish or cancel one first.",
            ));
        }
        let id = lock(&ctl.status).session_id.clone();
        sessions.insert(id, ctl);
        Ok(())
    }

    fn get(&self, session_id: &str) -> ApiResult<Arc<SessionCtl>> {
        lock(&self.sessions)
            .get(session_id)
            .cloned()
            .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "No such transfer."))
    }

    /// Open a window to send `selection` (this device shows the QR / code).
    ///
    /// # Errors
    /// Invalid selection, no LAN address, keychain unavailable.
    pub async fn start_send(self: &Arc<Self>, selection: LinkSelection) -> ApiResult<LinkStatus> {
        let id = new_session_id()?;
        let outgoing =
            host::build_outgoing(self.state.state(), &selection, &self.config.device_name)?;
        run::listen(self, id, Some(outgoing)).await
    }

    /// Open a window to receive (this device shows the QR / code).
    ///
    /// # Errors
    /// No LAN address, keychain unavailable.
    pub async fn start_receive(self: &Arc<Self>) -> ApiResult<LinkStatus> {
        let id = new_session_id()?;
        run::listen(self, id, None).await
    }

    /// Connect to a window shown on another device: `target` is a scanned `borealis-link:v1?...`
    /// payload or a typed code; `host` (`ip:port`) is needed with a code where discovery is
    /// unavailable. With `selection` this device sends, without it it receives.
    ///
    /// # Errors
    /// Malformed payload / code, role mismatch, invalid selection.
    pub async fn connect(
        self: &Arc<Self>,
        target: &str,
        host: Option<&str>,
        selection: Option<LinkSelection>,
    ) -> ApiResult<LinkStatus> {
        let target = run::parse_target(target, host, selection.is_some())?;
        let id = new_session_id()?;
        let outgoing = match &selection {
            Some(sel) => Some(host::build_outgoing(
                self.state.state(),
                sel,
                &self.config.device_name,
            )?),
            None => None,
        };
        run::dial(self, id, target, outgoing)
    }

    /// The last status of a session.
    ///
    /// # Errors
    /// `not_found`.
    pub fn status(&self, session_id: &str) -> ApiResult<LinkStatus> {
        let ctl = self.get(session_id)?;
        let mut status = lock(&ctl.status).clone();
        if let Some(w) = &ctl.window {
            status.expires_in_secs = status.expires_in_secs.map(|_| w.remaining().as_secs());
        }
        Ok(status)
    }

    /// Receiver: Accept (`true`) or Decline the previewed offer.
    ///
    /// # Errors
    /// `not_found`; `invalid_argument` when no preview is waiting.
    pub fn decide(&self, session_id: &str, accept: bool) -> ApiResult<LinkStatus> {
        let ctl = self.get(session_id)?;
        // Only an offer the user is looking at can be accepted: an early Accept must never
        // apply an offer that has not been previewed yet.
        if lock(&ctl.status).phase != LinkPhase::Preview {
            return Err(ApiError::invalid("Nothing is waiting for a decision."));
        }
        let tx = lock(&ctl.decision)
            .take()
            .ok_or_else(|| ApiError::invalid("Nothing is waiting for a decision."))?;
        let _ = tx.send(accept);
        self.status(session_id)
    }

    /// Cancel a session (closes its window and connection; nothing partial is kept).
    ///
    /// # Errors
    /// `not_found`.
    pub fn cancel(&self, session_id: &str) -> ApiResult<LinkStatus> {
        let ctl = self.get(session_id)?;
        ctl.cancel.cancel();
        if let Some(w) = &ctl.window {
            w.close();
        }
        self.status(session_id)
    }
}
