//! TLS accept loop on top of hyper: handshake and header-read timeouts, global and per-IP
//! connection caps, idle auto-stop and graceful shutdown.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::ConnectInfo;
use axum::Router;
use hyper::body::Incoming;
use hyper::Request;
use hyper_util::rt::{TokioIo, TokioTimer};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, watch, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinSet;
use tokio_rustls::TlsAcceptor;
use tower_service::Service as _;

use super::{lock, HostState, SHUTDOWN_GRACE};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_HEADER_BUF: usize = 64 * 1024;
const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(50);

/// Connection caps (slowloris and fd exhaustion).
#[derive(Debug, Clone, Copy)]
pub(super) struct ConnLimits {
    pub total: usize,
    pub per_ip: usize,
    pub idle_check: Duration,
}

/// Decrements the per-IP connection count when the connection ends.
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
        let mut c = lock(counts);
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
        let mut c = lock(&self.counts);
        if let Some(n) = c.get_mut(&self.ip) {
            *n -= 1;
            if *n == 0 {
                c.remove(&self.ip);
            }
        }
    }
}

/// Serve until `stop` fires (or its sender is dropped) or the host has been idle too long.
pub(super) async fn run(
    listener: TcpListener,
    acceptor: TlsAcceptor,
    app: Router,
    st: Arc<HostState>,
    limits: ConnLimits,
    mut stop: oneshot::Receiver<()>,
) {
    let slots = Arc::new(Semaphore::new(limits.total.max(1)));
    let per_ip = Arc::new(Mutex::new(HashMap::new()));
    let (closing_tx, closing_rx) = watch::channel(false);
    let mut conns = JoinSet::new();
    let mut idle_tick = tokio::time::interval(limits.idle_check);
    loop {
        tokio::select! {
            _ = &mut stop => break,
            _ = idle_tick.tick() => {
                if st.idle_expired() {
                    tracing::info!("ai host idle; stopping");
                    break;
                }
            }
            Some(_) = conns.join_next(), if !conns.is_empty() => {}
            acc = listener.accept() => {
                let Ok((tcp, peer)) = acc else {
                    tokio::time::sleep(ACCEPT_ERROR_BACKOFF).await;
                    continue;
                };
                let Ok(permit) = Arc::clone(&slots).try_acquire_owned() else { continue };
                let Some(ip_slot) = IpSlot::acquire(&per_ip, peer.ip().to_canonical(), limits.per_ip) else {
                    continue;
                };
                conns.spawn(connection(
                    tcp,
                    peer,
                    acceptor.clone(),
                    app.clone(),
                    Arc::clone(&st),
                    closing_rx.clone(),
                    (permit, ip_slot),
                ));
            }
        }
    }
    st.mark_stopped();
    let _ = closing_tx.send(true);
    let drain = async { while conns.join_next().await.is_some() {} };
    let _ = tokio::time::timeout(SHUTDOWN_GRACE, drain).await;
    conns.abort_all(); // kills lingering SSE streams, which cancels their tasks
}

async fn connection(
    tcp: TcpStream,
    peer: SocketAddr,
    acceptor: TlsAcceptor,
    app: Router,
    st: Arc<HostState>,
    mut closing: watch::Receiver<bool>,
    _slots: (OwnedSemaphorePermit, IpSlot),
) {
    let tls = match tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(tcp)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            tracing::debug!(%peer, error = %e, "tls handshake failed");
            return;
        }
        Err(_) => return,
    };
    let svc = hyper::service::service_fn(move |mut req: Request<Incoming>| {
        req.extensions_mut().insert(ConnectInfo(peer));
        st.touch();
        app.clone().call(req)
    });
    let mut http = hyper::server::conn::http1::Builder::new();
    http.timer(TokioTimer::new())
        .header_read_timeout(HEADER_READ_TIMEOUT)
        .max_buf_size(MAX_HEADER_BUF);
    let conn = http.serve_connection(TokioIo::new(tls), svc);
    tokio::pin!(conn);
    let closing = async move {
        let _ = closing.wait_for(|c| *c).await;
    };
    tokio::select! {
        _ = conn.as_mut() => return,
        () = closing => {}
    }
    conn.as_mut().graceful_shutdown();
    let _ = conn.await;
}
