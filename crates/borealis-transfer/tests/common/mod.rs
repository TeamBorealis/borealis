//! Two devices on loopback TLS, a bundle builder and a receive host over a temp data folder.
#![allow(dead_code)]

use std::collections::BTreeSet;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use borealis_bundle::{
    apply_plan, plan_receive, scan_characters, scan_packs, ApplyReport, ApplyTargets, Bundle,
    BundleMeta, BundleWriter, ItemValidator, Limits, Manifest, Plan,
};
use borealis_link::tls::TlsIdentity;
use borealis_link::transport::tcp::{TcpConfig, TcpConnection, TcpTransport};
use borealis_link::{Listener, PeerPolicy, Platform, Transport};
use borealis_transfer::{Event, Events, Local, ReceiveHost};

pub const T0: u64 = 1_700_000_000_000;

/// `(path inside the pack, bytes)`.
pub type PackFiles = Vec<(String, Vec<u8>)>;
/// Called with the bytes received so far.
pub type ProgressHook = Box<dyn FnMut(u64) + Send>;

/// A connected pair (sender side, receiver side).
pub async fn pair() -> (TcpConnection, TcpConnection) {
    let cfg = TcpConfig {
        idle_timeout: Duration::from_secs(10),
        ..TcpConfig::default()
    };
    let a = TlsIdentity::generate("a").unwrap();
    let b = TlsIdentity::generate("b").unwrap();
    let (fa, fb) = (a.fingerprint(), b.fingerprint());
    let ta = TcpTransport::new(a, cfg.clone());
    let tb = TcpTransport::new(b, cfg);
    let mut l = tb
        .listen("127.0.0.1:0".parse().unwrap(), PeerPolicy::Pinned(fa))
        .await
        .unwrap();
    let addr = l.local_addr();
    let accept = tokio::spawn(async move { l.accept().await.unwrap() });
    let ca = ta.connect(addr, PeerPolicy::Pinned(fb)).await.unwrap();
    (ca, accept.await.unwrap())
}

pub fn local(name: &str) -> Local {
    Local {
        name: name.into(),
        platform: Platform::Linux,
    }
}

pub fn character(name: &str) -> Vec<u8> {
    format!(r#"{{"format":"borealis-character","format_version":2,"meta":{{"name":"{name}"}}}}"#)
        .into_bytes()
}

/// A bundle file in `dir` with these characters and packs (`(key, files)`).
pub fn bundle_file(
    dir: &Path,
    chars: &[(&str, Vec<u8>)],
    packs: &[(&str, PackFiles)],
) -> (PathBuf, Manifest) {
    let meta = BundleMeta {
        app_version: "1.0.0".into(),
        source_device: "Sender".into(),
        created_ms: T0,
    };
    let mut w = BundleWriter::new(Cursor::new(Vec::new()), meta);
    for (key, bytes) in chars {
        w.add_character(key, bytes, T0, Vec::new()).unwrap();
    }
    for (key, files) in packs {
        w.add_pack(key, files, T0).unwrap();
    }
    let (out, manifest) = w.finish().unwrap();
    let path = dir.join("out.borealis");
    std::fs::write(&path, out.into_inner()).unwrap();
    (path, manifest)
}

/// Accepts JSON objects with `meta.name` as characters and folders with `pack.json` as packs.
pub struct TestValidator;

impl ItemValidator for TestValidator {
    fn character(&self, _: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
        let v: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        v.pointer("/meta/name")
            .and_then(serde_json::Value::as_str)
            .map(|_| bytes.to_vec())
            .ok_or_else(|| "not a character".into())
    }
    fn pack(&self, _: &str, dir: &Path) -> Result<(), String> {
        dir.join("pack.json")
            .is_file()
            .then_some(())
            .ok_or_else(|| "no pack.json".into())
    }
}

/// A receiving device's data folder.
pub struct Device {
    pub root: tempfile::TempDir,
}

impl Device {
    pub fn new() -> Self {
        Self {
            root: tempfile::tempdir().unwrap(),
        }
    }

    pub fn targets(&self) -> ApplyTargets {
        let r = self.root.path();
        ApplyTargets {
            characters_dir: r.join("characters"),
            library_dir: r.join("library"),
            trash_dir: r.join("sync/trash"),
            staging_dir: r.join("sync/staging"),
        }
    }

    /// Every file under the data folder, relative.
    pub fn files(&self) -> Vec<String> {
        fn walk(root: &Path, p: &Path, out: &mut Vec<String>) {
            for e in std::fs::read_dir(p).into_iter().flatten().flatten() {
                let path = e.path();
                if path.is_dir() {
                    walk(root, &path, out);
                } else {
                    out.push(path.strip_prefix(root).unwrap().display().to_string());
                }
            }
        }
        let mut out = Vec::new();
        walk(self.root.path(), self.root.path(), &mut out);
        out.sort();
        out
    }
}

impl ReceiveHost for Device {
    fn plan(&self, manifest: &Manifest) -> Result<Plan, String> {
        let t = self.targets();
        let mut local = scan_characters(&t.characters_dir).unwrap_or_default();
        local.extend(scan_packs(&t.library_dir, &Limits::default()).unwrap_or_default());
        Ok(plan_receive(manifest, &local, &BTreeSet::new()))
    }

    fn apply(
        &self,
        bundle: &mut Bundle<std::fs::File>,
        plan: &Plan,
    ) -> Result<ApplyReport, String> {
        apply_plan(bundle, plan, &self.targets(), &TestValidator).map_err(|e| e.to_string())
    }

    fn incoming_dir(&self) -> PathBuf {
        self.root.path().join("sync/incoming")
    }
}

/// Records events; optionally fires a callback on each.
#[derive(Default)]
pub struct Recorder {
    pub events: Mutex<Vec<Event>>,
    pub on_progress: Mutex<Option<ProgressHook>>,
}

impl Events for Recorder {
    fn event(&self, event: Event) {
        if let Event::Progress { done, .. } = &event {
            if let Some(f) = self.on_progress.lock().unwrap().as_mut() {
                f(*done);
            }
        }
        self.events.lock().unwrap().push(event);
    }
}
