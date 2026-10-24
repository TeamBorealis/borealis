#![cfg(feature = "tcp")]
//! Settings > Devices (C1): removing a device, or letting its pairing lapse after 180 days,
//! takes its certificate out of the registry's allowed set, so a listener built from the
//! registry refuses it at the next TLS handshake, also after a restart; the other paired
//! devices keep working and the removed device's pair key is deleted.

use std::net::SocketAddr;
use std::time::Duration;

use borealis_link::registry::{DeviceRegistry, PAIRING_TTL_DAYS};
use borealis_link::store::{forget_peer, peer_token, save_peer_token};
use borealis_link::tls::TlsIdentity;
use borealis_link::transport::tcp::{TcpConfig, TcpLinkListener, TcpTransport};
use borealis_link::{
    Connection, DeviceId, Listener, MemoryStore, PairedDevice, PeerPolicy, Platform, Secret,
    Transport,
};

const NOW: u64 = 1_800_000_000;
const DAY: u64 = 86_400;

fn device(name: &str) -> (TcpTransport, TlsIdentity) {
    let id = TlsIdentity::generate(name).unwrap();
    let cfg = TcpConfig {
        handshake_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(5),
        ..TcpConfig::default()
    };
    (TcpTransport::new(id.clone(), cfg), id)
}

fn loopback() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

fn policy(registry: &DeviceRegistry, now: u64) -> PeerPolicy {
    PeerPolicy::AnyOf(registry.allowed_fingerprints(now))
}

/// The client completes a round trip and the listener yields it.
async fn accepted(listener: &mut TcpLinkListener, client: &TcpTransport, pin: &PeerPolicy) {
    let to = listener.local_addr();
    let mut conn = client.connect(to, pin.clone()).await.unwrap();
    let mut peer = tokio::time::timeout(Duration::from_secs(5), listener.accept())
        .await
        .expect("a paired device is accepted")
        .unwrap();
    conn.send(b"ping").await.unwrap();
    assert_eq!(peer.recv().await.unwrap().unwrap(), b"ping");
}

/// TLS 1.3: a refused client sees the failure on its first read; the listener never yields it.
async fn refused(listener: &mut TcpLinkListener, client: &TcpTransport, pin: &PeerPolicy) {
    let to = listener.local_addr();
    if let Ok(mut c) = client.connect(to, pin.clone()).await {
        let _ = c.send(b"x").await;
        assert!(
            c.recv().await.is_err(),
            "a removed device must not get a frame"
        );
    }
    let got = tokio::time::timeout(Duration::from_millis(300), listener.accept()).await;
    assert!(got.is_err(), "the listener must not yield a removed device");
}

fn paired(registry: &DeviceRegistry, name: &str, id: &TlsIdentity, at: u64) -> DeviceRegistry {
    registry
        .with_paired(PairedDevice::new(name, Platform::Ios, id.fingerprint(), at))
        .unwrap()
}

#[tokio::test]
async fn a_removed_device_is_refused_at_the_next_handshake_and_after_a_restart() {
    let data = tempfile::tempdir().unwrap();
    let (desk, desk_id) = device("desk");
    let (phone, phone_id) = device("phone");
    let (tablet, tablet_id) = device("tablet");
    let pin = PeerPolicy::Pinned(desk_id.fingerprint());

    let registry = paired(&DeviceRegistry::default(), "Phone", &phone_id, NOW);
    let registry = paired(&registry, "Tablet", &tablet_id, NOW);
    registry.save(data.path()).unwrap();
    let store = MemoryStore::default();
    let phone_dev = DeviceId::of(&phone_id.fingerprint());
    let tablet_dev = DeviceId::of(&tablet_id.fingerprint());
    save_peer_token(&store, &phone_dev, &Secret::new(vec![1; 32])).unwrap();
    save_peer_token(&store, &tablet_dev, &Secret::new(vec![2; 32])).unwrap();

    let mut listener = desk
        .listen(loopback(), policy(&registry, NOW))
        .await
        .unwrap();
    accepted(&mut listener, &phone, &pin).await;
    accepted(&mut listener, &tablet, &pin).await;

    // Settings > Devices > Remove "Phone": registry without it, saved, key deleted.
    let removed = DeviceRegistry::load(data.path())
        .unwrap()
        .without(&phone_dev);
    removed.save(data.path()).unwrap();
    assert!(forget_peer(&store, &phone_dev).unwrap());
    listener.set_policy(policy(&removed, NOW));

    refused(&mut listener, &phone, &pin).await;
    accepted(&mut listener, &tablet, &pin).await;
    assert!(
        peer_token(&store, &phone_dev).unwrap().is_none(),
        "the removed device's key is gone"
    );
    assert!(
        peer_token(&store, &tablet_dev).unwrap().is_some(),
        "other devices keep theirs"
    );

    // After a restart the saved registry still refuses it.
    drop(listener);
    let reloaded = DeviceRegistry::load(data.path()).unwrap();
    assert!(reloaded.get(&phone_dev).is_none());
    let mut fresh = desk
        .listen(loopback(), policy(&reloaded, NOW))
        .await
        .unwrap();
    refused(&mut fresh, &phone, &pin).await;
    accepted(&mut fresh, &tablet, &pin).await;
}

#[tokio::test]
async fn a_pairing_unused_for_180_days_expires_and_is_refused() {
    let (desk, desk_id) = device("desk");
    let (phone, phone_id) = device("phone");
    let (tablet, tablet_id) = device("tablet");
    let pin = PeerPolicy::Pinned(desk_id.fingerprint());
    let registry = paired(&DeviceRegistry::default(), "Phone", &phone_id, NOW);
    // The tablet synced 30 days later, so it is still live when the phone lapses.
    let registry = paired(&registry, "Tablet", &tablet_id, NOW + 30 * DAY);

    let lapse = NOW + PAIRING_TTL_DAYS * DAY;
    let mut listener = desk
        .listen(loopback(), policy(&registry, lapse - 1))
        .await
        .unwrap();
    accepted(&mut listener, &phone, &pin).await;

    listener.set_policy(policy(&registry, lapse));
    refused(&mut listener, &phone, &pin).await;
    accepted(&mut listener, &tablet, &pin).await;

    let (pruned, gone) = registry.pruned(lapse);
    assert_eq!(gone, vec![DeviceId::of(&phone_id.fingerprint())]);
    assert_eq!(pruned.devices.len(), 1);
}
