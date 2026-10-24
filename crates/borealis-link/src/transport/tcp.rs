//! [`Transport`] over tokio TCP + rustls: TLS 1.3 only, mutual certificate authentication
//! against a [`PeerPolicy`], ALPN `borealis-link/1`, length-prefixed frames.
//!
//! The listener accepts TCP connections in a background task and runs handshakes concurrently
//! (bounded, with a timeout), so a slow or hostile peer cannot block others.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use rustls::pki_types::ServerName;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;
use tokio_rustls::{TlsAcceptor, TlsConnector, TlsStream};

use super::frame::{read_frame, write_frame};
use super::{Connection, Listener, PeerIdentity, PeerPolicy, Transport, ALPN, DEFAULT_MAX_FRAME};
use crate::error::LinkError;
use crate::fingerprint::CertFingerprint;
use crate::tls::{mutual_client_config, mutual_server_config, SharedPolicy, TlsIdentity};

/// Name sent in SNI; the pin verifier ignores names.
const SERVER_NAME: &str = "borealis-link";
const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(50);
const ACCEPT_QUEUE: usize = 8;

/// Timeouts and caps.
#[derive(Debug, Clone)]
pub struct TcpConfig {
    /// TCP connect.
    pub connect_timeout: Duration,
    /// TLS handshake (both sides).
    pub handshake_timeout: Duration,
    /// Longest wait for the next frame in [`Connection::recv`], and for one frame to be written in
    /// [`Connection::send`] (`ZERO` = wait forever).
    pub idle_timeout: Duration,
    /// Largest frame either side may send.
    pub max_frame: usize,
    /// Concurrent handshakes in a listener.
    pub max_pending_handshakes: usize,
    /// Concurrent handshakes from one source IP.
    pub max_handshakes_per_ip: usize,
}

impl Default for TcpConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(8),
            handshake_timeout: Duration::from_secs(10),
            idle_timeout: Duration::from_secs(60),
            max_frame: DEFAULT_MAX_FRAME,
            max_pending_handshakes: 8,
            max_handshakes_per_ip: 4,
        }
    }
}

/// TCP + TLS transport bound to this device's identity.
#[derive(Debug, Clone)]
pub struct TcpTransport {
    identity: TlsIdentity,
    config: TcpConfig,
}

impl TcpTransport {
    /// A transport presenting `identity`.
    pub fn new(identity: TlsIdentity, config: TcpConfig) -> Self {
        Self { identity, config }
    }
}

/// An established, authenticated TCP + TLS connection.
pub struct TcpConnection {
    stream: TlsStream<TcpStream>,
    peer: PeerIdentity,
    max_frame: usize,
    idle_timeout: Duration,
}

impl std::fmt::Debug for TcpConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TcpConnection")
            .field("peer", &self.peer)
            .finish_non_exhaustive()
    }
}

fn peer_of(stream: &TlsStream<TcpStream>) -> Result<PeerIdentity, LinkError> {
    let (_, state) = stream.get_ref();
    let cert = state
        .peer_certificates()
        .and_then(<[_]>::first)
        .ok_or_else(|| LinkError::Tls("peer sent no certificate".into()))?;
    Ok(PeerIdentity {
        fingerprint: CertFingerprint::of_der(cert),
    })
}

fn tls(e: impl std::fmt::Display) -> LinkError {
    LinkError::Tls(e.to_string())
}

impl TcpConnection {
    fn new(stream: TlsStream<TcpStream>, config: &TcpConfig) -> Result<Self, LinkError> {
        Ok(Self {
            peer: peer_of(&stream)?,
            stream,
            max_frame: config.max_frame,
            idle_timeout: config.idle_timeout,
        })
    }

    /// The remote socket address.
    pub fn peer_addr(&self) -> Option<SocketAddr> {
        self.stream.get_ref().0.peer_addr().ok()
    }
}

impl Connection for TcpConnection {
    fn peer(&self) -> &PeerIdentity {
        &self.peer
    }

    async fn send(&mut self, frame: &[u8]) -> Result<(), LinkError> {
        let max = self.max_frame;
        if self.idle_timeout.is_zero() {
            return write_frame(&mut self.stream, frame, max).await;
        }
        // A peer that stops reading must not stall the sender forever.
        tokio::time::timeout(self.idle_timeout, write_frame(&mut self.stream, frame, max))
            .await
            .map_err(|_| LinkError::Timeout("sending to the peer"))?
    }

    async fn recv(&mut self) -> Result<Option<Vec<u8>>, LinkError> {
        let max = self.max_frame;
        if self.idle_timeout.is_zero() {
            return read_frame(&mut self.stream, max).await;
        }
        tokio::time::timeout(self.idle_timeout, read_frame(&mut self.stream, max))
            .await
            .map_err(|_| LinkError::Timeout("waiting for the peer"))?
    }

    fn export_keying_material(
        &self,
        label: &[u8],
        context: &[u8],
        len: usize,
    ) -> Result<Vec<u8>, LinkError> {
        let out = vec![0u8; len];
        match &self.stream {
            TlsStream::Client(s) => s
                .get_ref()
                .1
                .export_keying_material(out, label, Some(context)),
            TlsStream::Server(s) => s
                .get_ref()
                .1
                .export_keying_material(out, label, Some(context)),
        }
        .map_err(tls)
    }

    async fn close(mut self) -> Result<(), LinkError> {
        use tokio::io::AsyncWriteExt as _;
        self.stream.shutdown().await.map_err(LinkError::from)
    }
}

impl Transport for TcpTransport {
    type Addr = SocketAddr;
    type Conn = TcpConnection;
    type Listener = TcpLinkListener;

    async fn connect(
        &self,
        addr: SocketAddr,
        policy: PeerPolicy,
    ) -> Result<TcpConnection, LinkError> {
        let (cfg, mismatch) = mutual_client_config(&self.identity, policy, &[ALPN])?;
        let tcp = tokio::time::timeout(self.config.connect_timeout, TcpStream::connect(addr))
            .await
            .map_err(|_| LinkError::Timeout("connecting"))??;
        let _ = tcp.set_nodelay(true);
        let name = ServerName::try_from(SERVER_NAME).map_err(tls)?;
        let handshake = TlsConnector::from(Arc::new(cfg)).connect(name, tcp);
        let stream = match tokio::time::timeout(self.config.handshake_timeout, handshake).await {
            Err(_) => return Err(LinkError::Timeout("tls handshake")),
            Ok(Err(_)) if mismatch.load(Ordering::SeqCst) => return Err(LinkError::PinMismatch),
            Ok(Err(e)) => return Err(tls(e)),
            Ok(Ok(s)) => s,
        };
        TcpConnection::new(TlsStream::Client(stream), &self.config)
    }

    async fn listen(
        &self,
        bind: SocketAddr,
        policy: PeerPolicy,
    ) -> Result<TcpLinkListener, LinkError> {
        let policy: SharedPolicy = Arc::new(RwLock::new(policy));
        let cfg = mutual_server_config(&self.identity, Arc::clone(&policy), &[ALPN])?;
        let socket = TcpListener::bind(bind).await?;
        let local = socket.local_addr()?;
        let (tx, rx) = mpsc::channel(ACCEPT_QUEUE);
        let task = tokio::spawn(accept_loop(
            socket,
            TlsAcceptor::from(cfg),
            self.config.clone(),
            tx,
        ));
        Ok(TcpLinkListener {
            local,
            policy,
            rx,
            task,
        })
    }
}

/// A listening socket; stops accepting when dropped.
pub struct TcpLinkListener {
    local: SocketAddr,
    policy: SharedPolicy,
    rx: mpsc::Receiver<TcpConnection>,
    task: JoinHandle<()>,
}

impl std::fmt::Debug for TcpLinkListener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TcpLinkListener")
            .field("local", &self.local)
            .finish_non_exhaustive()
    }
}

impl Drop for TcpLinkListener {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Listener for TcpLinkListener {
    type Conn = TcpConnection;
    type Addr = SocketAddr;

    fn local_addr(&self) -> SocketAddr {
        self.local
    }

    async fn accept(&mut self) -> Result<TcpConnection, LinkError> {
        self.rx
            .recv()
            .await
            .ok_or_else(|| LinkError::Io("listener stopped".into()))
    }

    fn set_policy(&self, policy: PeerPolicy) {
        *self
            .policy
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = policy;
    }
}

/// Counts concurrent handshakes per source IP; the slot frees itself when dropped.
struct IpSlot {
    counts: Arc<Mutex<HashMap<IpAddr, usize>>>,
    ip: IpAddr,
}

impl IpSlot {
    fn acquire(
        counts: &Arc<Mutex<HashMap<IpAddr, usize>>>,
        ip: IpAddr,
        max: usize,
    ) -> Option<Self> {
        let mut c = counts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let n = c.entry(ip).or_insert(0);
        if *n >= max {
            return None;
        }
        *n += 1;
        Some(Self {
            counts: Arc::clone(counts),
            ip,
        })
    }
}

impl Drop for IpSlot {
    fn drop(&mut self) {
        let mut c = self
            .counts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(n) = c.get_mut(&self.ip) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                c.remove(&self.ip);
            }
        }
    }
}

fn canonical_ip(addr: SocketAddr) -> IpAddr {
    match addr.ip() {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        v4 => v4,
    }
}

async fn accept_loop(
    socket: TcpListener,
    acceptor: TlsAcceptor,
    config: TcpConfig,
    tx: mpsc::Sender<TcpConnection>,
) {
    let pending = Arc::new(Semaphore::new(config.max_pending_handshakes.max(1)));
    let per_ip = Arc::new(Mutex::new(HashMap::new()));
    loop {
        let (tcp, addr) = match socket.accept().await {
            Ok(x) => x,
            Err(e) => {
                tracing::debug!("link accept failed: {e}");
                tokio::time::sleep(ACCEPT_ERROR_BACKOFF).await;
                continue;
            }
        };
        if tx.is_closed() {
            return;
        }
        let Ok(permit) = Arc::clone(&pending).try_acquire_owned() else {
            continue; // too many handshakes in flight: drop the socket
        };
        let Some(slot) = IpSlot::acquire(&per_ip, canonical_ip(addr), config.max_handshakes_per_ip)
        else {
            continue;
        };
        tokio::spawn(handshake(
            tcp,
            acceptor.clone(),
            config.clone(),
            tx.clone(),
            (permit, slot),
        ));
    }
}

async fn handshake(
    tcp: TcpStream,
    acceptor: TlsAcceptor,
    config: TcpConfig,
    tx: mpsc::Sender<TcpConnection>,
    guards: (OwnedSemaphorePermit, IpSlot),
) {
    let _ = tcp.set_nodelay(true);
    let result = tokio::time::timeout(config.handshake_timeout, acceptor.accept(tcp)).await;
    drop(guards);
    let stream = match result {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            tracing::debug!("link handshake refused: {e}");
            return;
        }
        Err(_) => {
            tracing::debug!("link handshake timed out");
            return;
        }
    };
    match TcpConnection::new(TlsStream::Server(stream), &config) {
        Ok(conn) => {
            let _ = tx.send(conn).await;
        }
        Err(e) => tracing::debug!("link connection dropped: {e}"),
    }
}
