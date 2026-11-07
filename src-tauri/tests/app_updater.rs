//! Install and restart against a local update feed (#25, UP2), as the update-check tests answer from a fixture
//! (B-41). A mock Tauri app registers the real updater plugin with a public key this test makes; a local HTTP server
//! serves a fixture `latest.json` and the update file it points at. The tests run the app's own check and download
//! (`self_update_plugin::find_update`, `download`), which verify the signature. Nothing is ever installed.
#![cfg(self_update)]

use base64::Engine as _;
use borealis_lib::self_update::{InstallFailure, InstallProgress};
use borealis_lib::self_update_plugin::{download, find_update};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;

const VERSION: &str = "99.0.0";
const PAYLOAD: &[u8] = b"Borealis 99.0.0 update file (fixture)";

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// A key pair made for this run, in the formats `tauri signer` writes: the public key and each signature are the
/// base64 of minisign's text files, and the trusted comment carries the version the file was signed for.
struct Signer(minisign::KeyPair);

impl Signer {
    fn new() -> Self {
        Self(minisign::KeyPair::generate_unencrypted_keypair().expect("key pair"))
    }

    fn pubkey(&self) -> String {
        b64(self
            .0
            .pk
            .to_box()
            .expect("public key")
            .to_string()
            .as_bytes())
    }

    fn sign(&self, data: &[u8], version: &str) -> String {
        let trusted = format!("timestamp:1\tfile:Borealis.update\tversion:{version}");
        let sig = minisign::sign(
            Some(&self.0.pk),
            &self.0.sk,
            std::io::Cursor::new(data),
            Some(&trusted),
            Some("signature from tauri secret key"),
        )
        .expect("signature");
        b64(sig.to_string().as_bytes())
    }
}

/// Serves fixed answers on 127.0.0.1 (path -> status and body) until the test process ends.
fn serve(routes: HashMap<String, (u16, Vec<u8>)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    let routes = Arc::new(routes);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let routes = Arc::clone(&routes);
            std::thread::spawn(move || {
                let mut request = Vec::new();
                let mut buf = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                    }
                }
                let text = String::from_utf8_lossy(&request);
                let path = text.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (status, body) = routes
                    .get(&path)
                    .cloned()
                    .unwrap_or((404, b"not found".to_vec()));
                let head = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            });
        }
    });
    base
}

/// The `latest.json` key this host's updater looks for (`{os}-{arch}`, as the release workflow writes it).
fn platform_key() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    format!("{os}-{}", std::env::consts::ARCH)
}

fn feed(version: &str, url: &str, signature: &str, platform: &str) -> Vec<u8> {
    serde_json::json!({
        "version": version,
        "notes": format!("Borealis {version}"),
        "pub_date": "2026-11-12T18:00:00Z",
        "platforms": { platform: { "signature": signature, "url": url } }
    })
    .to_string()
    .into_bytes()
}

/// A mock app (version 0.1.0) with the updater plugin configured like `tauri.conf.json`, but with this run's key and
/// the local feed.
fn app(pubkey: &str, endpoint: &str) -> tauri::App<tauri::test::MockRuntime> {
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().plugins.0.insert(
        "updater".into(),
        serde_json::json!({
            "pubkey": pubkey,
            "endpoints": [endpoint],
            "requireSignedVersion": true,
            "dangerousInsecureTransportProtocol": true
        }),
    );
    tauri::test::mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(context)
        .expect("mock app")
}

struct Case {
    signer: Signer,
    app: tauri::App<tauri::test::MockRuntime>,
}

/// A feed announcing `version`, whose entry for `platform` points at `served` with a signature of `signed` for
/// `signed_version`.
fn case(version: &str, platform: &str, served: &[u8], signed: &[u8], signed_version: &str) -> Case {
    let signer = Signer::new();
    let signature = signer.sign(signed, signed_version);
    // The feed names the file's address, so the server starts with the update file and gets the feed second.
    let listener_base = serve(HashMap::from([(
        "/Borealis.update".to_string(),
        (200, served.to_vec()),
    )]));
    let url = format!("{listener_base}/Borealis.update");
    let feed_base = serve(HashMap::from([(
        "/latest.json".to_string(),
        (200, feed(version, &url, &signature, platform)),
    )]));
    let app = app(&signer.pubkey(), &format!("{feed_base}/latest.json"));
    Case { signer, app }
}

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tauri::async_runtime::block_on(f)
}

#[test]
fn a_signed_update_from_the_feed_is_found_downloaded_and_verified() {
    let c = case(VERSION, &platform_key(), PAYLOAD, PAYLOAD, VERSION);
    let handle = c.app.handle();
    let update = block_on(find_update(handle, Some(VERSION))).expect("update");
    assert_eq!(update.version, VERSION);
    let mut progress: Vec<InstallProgress> = Vec::new();
    let bytes = block_on(download(&update, |p| progress.push(p))).expect("download");
    assert_eq!(bytes, PAYLOAD);
    let last = progress.last().expect("progress events");
    assert_eq!(last.downloaded, PAYLOAD.len() as u64);
    assert_eq!(last.total, Some(PAYLOAD.len() as u64));
    // The notice's version may be written with a leading v.
    assert!(block_on(find_update(handle, Some("v99.0.0"))).is_ok());
    assert!(c.signer.pubkey().len() > 40);
}

#[test]
fn a_tampered_download_is_refused() {
    let c = case(
        VERSION,
        &platform_key(),
        b"something else",
        PAYLOAD,
        VERSION,
    );
    let update = block_on(find_update(c.app.handle(), Some(VERSION))).expect("update");
    assert_eq!(
        block_on(download(&update, |_| {})).err(),
        Some(InstallFailure::BadSignature)
    );
}

#[test]
fn a_signature_made_for_another_version_is_refused() {
    let c = case(VERSION, &platform_key(), PAYLOAD, PAYLOAD, "98.0.0");
    let update = block_on(find_update(c.app.handle(), Some(VERSION))).expect("update");
    assert_eq!(
        block_on(download(&update, |_| {})).err(),
        Some(InstallFailure::BadSignature)
    );
}

#[test]
fn a_signature_from_another_key_is_refused() {
    let c = case(VERSION, &platform_key(), PAYLOAD, PAYLOAD, VERSION);
    let other = Signer::new();
    let other_app = app(&other.pubkey(), &endpoint_of(&c));
    let update = block_on(find_update(other_app.handle(), Some(VERSION))).expect("update");
    assert_eq!(
        block_on(download(&update, |_| {})).err(),
        Some(InstallFailure::BadSignature)
    );
}

/// The feed address the case's app was configured with.
fn endpoint_of(c: &Case) -> String {
    c.app.config().plugins.0["updater"]["endpoints"][0]
        .as_str()
        .expect("endpoint")
        .to_string()
}

#[test]
fn a_feed_without_this_platform_offers_no_install() {
    let c = case(VERSION, "plan9-mips", PAYLOAD, PAYLOAD, VERSION);
    assert_eq!(
        block_on(find_update(c.app.handle(), Some(VERSION))).err(),
        Some(InstallFailure::NotForThisPlatform)
    );
}

#[test]
fn a_feed_for_another_version_than_the_notice_installs_nothing() {
    let c = case(VERSION, &platform_key(), PAYLOAD, PAYLOAD, VERSION);
    assert_eq!(
        block_on(find_update(c.app.handle(), Some("98.0.0"))).err(),
        Some(InstallFailure::NotReady)
    );
}

#[test]
fn a_feed_that_is_not_newer_offers_nothing() {
    let c = case("0.0.1", &platform_key(), PAYLOAD, PAYLOAD, "0.0.1");
    assert_eq!(
        block_on(find_update(c.app.handle(), None)).err(),
        Some(InstallFailure::NotReady)
    );
}

#[test]
fn a_release_without_a_feed_is_not_ready_and_offline_is_a_network_failure() {
    let signer = Signer::new();
    let missing = serve(HashMap::new());
    let app404 = app(&signer.pubkey(), &format!("{missing}/latest.json"));
    assert_eq!(
        block_on(find_update(app404.handle(), Some(VERSION))).err(),
        Some(InstallFailure::NotReady)
    );
    let closed = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = closed.local_addr().expect("addr").port();
    drop(closed);
    let offline = app(
        &signer.pubkey(),
        &format!("http://127.0.0.1:{port}/latest.json"),
    );
    assert_eq!(
        block_on(find_update(offline.handle(), Some(VERSION))).err(),
        Some(InstallFailure::Network)
    );
}
