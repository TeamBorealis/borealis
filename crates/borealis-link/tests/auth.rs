#![cfg(feature = "tcp")]
//! B2 session authentication over real loopback TLS: QR secret proof, SPAKE2 typed codes, the
//! one-guess policy, expiry, and a man in the middle that terminates TLS on both sides.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use borealis_link::auth::{
    accept_auth, dial_auth, AuthError, AuthFailure, AuthWindow, Authenticated, Credential,
};
use borealis_link::tls::TlsIdentity;
use borealis_link::transport::tcp::{TcpConfig, TcpConnection, TcpTransport};
use borealis_link::{
    CertFingerprint, Connection, LinkError, LinkSecret, Listener, PeerPolicy, Transport, TypedCode,
};

fn device(name: &str) -> (TcpTransport, CertFingerprint) {
    let id = TlsIdentity::generate(name).unwrap();
    let cfg = TcpConfig {
        handshake_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(5),
        ..TcpConfig::default()
    };
    let fp = id.fingerprint();
    (TcpTransport::new(id, cfg), fp)
}

fn loopback() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

fn code(s: &str) -> TypedCode {
    s.parse().unwrap()
}

/// A listener device with an open window; returns its address, fingerprint, window and the
/// task that authenticates the first connection.
async fn listener(
    window: AuthWindow,
) -> (
    SocketAddr,
    CertFingerprint,
    Arc<AuthWindow>,
    tokio::task::JoinHandle<Result<Authenticated, AuthFailure>>,
) {
    let (desk, fp) = device("desk");
    let mut l = desk
        .listen(loopback(), PeerPolicy::AnyCertificate)
        .await
        .unwrap();
    let addr = l.local_addr();
    let window = Arc::new(window);
    let w = Arc::clone(&window);
    let task = tokio::spawn(async move {
        let mut conn = l.accept().await.unwrap();
        accept_auth(&mut conn, &w).await
    });
    (addr, fp, window, task)
}

fn window(c: &str) -> AuthWindow {
    AuthWindow::with(
        LinkSecret::generate().unwrap(),
        code(c),
        Duration::from_secs(300),
    )
}

#[tokio::test]
async fn qr_secret_authenticates_both_sides_with_the_same_pair_key() {
    let w = window("12-oak-raven");
    let secret = w.secret().clone();
    let (addr, fp, window, task) = listener(w).await;
    let (phone, phone_fp) = device("phone");
    let mut conn = phone.connect(addr, PeerPolicy::Pinned(fp)).await.unwrap();
    let mine = dial_auth(&mut conn, &Credential::Qr(secret)).await.unwrap();
    let theirs = task.await.unwrap().unwrap();
    assert_eq!(mine.peer.fingerprint, fp);
    assert_eq!(theirs.peer.fingerprint, phone_fp);
    assert_eq!(mine.pair_key.expose(), theirs.pair_key.expose());
    assert!(!window.is_open(), "single use");
}

#[tokio::test]
async fn typed_code_authenticates_with_spake2() {
    let (addr, _, window, task) = listener(window("42-ember-lantern")).await;
    let (pc, _) = device("pc2");
    let mut conn = pc.connect(addr, PeerPolicy::AnyCertificate).await.unwrap();
    let mine = dial_auth(&mut conn, &Credential::Code(code("42 Ember Lantern")))
        .await
        .unwrap();
    let theirs = task.await.unwrap().unwrap();
    assert_eq!(mine.pair_key.expose(), theirs.pair_key.expose());
    assert!(!window.is_open() && !window.is_dead());
}

#[tokio::test]
async fn one_wrong_code_kills_the_code() {
    let (pc, _) = device("pc2");
    let (addr, _, window, task) = listener(window("42-ember-lantern")).await;
    let mut a = pc.connect(addr, PeerPolicy::AnyCertificate).await.unwrap();
    let err = dial_auth(&mut a, &Credential::Code(code("42-ember-oak")))
        .await
        .unwrap_err();
    assert_eq!(err.auth(), Some(AuthError::Mismatch), "{err}");
    // The dialer hangs up after the listener's confirmation fails to verify; the listener sees
    // the attempt end without a valid confirmation.
    assert!(task.await.unwrap().is_err());
    assert!(window.is_dead(), "one wrong guess kills the code");
}

#[tokio::test]
async fn a_dead_code_refuses_the_right_code_too() {
    let (desk, _) = device("desk");
    let mut l = desk
        .listen(loopback(), PeerPolicy::AnyCertificate)
        .await
        .unwrap();
    let addr = l.local_addr();
    let w = Arc::new(window("42-ember-lantern"));
    let lw = Arc::clone(&w);
    let task = tokio::spawn(async move {
        let mut out = Vec::new();
        for _ in 0..2 {
            let mut conn = l.accept().await.unwrap();
            out.push(
                accept_auth(&mut conn, &lw)
                    .await
                    .err()
                    .and_then(|e| e.auth()),
            );
        }
        out
    });
    let (pc, _) = device("pc2");
    let mut a = pc.connect(addr, PeerPolicy::AnyCertificate).await.unwrap();
    assert!(dial_auth(&mut a, &Credential::Code(code("42-oak-oak")))
        .await
        .is_err());
    let mut b = pc.connect(addr, PeerPolicy::AnyCertificate).await.unwrap();
    assert!(
        dial_auth(&mut b, &Credential::Code(code("42-ember-lantern")))
            .await
            .is_err()
    );
    assert_eq!(
        task.await.unwrap(),
        vec![None, Some(AuthError::Closed)],
        "first attempt ends with the dialer hanging up, second finds the code dead"
    );
}

#[tokio::test]
async fn wrong_qr_secret_is_refused_and_kills_the_window() {
    let (addr, fp, window, task) = listener(window("3-oak-raven")).await;
    let (phone, _) = device("phone");
    let mut conn = phone.connect(addr, PeerPolicy::Pinned(fp)).await.unwrap();
    let err = dial_auth(&mut conn, &Credential::Qr(LinkSecret::generate().unwrap()))
        .await
        .unwrap_err();
    assert_eq!(err.auth(), Some(AuthError::Mismatch));
    assert_eq!(
        task.await.unwrap().unwrap_err().auth(),
        Some(AuthError::Mismatch)
    );
    assert!(window.is_dead());
}

#[tokio::test]
async fn an_expired_window_refuses() {
    let w = AuthWindow::with(
        LinkSecret::generate().unwrap(),
        code("5-oak-raven"),
        Duration::ZERO,
    );
    let (addr, _, _, task) = listener(w).await;
    let (pc, _) = device("pc2");
    let mut conn = pc.connect(addr, PeerPolicy::AnyCertificate).await.unwrap();
    assert!(dial_auth(&mut conn, &Credential::Code(code("5-oak-raven")))
        .await
        .is_err());
    assert_eq!(
        task.await.unwrap().unwrap_err().auth(),
        Some(AuthError::Expired)
    );
}

/// Relay every frame between two connections until either side closes.
async fn relay(mut a: TcpConnection, mut b: TcpConnection) {
    loop {
        tokio::select! {
            f = a.recv() => match f { Ok(Some(f)) => { if b.send(&f).await.is_err() { return; } } _ => return },
            f = b.recv() => match f { Ok(Some(f)) => { if a.send(&f).await.is_err() { return; } } _ => return },
        }
    }
}

/// A man in the middle on the LAN: it terminates the victim's TLS with its own certificate and
/// opens its own TLS session to the real listener, relaying every message unchanged.
async fn mitm(real: SocketAddr) -> SocketAddr {
    let (evil, _) = device("evil");
    let mut l = evil
        .listen(loopback(), PeerPolicy::AnyCertificate)
        .await
        .unwrap();
    let addr = l.local_addr();
    tokio::spawn(async move {
        let victim = l.accept().await.unwrap();
        let upstream = evil
            .connect(real, PeerPolicy::AnyCertificate)
            .await
            .unwrap();
        relay(victim, upstream).await;
    });
    addr
}

#[tokio::test]
async fn a_relaying_man_in_the_middle_fails_the_typed_code_handshake() {
    let (real, _, window, task) = listener(window("42-ember-lantern")).await;
    let evil = mitm(real).await;
    let (pc, _) = device("pc2");
    let mut conn = pc.connect(evil, PeerPolicy::AnyCertificate).await.unwrap();
    let err = dial_auth(&mut conn, &Credential::Code(code("42-ember-lantern")))
        .await
        .unwrap_err();
    assert_eq!(
        err.auth(),
        Some(AuthError::Mismatch),
        "channel binding: {err}"
    );
    assert!(
        task.await.unwrap().is_err(),
        "the listener never trusts the relay"
    );
    assert!(window.is_dead());
}

#[tokio::test]
async fn a_man_in_the_middle_cannot_reuse_a_qr_proof() {
    let w = window("9-oak-raven");
    let secret = w.secret().clone();
    let (real, fp, window, task) = listener(w).await;
    let evil = mitm(real).await;
    let (phone, _) = device("phone");
    // The pin from the QR stops it at the TLS layer.
    let pinned = phone.connect(evil, PeerPolicy::Pinned(fp)).await;
    assert!(matches!(pinned, Err(LinkError::PinMismatch)));
    // Even without the pin, the relayed proof is bound to the victim's exporter.
    let evil = mitm(real).await;
    let mut conn = phone
        .connect(evil, PeerPolicy::AnyCertificate)
        .await
        .unwrap();
    assert!(dial_auth(&mut conn, &Credential::Qr(secret)).await.is_err());
    assert_eq!(
        task.await.unwrap().unwrap_err().auth(),
        Some(AuthError::Mismatch)
    );
    assert!(window.is_dead());
}
