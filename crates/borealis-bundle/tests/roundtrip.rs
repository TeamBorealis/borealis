mod common;

use borealis_bundle::{
    build_bundle, plan_receive, scan_characters, scan_packs, Action, BuildRequest, Bundle,
    CharacterSelection, ItemKind, Limits, PackIndex, PackRef, PackSelection,
};
use common::{character_json, meta, pack_json, write};
use std::collections::BTreeSet;
use std::io::Cursor;

struct Fixture {
    _tmp: tempfile::TempDir,
    docs: std::path::PathBuf,
    data: std::path::PathBuf,
    chars: std::path::PathBuf,
    lib: std::path::PathBuf,
    settings: std::path::PathBuf,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let docs = tmp.path().join("docs");
    let data = tmp.path().join("data");
    write(
        &docs.join("characters/c_one.json"),
        &character_json("One", "ID_TEST_ELEM_A"),
    );
    write(
        &docs.join("characters/c_two.json"),
        &character_json("Two", "ID_NOT_IN_ANY_PACK"),
    );
    // A user pack that defines ID_TEST_ELEM_A, plus files that must never travel.
    let pack = data.join("library/pack-a");
    write(&pack.join("pack.json"), b"{\"id\":\"pack-a\"}");
    write(&pack.join("content.json"), &pack_json("ID_TEST_ELEM_A"));
    write(&pack.join("search/index.bin"), b"derived");
    write(&pack.join("host.key"), b"secret");
    write(&pack.join("ai-host-key.der"), b"secret");
    // A second user pack nobody needs.
    let other = data.join("library/pack-b");
    write(&other.join("pack.json"), b"{\"id\":\"pack-b\"}");
    write(&other.join("content.json"), &pack_json("ID_TEST_ELEM_B"));
    // Data-dir decoys outside the library.
    write(&data.join("ai-host-key.der"), b"secret");
    write(&data.join("models/m.bin"), b"model");
    write(
        &data.join("settings.json"),
        br#"{"playerName":"Pat","accentColor":"teal","documentsDir":"/x","mobile":true,"ai":{"pairedHost":"h","token":"t"},"tileSize":3,"sheet":{"theme":"ink"}}"#,
    );
    Fixture {
        _tmp: tmp,
        chars: docs.join("characters"),
        lib: data.join("library"),
        settings: data.join("settings.json"),
        docs,
        data,
    }
}

fn request<'a>(f: &'a Fixture, packs: PackSelection) -> BuildRequest<'a> {
    BuildRequest {
        characters_dir: &f.chars,
        library_dir: &f.lib,
        settings_file: Some(&f.settings),
        meta: meta(),
        characters: CharacterSelection::All,
        packs,
        bundled_index: PackIndex::new(),
        limits: Limits::default(),
    }
}

fn build(f: &Fixture, packs: PackSelection) -> Vec<u8> {
    let (cur, _, _) = build_bundle(Cursor::new(Vec::new()), &request(f, packs)).unwrap();
    cur.into_inner()
}

#[test]
fn round_trip_restores_every_byte() {
    let f = fixture();
    let bytes = build(&f, PackSelection::Required);
    let mut b = Bundle::open(Cursor::new(bytes)).unwrap();
    b.verify_all().unwrap();
    let m = b.manifest().clone();
    assert_eq!(m.format, "borealis-bundle/1");
    assert_eq!(m.source_device, "Test PC");
    assert_eq!(m.app_version, "1.2.3");
    let keys: Vec<_> = m.items.iter().map(|i| (i.kind, i.key.as_str())).collect();
    assert_eq!(
        keys,
        vec![
            (ItemKind::Character, "c_one"),
            (ItemKind::Character, "c_two"),
            (ItemKind::Pack, "pack-a"),
        ]
    );
    let one = b.read_item(ItemKind::Character, "c_one").unwrap();
    assert_eq!(
        one.files[0].1,
        std::fs::read(f.docs.join("characters/c_one.json")).unwrap()
    );
    let pack = b.read_item(ItemKind::Pack, "pack-a").unwrap();
    let names: Vec<_> = pack.files.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        names,
        vec!["packs/pack-a/content.json", "packs/pack-a/pack.json"]
    );
}

#[test]
fn required_packs_are_listed_per_character() {
    let f = fixture();
    let b = Bundle::open(Cursor::new(build(&f, PackSelection::None))).unwrap();
    let find = |k: &str| {
        b.manifest()
            .items
            .iter()
            .find(|i| i.key == k)
            .unwrap()
            .clone()
    };
    assert_eq!(
        find("c_one").required_packs,
        vec![PackRef {
            id: "pack-a".into(),
            bundled: false
        }]
    );
    assert!(find("c_two").required_packs.is_empty());
    // PackSelection::None ships no pack; Required ships only pack-a; All ships both.
    assert!(b.manifest().items.iter().all(|i| i.kind != ItemKind::Pack));
    let req = Bundle::open(Cursor::new(build(&f, PackSelection::Required))).unwrap();
    assert_eq!(count_packs(&req), 1);
    let all = Bundle::open(Cursor::new(build(&f, PackSelection::All))).unwrap();
    assert_eq!(count_packs(&all), 2);
}

fn count_packs(b: &Bundle<Cursor<Vec<u8>>>) -> usize {
    b.manifest()
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::Pack)
        .count()
}

#[test]
fn bundled_packs_are_named_but_never_shipped() {
    let f = fixture();
    let mut req = request(&f, PackSelection::All);
    let mut idx = PackIndex::new();
    idx.add_pack("srd-5.1", true, ["ID_NOT_IN_ANY_PACK".to_string()]);
    req.bundled_index = idx;
    let (cur, _, _) = build_bundle(Cursor::new(Vec::new()), &req).unwrap();
    let b = Bundle::open(Cursor::new(cur.into_inner())).unwrap();
    let two = b
        .manifest()
        .items
        .iter()
        .find(|i| i.key == "c_two")
        .unwrap();
    assert_eq!(two.required_packs[0].id, "srd-5.1");
    assert!(two.required_packs[0].bundled);
    assert!(b.manifest().items.iter().all(|i| i.key != "srd-5.1"));
}

#[test]
fn exclusions_are_enforced_and_settings_are_portable_only() {
    let f = fixture();
    let bytes = build(&f, PackSelection::All);
    let mut z = zip::ZipArchive::new(Cursor::new(bytes.clone())).unwrap();
    let names: Vec<String> = (0..z.len())
        .map(|i| z.by_index(i).unwrap().name().to_owned())
        .collect();
    for n in &names {
        assert!(
            !borealis_bundle::is_excluded(n),
            "excluded path leaked into the bundle: {n}"
        );
        for banned in [
            "search",
            ".key",
            ".der",
            "models",
            "ai-host",
            "settings.json",
        ] {
            assert!(!n.contains(banned), "{n} contains {banned}");
        }
    }
    let mut b = Bundle::open(Cursor::new(bytes)).unwrap();
    let s = b.read_settings().unwrap().unwrap();
    let keys: BTreeSet<_> = s.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        keys,
        ["accentColor", "playerName", "sheet"]
            .map(String::from)
            .into_iter()
            .collect()
    );
}

#[test]
fn local_scan_digests_match_the_manifest_so_a_resend_is_a_no_op() {
    let f = fixture();
    let b = Bundle::open(Cursor::new(build(&f, PackSelection::Required))).unwrap();
    let mut local = scan_characters(&f.docs.join("characters")).unwrap();
    local.extend(scan_packs(&f.data.join("library"), &Limits::default()).unwrap());
    let plan = plan_receive(b.manifest(), &local, &BTreeSet::new());
    assert_eq!(plan.entries.len(), 3);
    assert!(plan.entries.iter().all(|e| e.action == Action::Identical));
}

#[test]
fn explicit_selection_of_a_missing_character_is_an_error() {
    let f = fixture();
    let mut req = request(&f, PackSelection::None);
    req.characters = CharacterSelection::Keys(vec!["c_nope".into()]);
    assert!(build_bundle(Cursor::new(Vec::new()), &req).is_err());
}
