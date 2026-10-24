//! Device-link state (paired-device registry, B1) never travels in a bundle: every path
//! borealis-link declares is excluded, and a registry saved inside a pack is skipped.

mod common;

use borealis_bundle::{
    build_bundle, is_excluded, BuildRequest, CharacterSelection, Limits, PackIndex, PackSelection,
};
use borealis_link::registry::registry_path;
use borealis_link::{CertFingerprint, DeviceRegistry, PairedDevice, Platform, DATA_PATHS};
use common::{meta, pack_json, write};
use std::io::Cursor;

#[test]
fn every_link_data_path_is_excluded_at_any_depth() {
    for p in DATA_PATHS {
        let file = if p.ends_with('/') {
            format!("{p}x.json")
        } else {
            (*p).to_string()
        };
        for candidate in [
            p.to_string(),
            file.clone(),
            format!("packs/p/{p}"),
            format!("packs/p/{file}"),
            p.to_uppercase(),
        ] {
            assert!(is_excluded(&candidate), "not excluded: {candidate}");
        }
    }
}

#[test]
fn a_registry_inside_a_pack_is_left_out_of_the_bundle() {
    let tmp = tempfile::tempdir().unwrap();
    let chars = tmp.path().join("characters");
    std::fs::create_dir_all(&chars).unwrap();
    let lib = tmp.path().join("library");
    let pack = lib.join("pack-a");
    write(&pack.join("pack.json"), b"{\"id\":\"pack-a\"}");
    write(&pack.join("content.json"), &pack_json("ID_TEST_ELEM_A"));
    let device = PairedDevice::new(
        "Pixel",
        Platform::Android,
        CertFingerprint::of_der(b"peer"),
        1,
    );
    DeviceRegistry::default()
        .with_paired(device)
        .unwrap()
        .save(&pack)
        .unwrap();
    assert!(registry_path(&pack).is_file());

    let req = BuildRequest {
        characters_dir: &chars,
        library_dir: &lib,
        settings_file: None,
        meta: meta(),
        characters: CharacterSelection::All,
        packs: PackSelection::All,
        bundled_index: PackIndex::new(),
        limits: Limits::default(),
    };
    let (cur, _, _) = build_bundle(Cursor::new(Vec::new()), &req).unwrap();
    let mut z = zip::ZipArchive::new(Cursor::new(cur.into_inner())).unwrap();
    let names: Vec<String> = (0..z.len())
        .map(|i| z.by_index(i).unwrap().name().to_owned())
        .collect();
    assert!(
        names.iter().any(|n| n.ends_with("pack-a/content.json")),
        "{names:?}"
    );
    assert!(
        names
            .iter()
            .all(|n| !n.contains("link/") && !n.contains("devices.json")),
        "{names:?}"
    );
}
