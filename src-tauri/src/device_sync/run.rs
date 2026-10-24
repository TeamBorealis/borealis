//! Session tasks: the listening side (QR / code shown here) and the dialing side (scanned or
//! typed there), authentication, pairing memory, and the transfer itself.

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};

use borealis_link::auth::{
    accept_auth, dial_auth, AuthError, AuthWindow, Authenticated, Credential, AUTH_WINDOW_TTL,
};
use borealis_link::device::load_or_create_device_identity;
use borealis_link::registry::DeviceRegistry;
use borealis_link::store::save_peer_token;
use borealis_link::transport::tcp::{detect_lan_ip, TcpConfig, TcpConnection, TcpTransport};
use borealis_link::{
    Connection, DeviceId, LinkMode, LinkUri, Listener, PairedDevice, PeerPolicy, Platform,
    Transport, TypedCode,
};
use borealis_transfer::{
    receive, send, Cancel, Event, Events, Local, Outgoing, PeerInfo, Preview, PreviewAction,
    TransferError, TransferLimits,
};
use tokio::sync::oneshot;

use super::host::{AppReceiver, OutgoingBundle};
use super::{lock, DeviceSync, SessionCtl};
use crate::error::{ApiError, ApiResult};
use crate::sync_dto::{LinkPhase, LinkPreview, LinkPreviewItem, LinkStatus, LinkSummary};

/// How long the typing device browses mDNS for a nameplate.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
const DISCOVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);

/// A running mDNS advertisement (withdrawn when dropped).
type Advert = Box<dyn Send>;

/// What the dialing side connects to.
pub(crate) enum Target {
    Qr(LinkUri),
    Code(TypedCode, Option<SocketAddr>),
}

pub(crate) fn is_final(phase: LinkPhase) -> bool {
    matches!(
        phase,
        LinkPhase::Done
            | LinkPhase::Failed
            | LinkPhase::Cancelled
            | LinkPhase::Declined
            | LinkPhase::Expired
    )
}

/// Parse a scanned payload or a typed code (+ optional `ip:port`).
pub(crate) fn parse_target(target: &str, host: Option<&str>, sending: bool) -> ApiResult<Target> {
    let target = target.trim();
    if target.starts_with(borealis_link::LINK_URI_PREFIX) {
        let uri: LinkUri = target
            .parse()
            .map_err(|e: borealis_link::LinkUriError| ApiError::invalid(e.to_string()))?;
        // The QR says what the *showing* device does; this device must do the opposite.
        let wants = if sending {
            LinkMode::Receive
        } else {
            LinkMode::Send
        };
        if uri.mode != wants {
            let msg = if sending {
                "That device is sending, not receiving. Choose Receive here instead."
            } else {
                "That device is waiting to receive. Choose Send here and pick what to send."
            };
            return Err(ApiError::invalid(msg));
        }
        return Ok(Target::Qr(uri));
    }
    let code: TypedCode = target.parse().map_err(|_| {
        ApiError::invalid("That code is not valid. It looks like 42-ember-lantern.")
    })?;
    let addr = host
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .map(|h| {
            h.parse::<SocketAddr>().map_err(|_| {
                ApiError::invalid("Enter the address as shown, like 192.168.1.20:41234.")
            })
        })
        .transpose()?;
    Ok(Target::Code(code, addr))
}

fn role_word(sending: bool) -> &'static str {
    if sending {
        "send"
    } else {
        "receive"
    }
}

fn transport(sync: &DeviceSync) -> ApiResult<(TcpTransport, borealis_link::CertFingerprint)> {
    let store = sync.store()?;
    let identity = load_or_create_device_identity(store.as_ref())
        .map_err(|e| ApiError::io("load this device's identity", e))?;
    let fp = identity.fingerprint();
    Ok((TcpTransport::new(identity, TcpConfig::default()), fp))
}

fn bind_ip(sync: &DeviceSync) -> ApiResult<IpAddr> {
    sync.config.bind_ip.or_else(detect_lan_ip).ok_or_else(|| {
        ApiError::invalid("No network found. Connect both devices to the same Wi-Fi.")
    })
}

fn new_ctl(
    status: LinkStatus,
    window: Option<Arc<AuthWindow>>,
) -> (Arc<SessionCtl>, oneshot::Receiver<bool>) {
    let (tx, rx) = oneshot::channel();
    let ctl = Arc::new(SessionCtl {
        cancel: Cancel::new(),
        decision: Mutex::new(Some(tx)),
        status: Mutex::new(status),
        window,
    });
    (ctl, rx)
}

/// Update a session's status and emit it.
fn update(sync: &DeviceSync, ctl: &SessionCtl, f: impl FnOnce(&mut LinkStatus)) {
    let snapshot = {
        let mut s = lock(&ctl.status);
        if is_final(s.phase) {
            return;
        }
        f(&mut s);
        s.clone()
    };
    sync.events.status(&snapshot);
}

fn fail(sync: &DeviceSync, ctl: &SessionCtl, code: &str, message: String) {
    update(sync, ctl, |s| {
        s.phase = LinkPhase::Failed;
        s.error_code = Some(code.to_string());
        s.error = Some(message);
        s.qr_payload = None;
        s.code = None;
    });
}

/// Listening side: open the window, wait for one authenticated connection, then transfer.
pub(crate) async fn listen(
    sync: &Arc<DeviceSync>,
    id: String,
    outgoing: Option<OutgoingBundle>,
) -> ApiResult<LinkStatus> {
    let sending = outgoing.is_some();
    let (tcp, fp) = transport(sync)?;
    let ip = bind_ip(sync)?;
    let mut listener = tcp
        .listen(SocketAddr::new(ip, 0), PeerPolicy::AnyCertificate)
        .await
        .map_err(|e| ApiError::io("open the connection", e))?;
    let window = Arc::new(AuthWindow::open(AUTH_WINDOW_TTL).map_err(ApiError::internal)?);
    let addr = listener.local_addr();
    let uri = LinkUri {
        addr,
        fingerprint: fp,
        secret: window.secret().clone(),
        name: borealis_link::registry::clean_name(&sync.config.device_name),
        mode: if sending {
            LinkMode::Send
        } else {
            LinkMode::Receive
        },
    };
    let mut status = LinkStatus::new(&id, role_word(sending), LinkPhase::Waiting);
    status.qr_payload = Some(uri.to_string());
    status.code = Some(window.code().to_string());
    status.address = Some(addr.to_string());
    status.fingerprint = Some(fp.short_hex());
    status.expires_in_secs = Some(window.remaining().as_secs());
    let (ctl, decision) = new_ctl(status.clone(), Some(Arc::clone(&window)));
    sync.insert(Arc::clone(&ctl))?;
    let advert = advertise(sync, window.code().nameplate(), ip, addr.port());
    let sync = Arc::clone(sync);
    tokio::spawn(async move {
        let authed = accept_one(&sync, &ctl, &mut listener, &window).await;
        drop(listener);
        drop(advert);
        if let Some((conn, auth)) = authed {
            transfer(&sync, &ctl, conn, auth, outgoing, decision).await;
        }
    });
    Ok(status)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn advertise(sync: &DeviceSync, nameplate: u16, ip: IpAddr, port: u16) -> Option<Advert> {
    if !sync.config.discovery {
        return None;
    }
    borealis_link::discovery::Advertisement::start(nameplate, ip, port)
        .map(|a| Box::new(a) as Advert)
        .map_err(|e| tracing::info!("device sync: mDNS advertise unavailable: {e}"))
        .ok()
}

/// Phones advertise nothing (see `borealis_link::discovery`); the QR carries `ip:port`.
#[cfg(any(target_os = "android", target_os = "ios"))]
fn advertise(_: &DeviceSync, _: u16, _: IpAddr, _: u16) -> Option<Advert> {
    None
}

async fn accept_one(
    sync: &DeviceSync,
    ctl: &SessionCtl,
    listener: &mut borealis_link::transport::tcp::TcpLinkListener,
    window: &AuthWindow,
) -> Option<(TcpConnection, Authenticated)> {
    loop {
        let mut conn = tokio::select! {
            c = listener.accept() => c.ok()?,
            () = tokio::time::sleep(window.remaining()) => {
                update(sync, ctl, |s| { s.phase = LinkPhase::Expired; s.qr_payload = None; s.code = None; });
                return None;
            }
            () = ctl.cancel.cancelled() => {
                update(sync, ctl, |s| { s.phase = LinkPhase::Cancelled; s.qr_payload = None; s.code = None; });
                return None;
            }
        };
        update(sync, ctl, |s| s.phase = LinkPhase::Connecting);
        let attempt = tokio::select! {
            r = accept_auth(&mut conn, window) => r,
            () = ctl.cancel.cancelled() => {
                window.close();
                update(sync, ctl, |s| { s.phase = LinkPhase::Cancelled; s.qr_payload = None; s.code = None; });
                return None;
            }
        };
        match attempt {
            Ok(auth) => return Some((conn, auth)),
            // A connection that never started an attempt (silent, busy, or broken before its
            // first message) leaves the window open: keep listening.
            Err(_) if window.is_open() && !window.is_dead() => {
                update(sync, ctl, |s| s.phase = LinkPhase::Waiting);
            }
            Err(e) if e.auth() == Some(AuthError::Expired) => {
                update(sync, ctl, |s| {
                    s.phase = LinkPhase::Expired;
                    s.qr_payload = None;
                    s.code = None;
                });
                return None;
            }
            Err(e) => {
                tracing::info!("device sync: authentication failed: {e}");
                fail(
                    sync,
                    ctl,
                    "wrong_code",
                    "The code did not match, so it was closed. Start again for a new code.".into(),
                );
                return None;
            }
        }
    }
}

/// Dialing side: resolve, connect, authenticate, transfer (in the background).
pub(crate) fn dial(
    sync: &Arc<DeviceSync>,
    id: String,
    target: Target,
    outgoing: Option<OutgoingBundle>,
) -> ApiResult<LinkStatus> {
    let (tcp, _) = transport(sync)?;
    let mut status = LinkStatus::new(&id, role_word(outgoing.is_some()), LinkPhase::Connecting);
    if let Target::Qr(uri) = &target {
        status.peer_name = Some(uri.name.clone());
    }
    let (ctl, decision) = new_ctl(status.clone(), None);
    sync.insert(Arc::clone(&ctl))?;
    let sync = Arc::clone(sync);
    tokio::spawn(async move {
        let connected = tokio::select! {
            c = connect(&sync, &tcp, target) => c,
            () = ctl.cancel.cancelled() => Err((String::from("cancelled"), String::new())),
        };
        match connected {
            Ok((conn, auth)) => transfer(&sync, &ctl, conn, auth, outgoing, decision).await,
            Err((code, _)) if code == "cancelled" => {
                update(&sync, &ctl, |s| s.phase = LinkPhase::Cancelled)
            }
            Err((code, msg)) => fail(&sync, &ctl, &code, msg),
        }
    });
    Ok(status)
}

async fn resolve(
    sync: &DeviceSync,
    code: &TypedCode,
    addr: Option<SocketAddr>,
) -> Result<SocketAddr, (String, String)> {
    if let Some(a) = addr {
        return Ok(a);
    }
    let not_found = || {
        (
            "not_found".to_string(),
            "The other device was not found. Type the address shown under its code.".to_string(),
        )
    };
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    if sync.config.discovery {
        let nameplate = code.nameplate();
        let found = tokio::task::spawn_blocking(move || {
            borealis_link::discovery::find(nameplate, DISCOVERY_TIMEOUT)
        })
        .await
        .ok()
        .and_then(Result::ok)
        .flatten();
        return found.ok_or_else(not_found);
    }
    let _ = (sync, code);
    Err(not_found())
}

async fn connect(
    sync: &DeviceSync,
    tcp: &TcpTransport,
    target: Target,
) -> Result<(TcpConnection, Authenticated), (String, String)> {
    let conn_err = |e: borealis_link::LinkError| match e {
        borealis_link::LinkError::PinMismatch => (
            "pin_mismatch".to_string(),
            "That device's identity does not match its QR code.".to_string(),
        ),
        other => (
            "connection".to_string(),
            format!("Could not connect: {other}"),
        ),
    };
    let (addr, policy, cred) = match target {
        Target::Qr(uri) => (
            uri.addr,
            PeerPolicy::Pinned(uri.fingerprint),
            Credential::Qr(uri.secret),
        ),
        Target::Code(code, addr) => (
            resolve(sync, &code, addr).await?,
            PeerPolicy::AnyCertificate,
            Credential::Code(code),
        ),
    };
    let mut conn = tcp.connect(addr, policy).await.map_err(conn_err)?;
    let auth = dial_auth(&mut conn, &cred)
        .await
        .map_err(|e| match e.auth() {
            Some(AuthError::Mismatch) => (
                "wrong_code".to_string(),
                "The code did not match. Ask for a new code and try again.".to_string(),
            ),
            Some(AuthError::Expired | AuthError::Closed) => (
                "expired".to_string(),
                "That code is no longer valid. Ask for a new one.".to_string(),
            ),
            _ => ("connection".to_string(), format!("Could not connect: {e}")),
        })?;
    Ok((conn, auth))
}

/// Remembers the peer after its hello (registry + pair key in the secure store).
struct SessionEvents {
    sync: Arc<DeviceSync>,
    ctl: Arc<SessionCtl>,
    auth: Mutex<Option<Authenticated>>,
}

impl SessionEvents {
    fn remember(&self, peer: &PeerInfo) {
        let Some(auth) = lock(&self.auth).take() else {
            return;
        };
        let fp = auth.peer.fingerprint;
        let data = &self.sync.state.state().paths.data;
        let now = super::now_ms() / 1000;
        let id = DeviceId::of(&fp);
        // A known, unexpired device keeps its pairing date and gets a fresh "last synced";
        // anything else (new, expired, removed) is paired anew.
        let saved = DeviceRegistry::load(data)
            .and_then(|reg| match reg.get(&id) {
                Some(d) if !d.is_expired(now) => Ok(reg.with_seen(&id, now)),
                _ => reg.with_paired(PairedDevice::new(&peer.name, platform_of(peer), fp, now)),
            })
            .and_then(|reg| reg.save(data));
        if let Err(e) = saved {
            tracing::warn!("device sync: could not remember the device: {e}");
        }
        if let Ok(store) = self.sync.store() {
            if let Err(e) = save_peer_token(store.as_ref(), &id, &auth.pair_key) {
                tracing::warn!("device sync: could not store the pairing key: {e}");
            }
        }
    }
}

fn platform_of(peer: &PeerInfo) -> Platform {
    peer.platform
}

pub(crate) fn preview_dto(p: &Preview) -> LinkPreview {
    LinkPreview {
        items: p
            .items
            .iter()
            .map(|i| LinkPreviewItem {
                kind: match i.kind {
                    borealis_bundle::ItemKind::Character => "character".into(),
                    borealis_bundle::ItemKind::Pack => "pack".into(),
                },
                key: i.key.clone(),
                label: i.label.clone(),
                action: match i.action {
                    PreviewAction::New => "new",
                    PreviewAction::Replace => "replace",
                    PreviewAction::KeepLocal => "keep_local",
                    PreviewAction::Identical => "identical",
                }
                .into(),
                size: i.size,
                missing_packs: i.missing_packs.clone(),
            })
            .collect(),
        total_bytes: p.total_bytes,
    }
}

impl Events for SessionEvents {
    fn event(&self, event: Event) {
        let (sync, ctl) = (&self.sync, &self.ctl);
        match event {
            Event::Connected(peer) => {
                self.remember(&peer);
                update(sync, ctl, |s| s.peer_name = Some(peer.name.clone()));
            }
            Event::Preview(p) => update(sync, ctl, |s| {
                s.phase = LinkPhase::Preview;
                s.bytes_total = Some(p.total_bytes);
                s.preview = Some(preview_dto(&p));
            }),
            Event::WaitingForPeer => update(sync, ctl, |s| s.phase = LinkPhase::WaitingForPeer),
            Event::Progress { done, total } => update(sync, ctl, |s| {
                s.phase = LinkPhase::Transferring;
                s.bytes_done = Some(done);
                s.bytes_total = Some(total);
            }),
            Event::Applying => update(sync, ctl, |s| s.phase = LinkPhase::Applying),
        }
    }
}

async fn transfer(
    sync: &Arc<DeviceSync>,
    ctl: &Arc<SessionCtl>,
    mut conn: TcpConnection,
    auth: Authenticated,
    outgoing: Option<OutgoingBundle>,
    decision: oneshot::Receiver<bool>,
) {
    update(sync, ctl, |s| {
        s.phase = LinkPhase::Connecting;
        s.qr_payload = None;
        s.code = None;
        s.expires_in_secs = None;
    });
    let events = SessionEvents {
        sync: Arc::clone(sync),
        ctl: Arc::clone(ctl),
        auth: Mutex::new(Some(auth)),
    };
    let local = Local {
        name: sync.config.device_name.clone(),
        platform: Platform::current(),
    };
    let limits = TransferLimits::default();
    let result = match &outgoing {
        Some(out) => {
            let o = Outgoing {
                bundle: out.file.path(),
                manifest: &out.manifest,
                labels: out.labels.clone(),
            };
            send(&mut conn, &local, &o, &ctl.cancel, &events, &limits).await
        }
        None => {
            let host = AppReceiver {
                state: Arc::clone(&sync.state),
                events: Arc::clone(&sync.events),
            };
            receive(
                &mut conn,
                &local,
                &host,
                decision,
                &ctl.cancel,
                &events,
                &limits,
            )
            .await
        }
    };
    let _ = conn.close().await;
    drop(outgoing);
    match result {
        Ok(summary) => update(sync, ctl, |s| {
            s.phase = LinkPhase::Done;
            s.summary = Some(LinkSummary {
                added: summary.added,
                replaced: summary.replaced,
                kept_local: summary.kept_local,
                identical: summary.identical,
            });
        }),
        Err(TransferError::Cancelled) => update(sync, ctl, |s| s.phase = LinkPhase::Cancelled),
        Err(TransferError::Declined) => update(sync, ctl, |s| s.phase = LinkPhase::Declined),
        Err(e) => fail(sync, ctl, e.code(), e.to_string()),
    }
}
