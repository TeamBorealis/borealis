//! Hostile and malformed bundles: every one must be refused, none may panic.

mod common;

use borealis_bundle::{
    Bundle, BundleError, BundleMeta, BundleWriter, Item, ItemKind, Limits, Manifest,
};
use common::{
    character_item, character_json, file_entry, manifest_bytes, manifest_of, meta, raw_zip,
};
use std::io::Cursor;

fn open(bytes: Vec<u8>) -> Result<Bundle<Cursor<Vec<u8>>>, BundleError> {
    Bundle::open(Cursor::new(bytes))
}

fn good_parts() -> (Vec<u8>, Manifest) {
    let ch = character_json("A", "ID_X");
    (ch.clone(), manifest_of(vec![character_item("c_a", &ch, 5)]))
}

#[test]
fn a_well_formed_hand_built_bundle_opens() {
    let (ch, m) = good_parts();
    let z = raw_zip(&[
        ("manifest.json", &manifest_bytes(&m)),
        ("characters/c_a.json", &ch),
    ]);
    open(z).unwrap().verify_all().unwrap();
}

#[test]
fn tampered_content_is_refused_by_sha256() {
    let (_, m) = good_parts();
    let evil = character_json("EVIL", "ID_X");
    // Same length is not required: the size check or the hash check must trip either way.
    let z = raw_zip(&[
        ("manifest.json", &manifest_bytes(&m)),
        ("characters/c_a.json", &evil),
    ]);
    let mut b = open(z).unwrap();
    assert!(matches!(
        b.read_item(ItemKind::Character, "c_a"),
        Err(BundleError::HashMismatch(_) | BundleError::Inconsistent(_))
    ));

    // Same size, different bytes: must be the hash check.
    let mut same_len = character_json("A", "ID_X");
    let n = same_len.len();
    same_len[n - 5] = b'Z';
    let z = raw_zip(&[
        ("manifest.json", &manifest_bytes(&m)),
        ("characters/c_a.json", &same_len),
    ]);
    let mut b = open(z).unwrap();
    assert!(matches!(
        b.read_item(ItemKind::Character, "c_a"),
        Err(BundleError::HashMismatch(_))
    ));
    assert!(b.verify_all().is_err());
}

#[test]
fn tampered_manifest_digest_is_refused() {
    let (ch, mut m) = good_parts();
    m.items[0].sha256 = "0".repeat(64);
    let z = raw_zip(&[
        ("manifest.json", &manifest_bytes(&m)),
        ("characters/c_a.json", &ch),
    ]);
    assert!(open(z).unwrap().verify_all().is_err());
}

#[test]
fn path_traversal_and_absolute_names_are_refused() {
    let (ch, m) = good_parts();
    for bad in [
        "../evil.json",
        "characters/../../evil.json",
        "/etc/passwd",
        "C:/windows/x.json",
        "characters\\c_a.json",
        "characters//c_a.json",
        "./characters/c_a.json",
        "characters/c_a.json\0.png",
    ] {
        let z = raw_zip(&[
            ("manifest.json", &manifest_bytes(&m)),
            ("characters/c_a.json", &ch),
            (bad, b"x"),
        ]);
        assert!(
            matches!(open(z), Err(BundleError::UnsafePath(_))),
            "accepted {bad:?}"
        );
    }
}

#[test]
fn traversal_in_manifest_keys_and_paths_is_refused() {
    let ch = character_json("A", "ID_X");
    for (key, path) in [
        ("../c_a", "characters/../c_a.json"),
        ("c_a", "characters/../../c_a.json"),
        ("c_a", "characters/sub/c_a.json"),
        ("c/a", "characters/c/a.json"),
    ] {
        let mut item = character_item("c_a", &ch, 1);
        item.key = key.into();
        item.files[0].path = path.into();
        let m = manifest_of(vec![item]);
        let z = raw_zip(&[("manifest.json", &manifest_bytes(&m)), (path, &ch)]);
        assert!(open(z).is_err(), "accepted key {key:?} path {path:?}");
    }
}

#[test]
fn excluded_content_in_a_bundle_is_refused() {
    let pack = |rel: &str| -> Item {
        let f = file_entry(&format!("packs/p/{rel}"), b"{}");
        Item {
            kind: ItemKind::Pack,
            key: "p".into(),
            sha256: f.sha256.clone(),
            size: f.size,
            edited_at_ms: 1,
            files: vec![f],
            required_packs: Vec::new(),
        }
    };
    for rel in [
        "search/v1/index.bin",
        "ai-host-key.der",
        "x.pem",
        "models/m.bin",
        "settings.json",
        "import-drafts/d.json",
        "ai-profile/p.json",
    ] {
        let m = manifest_of(vec![pack(rel)]);
        let name = format!("packs/p/{rel}");
        let z = raw_zip(&[("manifest.json", &manifest_bytes(&m)), (&name, b"{}")]);
        assert!(
            matches!(open(z), Err(BundleError::Excluded(_))),
            "accepted {rel}"
        );
    }
}

/// Z: a Windows device name inside a pack (`nul.xml`, `COM1/x.json`) or as a pack key would reach
/// a device, not a file, on a Windows receiver; it is refused like any unsafe path.
#[test]
fn windows_device_names_are_refused() {
    let pack = |key: &str, rel: &str| -> (Item, String) {
        let name = format!("packs/{key}/{rel}");
        let f = file_entry(&name, b"{}");
        let item = Item {
            kind: ItemKind::Pack,
            key: key.into(),
            sha256: f.sha256.clone(),
            size: f.size,
            edited_at_ms: 1,
            files: vec![f],
            required_packs: Vec::new(),
        };
        (item, name)
    };
    for (key, rel) in [
        ("p", "nul.xml"),
        ("p", "CON"),
        ("p", "aux.tar.json"),
        ("p", "COM1/x.json"),
        ("p", "sub/lpt3.json"),
        ("NUL", "pack.json"),
    ] {
        let (item, name) = pack(key, rel);
        let m = manifest_of(vec![item]);
        let z = raw_zip(&[("manifest.json", &manifest_bytes(&m)), (&name, b"{}")]);
        assert!(
            matches!(open(z), Err(BundleError::UnsafePath(_))),
            "accepted {name}"
        );
    }
}

#[test]
fn non_portable_settings_are_refused() {
    let (ch, mut m) = good_parts();
    let settings = br#"{"playerName":"P","ai":{"token":"secret"}}"#;
    m.settings = Some(file_entry("settings.portable.json", settings));
    let z = raw_zip(&[
        ("manifest.json", &manifest_bytes(&m)),
        ("characters/c_a.json", &ch),
        ("settings.portable.json", settings),
    ]);
    let mut b = open(z).unwrap();
    assert!(matches!(b.read_settings(), Err(BundleError::Excluded(_))));
}

#[test]
fn unlisted_and_missing_entries_are_refused() {
    let (ch, m) = good_parts();
    let extra = raw_zip(&[
        ("manifest.json", &manifest_bytes(&m)),
        ("characters/c_a.json", &ch),
        ("characters/c_b.json", &ch),
    ]);
    assert!(matches!(open(extra), Err(BundleError::Inconsistent(_))));
    let missing = raw_zip(&[("manifest.json", &manifest_bytes(&m))]);
    assert!(matches!(open(missing), Err(BundleError::Inconsistent(_))));
}

#[test]
fn unknown_format_versions_and_non_bundles_are_refused() {
    let (ch, mut m) = good_parts();
    m.format = "borealis-bundle/2".into();
    let z = raw_zip(&[
        ("manifest.json", &manifest_bytes(&m)),
        ("characters/c_a.json", &ch),
    ]);
    assert!(matches!(open(z), Err(BundleError::UnsupportedFormat(f)) if f == "borealis-bundle/2"));

    m.format = "something-else/1".into();
    let z = raw_zip(&[
        ("manifest.json", &manifest_bytes(&m)),
        ("characters/c_a.json", &ch),
    ]);
    assert!(matches!(open(z), Err(BundleError::NotABundle(_))));

    assert!(matches!(
        open(b"not a zip at all".to_vec()),
        Err(BundleError::NotABundle(_))
    ));
    assert!(matches!(
        open(raw_zip(&[("hello.txt", b"hi")])),
        Err(BundleError::NotABundle(_))
    ));
}

#[test]
fn the_reserved_campaigns_slot_is_refused_until_it_is_built() {
    let (ch, mut m) = good_parts();
    m.campaigns = vec![serde_json::json!({"id":"camp"})];
    let z = raw_zip(&[
        ("manifest.json", &manifest_bytes(&m)),
        ("characters/c_a.json", &ch),
    ]);
    assert!(matches!(open(z), Err(BundleError::UnsupportedFeature(_))));
}

#[test]
fn zip_bombs_hit_the_ratio_and_size_caps() {
    // 8 MiB of zeros deflates to a few KiB: far past the 200x ratio cap.
    let bomb = vec![0u8; 8 << 20];
    let mut item = character_item("c_a", &bomb, 1);
    item.files[0].size = bomb.len() as u64;
    let m = manifest_of(vec![item]);
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    use std::io::Write;
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    z.start_file("manifest.json", opts).unwrap();
    z.write_all(&manifest_bytes(&m)).unwrap();
    z.start_file("characters/c_a.json", opts).unwrap();
    z.write_all(&bomb).unwrap();
    let bytes = z.finish().unwrap().into_inner();
    assert!(bytes.len() < 200_000);
    assert!(matches!(
        open(bytes.clone()),
        Err(BundleError::LimitExceeded(_))
    ));

    // With the ratio cap lifted, the per-file cap still refuses it.
    let lim = Limits {
        max_ratio: u64::MAX,
        max_file_bytes: 1 << 20,
        ..Limits::default()
    };
    assert!(matches!(
        Bundle::open_with(Cursor::new(bytes.clone()), lim),
        Err(BundleError::LimitExceeded(_))
    ));
    // And the total cap.
    let lim = Limits {
        max_ratio: u64::MAX,
        max_total_bytes: 1 << 20,
        ..Limits::default()
    };
    assert!(matches!(
        Bundle::open_with(Cursor::new(bytes), lim),
        Err(BundleError::LimitExceeded(_))
    ));
}

#[test]
fn a_lying_manifest_size_cannot_make_the_reader_over_read() {
    let (ch, mut m) = good_parts();
    m.items[0].files[0].size = 3;
    m.items[0].size = 3;
    let z = raw_zip(&[
        ("manifest.json", &manifest_bytes(&m)),
        ("characters/c_a.json", &ch),
    ]);
    let mut b = open(z).unwrap();
    assert!(b.read_item(ItemKind::Character, "c_a").is_err());
}

#[test]
fn too_many_entries_are_refused() {
    let (ch, m) = good_parts();
    let z = raw_zip(&[
        ("manifest.json", &manifest_bytes(&m)),
        ("characters/c_a.json", &ch),
    ]);
    let lim = Limits {
        max_entries: 1,
        ..Limits::default()
    };
    assert!(matches!(
        Bundle::open_with(Cursor::new(z), lim),
        Err(BundleError::LimitExceeded(_))
    ));
}

#[test]
fn writer_refuses_what_the_reader_would() {
    let sink = || BundleWriter::new(Cursor::new(Vec::new()), BundleMeta { ..meta() });
    let mut w = sink();
    assert!(matches!(
        w.add_pack("p", &[("search/i.bin".into(), vec![1])], 1),
        Err(BundleError::Excluded(_))
    ));
    assert!(matches!(
        w.add_pack("q", &[("../x.json".into(), vec![1])], 1),
        Err(BundleError::UnsafePath(_))
    ));
    assert!(matches!(
        w.add_character("../evil", b"{}", 1, vec![]),
        Err(BundleError::UnsafePath(_))
    ));
    assert!(w.add_character("c_ok", b"[1]", 1, vec![]).is_err());
    assert!(w.add_character("c_ok", b"not json", 1, vec![]).is_err());
    assert!(matches!(
        w.set_portable_settings(&serde_json::json!({"ai": {"token": "t"}})),
        Err(BundleError::Excluded(_))
    ));
    assert!(matches!(
        w.set_portable_settings(&serde_json::json!({"documentsDir": "/x"})),
        Err(BundleError::Excluded(_))
    ));
}

#[test]
fn writer_caps_apply() {
    let lim = Limits {
        max_file_bytes: 10,
        ..Limits::default()
    };
    let mut w = BundleWriter::with_limits(Cursor::new(Vec::new()), meta(), lim);
    assert!(matches!(
        w.add_character("c_big", br#"{"a":"0123456789"}"#, 1, vec![]),
        Err(BundleError::LimitExceeded(_))
    ));
}
