//! PF1 (B-235): reading a pack on its own, so a broken pack never touches the builder.

use borealis_content::{ContentStoreBuilder, ParsedPack};
use std::path::PathBuf;

const BUNDLED: [&str; 5] = [
    "core",
    "srd-5.1",
    "srd-5.2.1",
    "srd-5.1-monsters",
    "srd-5.2.1-monsters",
];

fn pack_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../content")
        .join(name)
}

fn bundled() -> ContentStoreBuilder {
    BUNDLED.iter().fold(ContentStoreBuilder::new(), |b, p| {
        b.add_pack_dir(&pack_dir(p)).unwrap()
    })
}

#[test]
fn a_parsed_pack_adds_exactly_what_add_pack_dir_adds() {
    let direct = bundled();
    let parsed = BUNDLED.iter().fold(ContentStoreBuilder::new(), |b, p| {
        let pack = ParsedPack::read(&pack_dir(p)).unwrap();
        assert_eq!(pack.info().id, *p);
        b.add_parsed_pack(pack)
    });
    assert!(parsed.same_content(&direct));
    let (a, b) = (parsed.build(), direct.build());
    assert!(a.len() > 3_000);
    assert!(a.iter().eq(b.iter()));
}

#[test]
fn a_broken_pack_fails_to_read_and_names_the_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("pack.json"),
        r#"{"format":"borealis-pack/1","id":"x","name":"X","version":"1","origin":"custom","rulesets":["dnd2014"],"files":[{"path":"a.json","kind":"content"}]}"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("a.json"), "{ nope").unwrap();
    let err = ParsedPack::read(dir.path()).unwrap_err().to_string();
    assert!(err.contains("a.json"), "{err}");
}
