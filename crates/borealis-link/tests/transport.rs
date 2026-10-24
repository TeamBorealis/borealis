#![cfg(feature = "tcp")]
//! TCP + TLS transport over real loopback sockets: mutual pinning both ways, typed pin
//! mismatch, policy changes on a running listener, frame caps and channel binding.

use std::net::SocketAddr;
use std::time::Duration;

use borealis_link::tls::TlsIdentity;
use borealis_link::transport::tcp::{TcpConfig, TcpTransport};
use borealis_link::{Connection, LinkError, Listener, PeerPolicy, Transport};

const LOOPBACK: &str = "127.0.0.1:0";

fn device(name: &str) -> (TcpTransport, TlsIdentity) {
    let id = TlsIdentity::generate(name).unwrap();
    let cfg = TcpConfig {
        handshake_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(5),
        max_frame: 64 * 1024,
        ..TcpConfig::default()
    };
    (TcpTransport::new(id.clone(), cfg), id)
}

fn addr() -> SocketAddr {
    LOOPBACK.parse().unwrap()
}

#[tokio::test]
async fn paired_devices_exchange_frames_both_ways() {
    let (desk, desk_id) = device("desk");
    let (phone, phone_id) = device("phone");
    let mut listener = desk
        .listen(addr(), PeerPolicy::Pinned(phone_id.fingerprint()))
        .await
        .unwrap();
    let to = listener.local_addr();
    let server = tokio::spawn(async move {
        let mut conn = listener.accept().await.unwrap();
        let got = conn.recv().await.unwrap().unwrap();
        conn.send(&[got.as_slice(), b" back"].concat())
            .await
            .unwrap();
        let ekm = conn
            .export_keying_material(b"borealis test", b"", 32)
            .unwrap();
        assert!(
            conn.recv().await.unwrap().is_none(),
            "client closed cleanly"
        );
        (conn.peer().fingerprint, ekm)
    });
    let mut conn = phone
        .connect(to, PeerPolicy::Pinned(desk_id.fingerprint()))
        .await
        .unwrap();
    assert_eq!(conn.peer().fingerprint, desk_id.fingerprint());
    conn.send(b"hello").await.unwrap();
    assert_eq!(conn.recv().await.unwrap().unwrap(), b"hello back");
    let ekm = conn
        .export_keying_material(b"borealis test", b"", 32)
        .unwrap();
    conn.close().await.unwrap();
    let (seen_by_server, server_ekm) = server.await.unwrap();
    assert_eq!(
        seen_by_server,
        phone_id.fingerprint(),
        "server proved the client"
    );
    assert_eq!(ekm, server_ekm, "both ends derive the same channel binding");
    assert_eq!(ekm.len(), 32);
}

#[tokio::test]
async fn a_server_with_another_certificate_is_a_typed_pin_mismatch() {
    let (desk, _) = device("desk");
    let (phone, _) = device("phone");
    let (_, impostor) = device("impostor");
    let listener = desk
        .listen(addr(), PeerPolicy::AnyCertificate)
        .await
        .unwrap();
    let err = phone
        .connect(
            listener.local_addr(),
            PeerPolicy::Pinned(impostor.fingerprint()),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, LinkError::PinMismatch), "{err:?}");
}

/// In TLS 1.3 the server checks the client certificate after the client already finished, so
/// a refused client sees the failure on its first read; the listener never yields it.
async fn refused(
    listener: &mut impl Listener,
    client: &TcpTransport,
    pin: PeerPolicy,
    to: SocketAddr,
) {
    if let Ok(mut c) = client.connect(to, pin).await {
        let _ = c.send(b"x").await;
        assert!(
            c.recv().await.is_err(),
            "refused client must not get a frame"
        );
    }
    let accepted = tokio::time::timeout(Duration::from_millis(300), listener.accept()).await;
    assert!(
        accepted.is_err(),
        "listener must not yield a refused client"
    );
}

#[tokio::test]
async fn unknown_clients_are_refused_and_revocation_applies_to_new_handshakes() {
    let (desk, desk_id) = device("desk");
    let (phone, phone_id) = device("phone");
    let (stranger, _) = device("stranger");
    let mut listener = desk
        .listen(addr(), PeerPolicy::AnyOf(vec![phone_id.fingerprint()]))
        .await
        .unwrap();
    let to = listener.local_addr();
    let pin = PeerPolicy::Pinned(desk_id.fingerprint());

    refused(&mut listener, &stranger, pin.clone(), to).await;

    let ok = phone.connect(to, pin.clone()).await.unwrap();
    let accepted = listener.accept().await.unwrap();
    assert_eq!(accepted.peer().fingerprint, phone_id.fingerprint());
    drop((ok, accepted));

    listener.set_policy(PeerPolicy::AnyOf(Vec::new())); // phone removed in Settings > Devices
    refused(&mut listener, &phone, pin, to).await;
}

#[tokio::test]
async fn oversized_frames_are_refused() {
    let (desk, desk_id) = device("desk");
    let (phone, _) = device("phone");
    let mut listener = desk
        .listen(addr(), PeerPolicy::AnyCertificate)
        .await
        .unwrap();
    let to = listener.local_addr();
    let server = tokio::spawn(async move { listener.accept().await.unwrap() });
    let mut conn = phone
        .connect(to, PeerPolicy::Pinned(desk_id.fingerprint()))
        .await
        .unwrap();
    let _server_conn = server.await.unwrap();
    let err = conn.send(&vec![0u8; 64 * 1024 + 1]).await.unwrap_err();
    assert!(matches!(err, LinkError::FrameTooLarge { .. }), "{err:?}");
    conn.send(&vec![0u8; 64 * 1024]).await.unwrap();
}

#[tokio::test]
async fn idle_peers_time_out() {
    let (desk, desk_id) = device("desk");
    let id = TlsIdentity::generate("phone").unwrap();
    let phone = TcpTransport::new(
        id,
        TcpConfig {
            idle_timeout: Duration::from_millis(200),
            ..TcpConfig::default()
        },
    );
    let mut listener = desk
        .listen(addr(), PeerPolicy::AnyCertificate)
        .await
        .unwrap();
    let to = listener.local_addr();
    let server = tokio::spawn(async move { listener.accept().await.unwrap() });
    let mut conn = phone
        .connect(to, PeerPolicy::Pinned(desk_id.fingerprint()))
        .await
        .unwrap();
    let _keep = server.await.unwrap();
    assert!(matches!(conn.recv().await, Err(LinkError::Timeout(_))));
}
