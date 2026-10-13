//! Security regressions: signed update indexes (trust on first use), mandatory entry hashes,
//! and reserved bundled pack ids.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_lib::content_dto::{ImportPackRequest, PackKind};
use borealis_lib::content_index::{sha256_hex, Fetcher};
use borealis_lib::content_packs::NullSink;
use borealis_lib::state::{AppPaths, AppState};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;

const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<elements>
  <info><name>Sec Book</name></info>
  <element name="Sec Gift" type="Feat" source="Sec Book" id="ID_SEC_FEAT_GIFT">
    <description><p>Test.</p></description>
    <sheet display="true"><description>Test.</description></sheet>
    <setters><set name="category">General</set></setters>
  </element>
</elements>"#;

const INDEX_URL: &str = "https://example.invalid/sec.index.json";
const SIG_URL: &str = "https://example.invalid/sec.index.json.sig";
const FILE_URL: &str = "https://example.invalid/sec-book.xml";

struct Stub(HashMap<String, Vec<u8>>);

impl Fetcher for Stub {
    fn get(&self, url: &str, _max: usize) -> Result<Vec<u8>, String> {
        self.0.get(url).cloned().ok_or_else(|| "not found".into())
    }
}

fn app() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    (dir, AppState::new(paths).expect("state"))
}

fn index_bytes(hash: Option<&str>) -> Vec<u8> {
    let attributes = hash.map_or_else(|| json!({}), |h| json!({ "sha256": h }));
    serde_json::to_vec(&json!({
        "format": "borealis-index/1",
        "info": { "name": "Sec Index" },
        "files": [{ "name": "sec-book.xml", "url": FILE_URL, "extra": { "attributes": attributes } }]
    }))
    .unwrap()
}

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn sig_file(k: &SigningKey, signed: &[u8]) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "publicKey": STANDARD.encode(k.verifying_key().to_bytes()),
        "signature": STANDARD.encode(k.sign(signed).to_bytes()),
    }))
    .unwrap()
}

fn stub(index: &[u8], sig: Option<Vec<u8>>) -> Stub {
    let mut m = HashMap::from([
        (INDEX_URL.to_string(), index.to_vec()),
        (FILE_URL.to_string(), XML.as_bytes().to_vec()),
    ]);
    if let Some(s) = sig {
        m.insert(SIG_URL.to_string(), s);
    }
    Stub(m)
}

fn good_index() -> Vec<u8> {
    index_bytes(Some(&sha256_hex(XML.as_bytes())))
}

#[test]
fn valid_signed_index_is_verified_and_installs() {
    let (_d, app) = app();
    app.add_index(INDEX_URL).unwrap();
    let idx = good_index();
    let r = app
        .update_indexes(&stub(&idx, Some(sig_file(&key(1), &idx))), &NullSink)
        .unwrap();
    assert_eq!((r.updated, r.failed.len()), (1, 0));
    assert!(app.list_indexes()[0].verified);
}

#[test]
fn unsigned_index_installs_but_is_unverified() {
    let (_d, app) = app();
    app.add_index(INDEX_URL).unwrap();
    let r = app
        .update_indexes(&stub(&good_index(), None), &NullSink)
        .unwrap();
    assert_eq!(r.updated, 1);
    assert!(!app.list_indexes()[0].verified);
}

#[test]
fn tampered_index_is_rejected() {
    let (_d, app) = app();
    app.add_index(INDEX_URL).unwrap();
    let idx = good_index();
    let sig = sig_file(&key(1), &idx);
    let mut tampered = idx.clone();
    tampered.extend_from_slice(b"\n");
    let r = app
        .update_indexes(&stub(&tampered, Some(sig)), &NullSink)
        .unwrap();
    assert_eq!((r.updated, r.failed), (0, vec![INDEX_URL.to_string()]));
    assert!(!app.list_indexes()[0].verified);
}

#[test]
fn changed_key_or_dropped_signature_is_rejected_after_first_use() {
    let (_d, app) = app();
    app.add_index(INDEX_URL).unwrap();
    let idx = good_index();
    app.update_indexes(&stub(&idx, Some(sig_file(&key(1), &idx))), &NullSink)
        .unwrap();
    // A valid signature from a different key.
    let r = app
        .update_indexes(&stub(&idx, Some(sig_file(&key(2), &idx))), &NullSink)
        .unwrap();
    assert_eq!(r.failed, [INDEX_URL]);
    // The signature was stripped.
    let r = app.update_indexes(&stub(&idx, None), &NullSink).unwrap();
    assert_eq!(r.failed, [INDEX_URL]);
    // The pinned key still works.
    let r = app
        .update_indexes(&stub(&idx, Some(sig_file(&key(1), &idx))), &NullSink)
        .unwrap();
    assert!(r.failed.is_empty());
    assert!(app.list_indexes()[0].verified);
}

#[test]
fn entry_without_sha256_is_refused() {
    let (_d, app) = app();
    app.add_index(INDEX_URL).unwrap();
    let r = app
        .update_indexes(&stub(&index_bytes(None), None), &NullSink)
        .unwrap();
    assert_eq!((r.updated, r.failed), (0, vec![FILE_URL.to_string()]));
}

fn request(name: &str, body: &str) -> ImportPackRequest {
    ImportPackRequest {
        file_name: name.into(),
        content_base64: STANDARD.encode(body),
        format: None,
    }
}

fn content_json(id: &str) -> String {
    json!({
        "format": "borealis-content/1",
        "elements": [{ "id": id, "name": "X", "type": "Feat", "source": "Core" }]
    })
    .to_string()
}

#[test]
fn manual_import_named_like_a_bundled_pack_gets_a_user_prefix() {
    let (dir, app) = app();
    let before = app.list_packs().unwrap();
    for name in ["core.xml", "srd-5.1.xml", "SRD 5.2.1.xml"] {
        let r = app.import_pack(&request(name, XML), &NullSink).unwrap();
        let pack = r.pack.expect("installed");
        assert!(pack.id.starts_with("user-"), "{name} -> {}", pack.id);
    }
    assert!(!dir.path().join("library/core").exists());
    let after = app.list_packs().unwrap();
    for p in before.iter().filter(|p| p.kind == PackKind::Bundled) {
        assert!(
            after
                .iter()
                .any(|a| a.id == p.id && a.kind == PackKind::Bundled),
            "{}",
            p.id
        );
    }
}

#[test]
fn overriding_bundled_elements_is_reported_as_a_warning() {
    let (_d, app) = app();
    let body = content_json("ID_EXPERTISE_SKILL_ACROBATICS");
    let r = app
        .import_pack(&request("homebrew.json", &body), &NullSink)
        .unwrap();
    assert!(r
        .report
        .issues
        .iter()
        .any(|i| i.severity == "warning" && i.message.contains("ID_EXPERTISE_SKILL_ACROBATICS")));
    let clean = app
        .import_pack(
            &request("other.json", &content_json("ID_SEC_UNIQUE")),
            &NullSink,
        )
        .unwrap();
    assert!(clean.report.issues.is_empty());
}
