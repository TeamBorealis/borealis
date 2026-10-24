//! Session authentication inside a TLS 1.3 [`Connection`] (B2).
//!
//! The device that shows the QR / code opens an [`AuthWindow`] and listens with
//! [`PeerPolicy::AnyCertificate`](crate::PeerPolicy::AnyCertificate); the other device proves it
//! saw the window:
//! * **QR**: the dialer pinned the listener's certificate from the QR, then sends
//!   `HMAC(secret, "client" || exporter)`; the listener answers with its own proof.
//! * **Typed code**: symmetric SPAKE2 (Ed25519) over the code, with the TLS exporter as the
//!   SPAKE2 identity, then HMAC key confirmation over the exporter both ways. A man in the middle
//!   who terminates TLS on both sides sees two different exporters, so the keys differ and the
//!   confirmation fails even when he relays every SPAKE2 message unchanged.
//!
//! A window allows **one attempt**: whatever its outcome, the window is consumed (success) or
//! dead (anything else), and it expires after [`AUTH_WINDOW_TTL`]. A concurrent second connection
//! is refused without touching the running attempt.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use spake2::{Ed25519Group, Identity, Password, Spake2};

use crate::code::TypedCode;
use crate::error::LinkError;
use crate::link_uri::LinkSecret;
use crate::store::Secret;
use crate::transport::{Connection, PeerIdentity};

/// How long a Send/Receive window (QR + code) stays open.
pub const AUTH_WINDOW_TTL: Duration = Duration::from_secs(5 * 60);
/// Longest time the whole authentication exchange may take.
pub const AUTH_TIMEOUT: Duration = Duration::from_secs(20);
/// RFC 8446 exporter label the authentication binds to.
pub const EXPORTER_LABEL: &[u8] = b"EXPORTER-borealis-link-auth";
const EXPORTER_LEN: usize = 32;
const PROTOCOL: u8 = 1;
const MAX_AUTH_FRAME: usize = 4096;
const PAIR_KEY_INFO: &[u8] = b"borealis-link pair key v1";

type HmacSha256 = Hmac<Sha256>;

/// Why authentication failed (typed, so the UI can say "wrong code" vs "expired").
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AuthError {
    /// The proof or key confirmation did not match (wrong code, wrong QR, or a man in the middle).
    #[error("the code did not match; start again with a new code")]
    Mismatch,
    /// The window expired.
    #[error("the code expired")]
    Expired,
    /// The window was already used or killed by an earlier attempt.
    #[error("the code was already used")]
    Closed,
    /// Another device is authenticating right now.
    #[error("another device is connecting")]
    Busy,
    /// The peer broke the protocol or stalled.
    #[error("the other device did not follow the protocol")]
    Protocol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Open,
    Busy,
    Used,
    Dead,
}

/// One Send/Receive window: the QR secret and the typed code, single use, with an expiry.
pub struct AuthWindow {
    secret: LinkSecret,
    code: TypedCode,
    expires: Instant,
    status: Mutex<Status>,
}

impl std::fmt::Debug for AuthWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthWindow")
            .field("code", &self.code)
            .field("status", &*lock(&self.status))
            .finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl AuthWindow {
    /// A window with a fresh secret and code, open for `ttl`.
    ///
    /// # Errors
    /// [`LinkError::Random`].
    pub fn open(ttl: Duration) -> Result<Self, LinkError> {
        Ok(Self::with(
            LinkSecret::generate()?,
            TypedCode::generate()?,
            ttl,
        ))
    }

    /// A window for a given secret and code (tests).
    pub fn with(secret: LinkSecret, code: TypedCode, ttl: Duration) -> Self {
        Self {
            secret,
            code,
            expires: Instant::now() + ttl,
            status: Mutex::new(Status::Open),
        }
    }

    /// The QR secret.
    pub fn secret(&self) -> &LinkSecret {
        &self.secret
    }

    /// The typed code.
    pub fn code(&self) -> &TypedCode {
        &self.code
    }

    /// Time left; zero once expired.
    pub fn remaining(&self) -> Duration {
        self.expires.saturating_duration_since(Instant::now())
    }

    /// True while an attempt could still succeed.
    pub fn is_open(&self) -> bool {
        matches!(*lock(&self.status), Status::Open | Status::Busy) && !self.remaining().is_zero()
    }

    /// True once an attempt failed (the code is dead).
    pub fn is_dead(&self) -> bool {
        *lock(&self.status) == Status::Dead
    }

    /// Close the window (dialog cancelled).
    pub fn close(&self) {
        let mut s = lock(&self.status);
        if *s != Status::Used {
            *s = Status::Dead;
        }
    }

    fn begin(&self) -> Result<Attempt<'_>, AuthError> {
        let mut s = lock(&self.status);
        if self.remaining().is_zero() {
            if *s == Status::Open {
                *s = Status::Dead;
            }
            return Err(AuthError::Expired);
        }
        match *s {
            Status::Open => {
                *s = Status::Busy;
                Ok(Attempt {
                    window: self,
                    ok: false,
                })
            }
            Status::Busy => Err(AuthError::Busy),
            Status::Used | Status::Dead => Err(AuthError::Closed),
        }
    }
}

/// The running attempt; dropping it without success kills the window.
struct Attempt<'a> {
    window: &'a AuthWindow,
    ok: bool,
}

impl Drop for Attempt<'_> {
    fn drop(&mut self) {
        *lock(&self.window.status) = if self.ok { Status::Used } else { Status::Dead };
    }
}

/// What the dialing device knows.
#[derive(Debug, Clone)]
pub enum Credential {
    /// The secret from a scanned QR (the connection must have pinned the QR's fingerprint).
    Qr(LinkSecret),
    /// A typed code.
    Code(TypedCode),
}

/// A successfully authenticated session.
#[derive(Debug)]
pub struct Authenticated {
    /// The peer (its certificate, proven by TLS and now bound to the window).
    pub peer: PeerIdentity,
    /// 32-byte key both sides derived; kept in the secure store for the pairing (C1).
    pub pair_key: Secret,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "auth", rename_all = "lowercase", deny_unknown_fields)]
enum Hello {
    Qr { v: u8, proof: String },
    Code { v: u8, msg: String },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    msg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proof: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Confirm {
    proof: String,
}

fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn unb64(s: &str) -> Result<Vec<u8>, AuthError> {
    if s.len() > MAX_AUTH_FRAME {
        return Err(AuthError::Protocol);
    }
    URL_SAFE_NO_PAD.decode(s).map_err(|_| AuthError::Protocol)
}

fn mac(key: &[u8], role: &[u8], ekm: &[u8]) -> Vec<u8> {
    let mut m = <HmacSha256 as Mac>::new_from_slice(key).expect("hmac takes any key length");
    m.update(b"borealis-link auth v1 ");
    m.update(role);
    m.update(&[0]);
    m.update(ekm);
    m.finalize().into_bytes().to_vec()
}

fn mac_ok(key: &[u8], role: &[u8], ekm: &[u8], got: &[u8]) -> bool {
    let mut m = <HmacSha256 as Mac>::new_from_slice(key).expect("hmac takes any key length");
    m.update(b"borealis-link auth v1 ");
    m.update(role);
    m.update(&[0]);
    m.update(ekm);
    m.verify_slice(got).is_ok()
}

fn pair_key(ikm: &[u8], ekm: &[u8]) -> Secret {
    let hk = hkdf::Hkdf::<Sha256>::new(Some(ekm), ikm);
    let mut out = vec![0u8; 32];
    hk.expand(PAIR_KEY_INFO, &mut out)
        .expect("32 bytes is a valid HKDF-SHA256 length");
    Secret::new(out)
}

fn exporter<C: Connection>(conn: &C) -> Result<Vec<u8>, LinkError> {
    conn.export_keying_material(EXPORTER_LABEL, b"", EXPORTER_LEN)
}

fn spake_identity(ekm: &[u8]) -> Identity {
    Identity::new(&[b"borealis-link/1 ".as_slice(), ekm].concat())
}

async fn send_json<C: Connection, T: Serialize>(conn: &mut C, v: &T) -> Result<(), AuthFailure> {
    let bytes = serde_json::to_vec(v).map_err(|_| AuthError::Protocol)?;
    conn.send(&bytes).await.map_err(AuthFailure::Link)
}

async fn recv_json<C: Connection, T: for<'de> Deserialize<'de>>(
    conn: &mut C,
) -> Result<T, AuthFailure> {
    let frame = conn
        .recv()
        .await
        .map_err(AuthFailure::Link)?
        .ok_or(AuthError::Protocol)?;
    if frame.len() > MAX_AUTH_FRAME {
        return Err(AuthError::Protocol.into());
    }
    Ok(serde_json::from_slice(&frame).map_err(|_| AuthError::Protocol)?)
}

/// Authentication failure: a typed protocol outcome or a transport error.
#[derive(Debug, thiserror::Error)]
pub enum AuthFailure {
    /// The exchange completed but was refused, or the window was not usable.
    #[error(transparent)]
    Auth(#[from] AuthError),
    /// The connection failed underneath.
    #[error(transparent)]
    Link(#[from] LinkError),
}

impl AuthFailure {
    /// The typed reason, when there is one.
    pub fn auth(&self) -> Option<AuthError> {
        match self {
            Self::Auth(a) => Some(*a),
            Self::Link(_) => None,
        }
    }
}

async fn timed<T>(
    fut: impl std::future::Future<Output = Result<T, AuthFailure>>,
) -> Result<T, AuthFailure> {
    tokio::time::timeout(AUTH_TIMEOUT, fut)
        .await
        .map_err(|_| AuthFailure::Link(LinkError::Timeout("authenticating")))?
}

/// Listener side: authenticate the dialer against `window`. One attempt per window; any failure
/// after the dialer's first message kills it.
///
/// # Errors
/// [`AuthFailure`]; the caller drops the connection.
pub async fn accept_auth<C: Connection>(
    conn: &mut C,
    window: &AuthWindow,
) -> Result<Authenticated, AuthFailure> {
    let ekm = exporter(conn)?;
    timed(async {
        let hello: Hello = recv_json(conn).await?;
        let mut attempt = window.begin()?;
        let key = match hello {
            Hello::Qr { v, proof } => {
                check_version(v)?;
                let secret = window.secret.as_bytes();
                if !mac_ok(secret, b"qr client", &ekm, &unb64(&proof)?) {
                    let _ = send_json(conn, &no()).await;
                    return Err(AuthError::Mismatch.into());
                }
                let reply = Reply {
                    ok: true,
                    msg: None,
                    proof: Some(b64(&mac(secret, b"qr server", &ekm))),
                };
                send_json(conn, &reply).await?;
                pair_key(secret, &ekm)
            }
            Hello::Code { v, msg } => {
                check_version(v)?;
                let password = Password::new(window.code.password());
                let (state, out) =
                    Spake2::<Ed25519Group>::start_symmetric(&password, &spake_identity(&ekm));
                let k = state
                    .finish(&unb64(&msg)?)
                    .map_err(|_| AuthError::Protocol)?;
                let reply = Reply {
                    ok: true,
                    msg: Some(b64(&out)),
                    proof: Some(b64(&mac(&k, b"code server", &ekm))),
                };
                send_json(conn, &reply).await?;
                let confirm: Confirm = recv_json(conn).await?;
                if !mac_ok(&k, b"code client", &ekm, &unb64(&confirm.proof)?) {
                    let _ = send_json(conn, &no()).await;
                    return Err(AuthError::Mismatch.into());
                }
                send_json(conn, &yes()).await?;
                pair_key(&k, &ekm)
            }
        };
        attempt.ok = true;
        Ok(Authenticated {
            peer: conn.peer().clone(),
            pair_key: key,
        })
    })
    .await
}

fn check_version(v: u8) -> Result<(), AuthError> {
    (v == PROTOCOL).then_some(()).ok_or(AuthError::Protocol)
}

fn yes() -> Reply {
    Reply {
        ok: true,
        msg: None,
        proof: None,
    }
}

fn no() -> Reply {
    Reply {
        ok: false,
        msg: None,
        proof: None,
    }
}

/// Dialer side: prove `credential` to the listener and check its answer.
///
/// # Errors
/// [`AuthFailure`]: `Mismatch` for a wrong code (the listener's code is now dead) or a man in
/// the middle; `Protocol` for a refused or broken exchange.
pub async fn dial_auth<C: Connection>(
    conn: &mut C,
    credential: &Credential,
) -> Result<Authenticated, AuthFailure> {
    let ekm = exporter(conn)?;
    timed(async {
        let key = match credential {
            Credential::Qr(secret) => {
                let s = secret.as_bytes();
                let hello = Hello::Qr {
                    v: PROTOCOL,
                    proof: b64(&mac(s, b"qr client", &ekm)),
                };
                send_json(conn, &hello).await?;
                let reply: Reply = recv_json(conn).await?;
                let proof = reply
                    .proof
                    .filter(|_| reply.ok)
                    .ok_or(AuthError::Mismatch)?;
                if !mac_ok(s, b"qr server", &ekm, &unb64(&proof)?) {
                    return Err(AuthError::Mismatch.into());
                }
                pair_key(s, &ekm)
            }
            Credential::Code(code) => {
                let password = Password::new(code.password());
                let (state, out) =
                    Spake2::<Ed25519Group>::start_symmetric(&password, &spake_identity(&ekm));
                let hello = Hello::Code {
                    v: PROTOCOL,
                    msg: b64(&out),
                };
                send_json(conn, &hello).await?;
                let reply: Reply = recv_json(conn).await?;
                let (Some(msg), Some(proof), true) = (reply.msg, reply.proof, reply.ok) else {
                    return Err(AuthError::Mismatch.into());
                };
                let k = state
                    .finish(&unb64(&msg)?)
                    .map_err(|_| AuthError::Protocol)?;
                if !mac_ok(&k, b"code server", &ekm, &unb64(&proof)?) {
                    return Err(AuthError::Mismatch.into());
                }
                let confirm = Confirm {
                    proof: b64(&mac(&k, b"code client", &ekm)),
                };
                send_json(conn, &confirm).await?;
                let done: Reply = recv_json(conn).await?;
                if !done.ok {
                    return Err(AuthError::Mismatch.into());
                }
                pair_key(&k, &ekm)
            }
        };
        Ok(Authenticated {
            peer: conn.peer().clone(),
            pair_key: key,
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(ttl: Duration) -> AuthWindow {
        AuthWindow::with(
            LinkSecret::generate().unwrap(),
            "7-oak-raven".parse().unwrap(),
            ttl,
        )
    }

    #[test]
    fn one_attempt_per_window() {
        let w = window(AUTH_WINDOW_TTL);
        let a = w.begin().unwrap();
        assert_eq!(w.begin().err(), Some(AuthError::Busy));
        drop(a);
        assert!(w.is_dead());
        assert_eq!(w.begin().err(), Some(AuthError::Closed));
    }

    #[test]
    fn a_successful_attempt_consumes_the_window() {
        let w = window(AUTH_WINDOW_TTL);
        let mut a = w.begin().unwrap();
        a.ok = true;
        drop(a);
        assert!(!w.is_open() && !w.is_dead());
        assert_eq!(w.begin().err(), Some(AuthError::Closed));
    }

    #[test]
    fn expired_window_refuses() {
        let w = window(Duration::ZERO);
        assert_eq!(w.begin().err(), Some(AuthError::Expired));
        assert!(!w.is_open());
    }

    #[test]
    fn macs_bind_role_and_exporter() {
        let k = b"key";
        let m = mac(k, b"qr client", b"ekm-1");
        assert!(mac_ok(k, b"qr client", b"ekm-1", &m));
        assert!(!mac_ok(k, b"qr server", b"ekm-1", &m));
        assert!(!mac_ok(k, b"qr client", b"ekm-2", &m));
    }
}
