//! The device-to-device transport: an authenticated, framed, bidirectional message channel to a
//! peer whose identity (certificate fingerprint) was proven during the handshake.
//!
//! The sync protocol (B3) is written against these traits only, so the TCP+TLS implementation
//! ([`tcp`], feature `tcp`) can later be swapped for another stack (for example iroh QUIC with
//! relays off) without touching it.

#[cfg(feature = "tcp")]
pub mod frame;
#[cfg(feature = "tcp")]
pub mod tcp;

use std::fmt;
use std::future::Future;

use crate::error::LinkError;
use crate::fingerprint::{CertFingerprint, DeviceId};

/// ALPN protocol id of the device link (distinct from the AI host's `http/1.1`).
pub const ALPN: &[u8] = b"borealis-link/1";
/// Default largest frame (1 MiB); bigger payloads (bundles, B3) are chunked by the caller.
pub const DEFAULT_MAX_FRAME: usize = 1024 * 1024;

/// Which peer certificates a side accepts during the handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerPolicy {
    /// Exactly this certificate (QR pin, or reconnecting to one paired device).
    Pinned(CertFingerprint),
    /// Any of these certificates (a listener serving its paired devices). Empty = nobody.
    AnyOf(Vec<CertFingerprint>),
    /// Any certificate whose key signs the handshake. The application **must** then
    /// authenticate the peer itself before trusting it: the typed-code pairing (B2) runs SPAKE2
    /// bound to [`Connection::export_keying_material`]. Use only while a pairing window is open.
    AnyCertificate,
}

impl PeerPolicy {
    /// True when `fp` may complete a handshake.
    pub fn allows(&self, fp: &CertFingerprint) -> bool {
        match self {
            Self::Pinned(pin) => pin.ct_eq(fp),
            Self::AnyOf(pins) => pins.iter().fold(false, |ok, p| ok | p.ct_eq(fp)),
            Self::AnyCertificate => true,
        }
    }
}

/// The peer as proven by the handshake (it holds the private key of this certificate).
#[derive(Clone, PartialEq, Eq)]
pub struct PeerIdentity {
    /// SHA-256 of the peer's certificate.
    pub fingerprint: CertFingerprint,
}

impl PeerIdentity {
    /// The peer's device id, derived from its certificate.
    pub fn device_id(&self) -> DeviceId {
        DeviceId::of(&self.fingerprint)
    }
}

impl fmt::Debug for PeerIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PeerIdentity({})", self.device_id())
    }
}

/// One authenticated, framed connection. Frames arrive whole and in order. After any error
/// (including a timeout, which may interrupt a frame) the connection must be dropped.
pub trait Connection: Send {
    /// The authenticated peer.
    fn peer(&self) -> &PeerIdentity;

    /// Send one frame (at most the configured maximum frame size).
    fn send(&mut self, frame: &[u8]) -> impl Future<Output = Result<(), LinkError>> + Send;

    /// Receive the next frame; `Ok(None)` when the peer closed the connection cleanly.
    fn recv(&mut self) -> impl Future<Output = Result<Option<Vec<u8>>, LinkError>> + Send;

    /// RFC 8446 / 5705 keying-material exporter for channel binding (B2's SPAKE2 binds to it,
    /// so a man in the middle with its own TLS sessions cannot splice two handshakes).
    ///
    /// # Errors
    /// [`LinkError::Tls`] if the exporter is unavailable.
    fn export_keying_material(
        &self,
        label: &[u8],
        context: &[u8],
        len: usize,
    ) -> Result<Vec<u8>, LinkError>;

    /// Flush and close the sending side (sends TLS close_notify).
    fn close(self) -> impl Future<Output = Result<(), LinkError>> + Send;
}

/// Accepts authenticated connections.
pub trait Listener: Send {
    /// The connection type.
    type Conn: Connection;
    /// The address peers connect to.
    type Addr: Clone + fmt::Debug + Send + Sync;

    /// Where this listener can be reached.
    fn local_addr(&self) -> Self::Addr;

    /// Next connection that completed the handshake under the current policy. Failed
    /// handshakes are dropped internally and never surface here.
    fn accept(&mut self) -> impl Future<Output = Result<Self::Conn, LinkError>> + Send;

    /// Replace the accept policy (paired-device set changed, pairing window opened or closed).
    /// Applies to handshakes that start afterwards.
    fn set_policy(&self, policy: PeerPolicy);
}

/// Opens connections and listeners with this device's identity.
pub trait Transport: Send + Sync {
    /// Peer address type (`SocketAddr` for TCP).
    type Addr: Clone + fmt::Debug + Send + Sync;
    /// The connection type.
    type Conn: Connection;
    /// The listener type.
    type Listener: Listener<Conn = Self::Conn, Addr = Self::Addr>;

    /// Connect to `addr`, accepting only a server that `policy` allows. A server certificate
    /// outside the policy fails with [`LinkError::PinMismatch`]. In TLS 1.3 the server judges
    /// *this* device's certificate after the client finished, so a refusal by the server shows
    /// up as an error on the first [`Connection::recv`].
    fn connect(
        &self,
        addr: Self::Addr,
        policy: PeerPolicy,
    ) -> impl Future<Output = Result<Self::Conn, LinkError>> + Send;

    /// Start listening on `bind`, accepting only clients that `policy` allows.
    fn listen(
        &self,
        bind: Self::Addr,
        policy: PeerPolicy,
    ) -> impl Future<Output = Result<Self::Listener, LinkError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policies_allow_only_what_they_name() {
        let a = CertFingerprint::of_der(b"a");
        let b = CertFingerprint::of_der(b"b");
        assert!(PeerPolicy::Pinned(a).allows(&a));
        assert!(!PeerPolicy::Pinned(a).allows(&b));
        assert!(PeerPolicy::AnyOf(vec![a, b]).allows(&b));
        assert!(!PeerPolicy::AnyOf(Vec::new()).allows(&a));
        assert!(PeerPolicy::AnyCertificate.allows(&b));
    }

    #[test]
    fn peer_debug_shows_the_device_id_only() {
        let p = PeerIdentity {
            fingerprint: CertFingerprint::of_der(b"a"),
        };
        assert_eq!(format!("{p:?}"), format!("PeerIdentity({})", p.device_id()));
    }
}
