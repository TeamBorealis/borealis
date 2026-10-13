#![cfg(all(feature = "host", feature = "remote"))]
//! Protocol v2 security properties: certificate pinning, pairing strings, token expiry,
//! per-IP pairing limits, idle auto-stop, connection caps and private-file persistence.

mod host_lan;

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use borealis_ai::host::{
    write_private_file, AiHost, DeviceRecord, HostConfig, HostHandle, HostIdentity, CERT_FILE,
    KEY_FILE,
};
use borealis_ai::protocol::{CertFingerprint, PAIRING_URI_PREFIX};
use borealis_ai::remote::{PairedHost, RemoteClient, RemoteError};
use host_lan::{identity, loopback_config, qa, OkRunner, Target};

async fn start_with(cfg: HostConfig, known: Vec<DeviceRecord>) -> (HostHandle, Target) {
    let host = AiHost::start(cfg, identity(), Arc::new(OkRunner), None, known)
        .await
        .unwrap();
    let t = Target::of(&host);
    (host, t)
}

async fn start(tweak: impl FnOnce(&mut HostConfig)) -> (HostHandle, Target) {
    let mut cfg = loopback_config();
    tweak(&mut cfg);
    start_with(cfg, vec![]).await
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn wrong_code(code: &str) -> &'static str {
    if code == "00000000" {
        "00000001"
    } else {
        "00000000"
    }
}

#[tokio::test]
async fn pinned_client_pairs_from_the_pairing_string() {
    let (host, _) = start(|_| {}).await;
    let pc = host.begin_pairing().unwrap();
    let uri = pc.uri.to_string();
    assert!(uri.starts_with(PAIRING_URI_PREFIX), "{uri}");
    assert!(uri.contains(&host.fingerprint().to_base64url()));
    assert_eq!(pc.code.len(), 8);

    let (client, paired) = RemoteClient::pair_with_uri(&uri, "Phone").await.unwrap();
    assert_eq!(paired.fingerprint, host.fingerprint());
    assert_eq!(paired.addr, host.local_addr());
    let ttl = 30 * 24 * 60 * 60;
    assert!(paired.expires_at.abs_diff(now() + ttl) < 60);
    assert!(client.run_task(&qa(), None).await.is_ok());

    // The stored record (pin + token) is enough to reconnect.
    let json = serde_json::to_string(&paired).unwrap();
    let back: PairedHost = serde_json::from_str(&json).unwrap();
    let again = RemoteClient::from_paired(&back).unwrap();
    assert!(again.run_task(&qa(), None).await.is_ok());
}

#[tokio::test]
async fn wrong_fingerprint_is_rejected_with_a_typed_error() {
    let (host, t) = start(|_| {}).await;
    let bad = RemoteClient::new(t.addr, CertFingerprint([7; 32])).unwrap();
    assert!(matches!(bad.info().await, Err(RemoteError::PinMismatch)));

    // A pairing string carrying another certificate's pin must not leak the code or pair.
    let mut uri = host.begin_pairing().unwrap().uri;
    let real_code = uri.code.clone();
    uri.fingerprint = HostIdentity::generate().unwrap().fingerprint();
    let err = RemoteClient::pair_with_uri(&uri.to_string(), "evil")
        .await
        .unwrap_err();
    assert!(matches!(err, RemoteError::PinMismatch), "{err}");
    assert!(host.devices().is_empty());
    let mut good = t.client();
    assert!(
        good.pair(&real_code, "ok").await.is_ok(),
        "code still valid"
    );
}

#[tokio::test]
async fn a_different_host_identity_fails_the_pin() {
    let (_host, t) = start(|_| {}).await;
    let other = HostIdentity::generate().unwrap();
    let impostor = AiHost::start(loopback_config(), &other, Arc::new(OkRunner), None, vec![])
        .await
        .unwrap();
    let c = RemoteClient::new(impostor.local_addr(), t.fp).unwrap();
    assert!(matches!(c.info().await, Err(RemoteError::PinMismatch)));
}

#[tokio::test]
async fn plain_http_is_not_served() {
    let (_host, t) = start(|_| {}).await;
    let r = reqwest::Client::new()
        .get(format!("http://{}/v1/info", t.addr))
        .timeout(Duration::from_secs(5))
        .send()
        .await;
    assert!(r.is_err() || !r.unwrap().status().is_success());
}

#[tokio::test]
async fn bad_pairing_strings_are_typed_errors() {
    let err = RemoteClient::pair_with_uri("borealis-pair:v1?h=1.2.3.4:5", "x")
        .await
        .unwrap_err();
    assert!(matches!(err, RemoteError::PairingUri(_)), "{err}");
}

#[tokio::test]
async fn expired_tokens_are_rejected_and_pruned() {
    let seen = Arc::new(Mutex::new(Vec::<usize>::new()));
    let (host, t) = start(|c| c.token_ttl = Duration::ZERO).await;
    let s = Arc::clone(&seen);
    host.set_devices_hook(Arc::new(move |d| s.lock().unwrap().push(d.len())));
    let code = host.begin_pairing().unwrap().code;
    let mut c = t.client();
    assert_eq!(c.pair(&code, "p").await.unwrap().expires_in_secs, 0);
    let err = c.run_task(&qa(), None).await.unwrap_err().to_string();
    assert!(
        err.contains("unauthorized") && err.contains("expired"),
        "{err}"
    );
    assert!(host.devices().is_empty());
    assert_eq!(*seen.lock().unwrap(), [1, 0], "pair then prune persisted");
}

#[tokio::test]
async fn expired_known_devices_are_dropped_at_start() {
    let rec = |id: &str, expires_at| DeviceRecord {
        id: id.into(),
        name: id.into(),
        token_hash: format!("{id}-hash"),
        expires_at,
    };
    let known = vec![rec("old", 0), rec("new", now() + 3600)];
    let (host, _) = start_with(loopback_config(), known).await;
    let ids: Vec<_> = host.devices().into_iter().map(|d| d.id).collect();
    assert_eq!(ids, ["new"]);
}

#[tokio::test]
async fn wrong_guesses_from_one_ip_do_not_burn_the_code_for_another() {
    if std::net::TcpListener::bind("[::1]:0").is_err() {
        eprintln!("skipped: no IPv6 loopback");
        return;
    }
    let mut cfg = loopback_config();
    cfg.bind = Some("[::]:0".parse().unwrap()); // dual stack: 127.0.0.1 and ::1 are two senders
    cfg.max_pairing_attempts = 3;
    let (host, t) = start_with(cfg, vec![]).await;
    let port = host.local_addr().port();
    let code = host.begin_pairing().unwrap().code;

    let mut a = RemoteClient::new(([127, 0, 0, 1], port).into(), t.fp).unwrap();
    for _ in 0..3 {
        assert!(a.pair(wrong_code(&code), "a").await.is_err());
    }
    let locked = a.pair(&code, "a").await.unwrap_err().to_string();
    assert!(
        locked.contains("pairing_failed"),
        "A is locked out: {locked}"
    );

    let v6: std::net::SocketAddr = format!("[::1]:{port}").parse().unwrap();
    let mut b = RemoteClient::new(v6, t.fp).unwrap();
    b.pair(&code, "b").await.expect("B can still use the code");
    assert_eq!(host.devices().len(), 1);
}

#[tokio::test]
async fn global_failure_ceiling_destroys_the_code() {
    let (host, t) = start(|c| {
        c.max_pairing_attempts = 100;
        c.max_pairing_failures_total = 3;
    })
    .await;
    let code = host.begin_pairing().unwrap().code;
    let mut c = t.client();
    for _ in 0..3 {
        assert!(c.pair(wrong_code(&code), "x").await.is_err());
    }
    assert!(c.pair(&code, "x").await.is_err(), "code destroyed");
    assert!(host.devices().is_empty());
}

#[tokio::test]
async fn devices_hook_runs_on_pair_unpair_and_revoke() {
    let seen = Arc::new(Mutex::new(Vec::<usize>::new()));
    let (host, t) = start(|_| {}).await;
    let s = Arc::clone(&seen);
    host.set_devices_hook(Arc::new(move |d| s.lock().unwrap().push(d.len())));
    let mut c = t.client();
    c.pair(&host.begin_pairing().unwrap().code, "one")
        .await
        .unwrap();
    let id = host.devices()[0].id.clone();
    assert!(host.revoke(&id));
    let mut c2 = t.client();
    c2.pair(&host.begin_pairing().unwrap().code, "two")
        .await
        .unwrap();
    c2.unpair().await.unwrap();
    assert_eq!(*seen.lock().unwrap(), [1, 0, 1, 0]);
}

#[tokio::test]
async fn idle_host_stops_itself() {
    let (host, t) = start(|c| c.idle_timeout = Duration::from_millis(100)).await;
    assert!(host.is_running());
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!host.is_running());
    assert!(t.client().info().await.is_err());

    let (forever, t2) = start(|c| c.idle_timeout = Duration::ZERO).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(forever.is_running());
    assert!(t2.client().info().await.is_ok());
}

#[tokio::test]
async fn per_ip_connection_cap_holds_off_extra_connections() {
    let (_host, t) = start(|c| c.max_connections_per_ip = 1).await;
    let idle = tokio::net::TcpStream::connect(t.addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        t.client().info().await.is_err(),
        "second connection refused"
    );
    drop(idle);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(t.client().info().await.is_ok());
}

#[test]
fn private_file_write_is_atomic_and_0600() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ai-host.json");
    write_private_file(&path, b"[1]").unwrap();
    #[cfg(unix)]
    let first_inode = {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let m = std::fs::metadata(&path).unwrap();
        assert_eq!(m.permissions().mode() & 0o777, 0o600);
        m.ino()
    };
    write_private_file(&path, b"[2]").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"[2]");
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let m = std::fs::metadata(&path).unwrap();
        assert_eq!(m.permissions().mode() & 0o777, 0o600);
        assert_ne!(
            m.ino(),
            first_inode,
            "replaced by rename, not rewritten in place"
        );
    }
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["ai-host.json"], "no temp files left behind");
}

#[test]
fn identity_persists_and_its_key_is_private() {
    let dir = tempfile::tempdir().unwrap();
    let a = HostIdentity::load_or_create(dir.path()).unwrap();
    let b = HostIdentity::load_or_create(dir.path()).unwrap();
    assert_eq!(a.fingerprint(), b.fingerprint());
    assert_eq!(a.fingerprint(), CertFingerprint::of_der(a.cert_der()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let m = std::fs::metadata(dir.path().join(KEY_FILE)).unwrap();
        assert_eq!(m.permissions().mode() & 0o777, 0o600);
    }
    std::fs::write(dir.path().join(CERT_FILE), b"garbage").unwrap();
    let c = HostIdentity::load_or_create(dir.path()).unwrap();
    assert_ne!(
        c.fingerprint(),
        a.fingerprint(),
        "unusable identity is replaced"
    );
}
