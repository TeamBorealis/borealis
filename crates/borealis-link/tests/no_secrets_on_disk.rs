#![cfg(feature = "tls")]
//! B1 acceptance: device keys and paired-device tokens are never written to the app data dir.
//! A full pairing round (own identity, peer record, token) leaves only public data on disk.

use std::path::Path;

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use borealis_link::device::load_or_create_device_identity;
use borealis_link::random::{hex, random_token};
use borealis_link::registry::registry_path;
use borealis_link::store::save_peer_token;
use borealis_link::tls::TlsIdentity;
use borealis_link::{DeviceRegistry, MemoryStore, PairedDevice, Platform, Secret};

fn files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().filter_map(Result::ok) {
        let p = e.path();
        if p.is_dir() {
            out.extend(files(&p));
        } else {
            out.push(p);
        }
    }
    out
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn keys_and_tokens_never_reach_data_files() {
    let data = tempfile::tempdir().unwrap();
    let store = MemoryStore::default();
    let me = load_or_create_device_identity(&store).unwrap();
    let peer = TlsIdentity::generate("peer").unwrap();
    let device = PairedDevice::new(
        "Pixel",
        Platform::Android,
        peer.fingerprint(),
        1_800_000_000,
    );
    let token = random_token().unwrap();
    save_peer_token(
        &store,
        &device.device_id,
        &Secret::new(token.clone().into_bytes()),
    )
    .unwrap();
    DeviceRegistry::default()
        .with_paired(device)
        .unwrap()
        .save(data.path())
        .unwrap();

    let secret = me.to_secret();
    let key = &secret.expose()[9 + me.cert_der().len()..];
    assert!(key.len() > 32, "PKCS#8 key bytes");
    let needles: Vec<(&str, Vec<u8>)> = vec![
        ("key DER", key.to_vec()),
        ("key hex", hex(key).into_bytes()),
        ("key base64", STANDARD.encode(key).into_bytes()),
        ("key base64url", URL_SAFE_NO_PAD.encode(key).into_bytes()),
        ("token", token.clone().into_bytes()),
    ];
    let written = files(data.path());
    assert_eq!(
        written,
        vec![registry_path(data.path())],
        "only the registry is written"
    );
    for f in written {
        let bytes = std::fs::read(&f).unwrap();
        for (what, needle) in &needles {
            assert!(
                !contains(&bytes, needle),
                "{what} leaked into {}",
                f.display()
            );
        }
    }
}
