//! WB-06 (homebrew creator, spec 14.1): a pack built in memory loads like the same pack read from disk, and replacing
//! one pack of a builder in place gives the builder a full load would give.

use borealis_content::validate::validate_elements;
use borealis_content::{
    ContentFile, ContentStoreBuilder, Element, PackFile, PackFileKind, PackFormat, PackManifest,
    PackOrigin, ParsedPack, Ruleset, Severity,
};
use std::collections::HashSet;
use std::path::Path;

fn manifest(id: &str, version: &str, rulesets: Vec<Ruleset>) -> PackManifest {
    PackManifest {
        format: PackFormat,
        id: id.into(),
        name: id.to_uppercase(),
        version: version.into(),
        origin: PackOrigin::Custom,
        rulesets,
        license: None,
        attribution: None,
        description: None,
        files: vec![PackFile {
            path: "content.json".into(),
            kind: PackFileKind::Content,
        }],
    }
}

fn content(names: &[&str]) -> ContentFile {
    ContentFile::new(
        names
            .iter()
            .map(|n| Element::new(format!("ID_T_LANGUAGE_{n}"), *n, "Language", "Test"))
            .collect(),
    )
}

fn write_pack(dir: &Path, m: &PackManifest, c: &ContentFile) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("pack.json"), m.to_json_pretty().unwrap()).unwrap();
    std::fs::write(dir.join("content.json"), c.to_json_pretty().unwrap()).unwrap();
}

#[test]
fn a_pack_from_parts_equals_the_pack_read_from_its_folder() {
    let dir = tempfile::tempdir().unwrap();
    let m = manifest("hb", "1.0.0", vec![Ruleset::Dnd2024]);
    let c = content(&["A", "B"]);
    write_pack(dir.path(), &m, &c);
    let read = ContentStoreBuilder::new().add_parsed_pack(ParsedPack::read(dir.path()).unwrap());
    let parts = ContentStoreBuilder::new()
        .add_parsed_pack(ParsedPack::from_parts(m, vec![("content.json".into(), c)]));
    assert!(read.same_content(&parts));
    // the pack's only ruleset is the file's default, as when read from disk
    let store = parts.build();
    assert_eq!(
        store.element("ID_T_LANGUAGE_A").unwrap().ruleset,
        Some(Ruleset::Dnd2024)
    );
}

#[test]
fn replacing_a_pack_keeps_its_load_position_and_equals_a_full_load() {
    let root = tempfile::tempdir().unwrap();
    let (a, b, c) = (
        root.path().join("a"),
        root.path().join("b"),
        root.path().join("c"),
    );
    write_pack(
        &a,
        &manifest("a", "1.0.0", vec![Ruleset::Dnd2014]),
        &content(&["X"]),
    );
    write_pack(
        &b,
        &manifest("b", "1.0.0", vec![Ruleset::Dnd2024]),
        &content(&["Y"]),
    );
    write_pack(
        &c,
        &manifest("c", "1.0.0", vec![Ruleset::Dnd2024]),
        &content(&["Z"]),
    );
    let load = || {
        [&a, &b, &c]
            .iter()
            .fold(ContentStoreBuilder::new(), |bl, d| {
                bl.add_pack_dir(d).unwrap()
            })
    };
    let before = load();
    // pack b gets a new version and one more element; written to disk, a full load sees the same
    let m2 = manifest("b", "1.0.1", vec![Ruleset::Dnd2024]);
    let c2 = content(&["Y", "W"]);
    write_pack(&b, &m2, &c2);
    let replaced = before
        .clone()
        .replace_pack(ParsedPack::from_parts(
            m2,
            vec![("content.json".into(), c2)],
        ))
        .expect("pack b is in the builder");
    assert!(replaced.same_content(&load()));
    let store = replaced.build();
    assert!(store.contains_id("ID_T_LANGUAGE_W"));
    assert_eq!(
        store
            .packs()
            .iter()
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
    assert_eq!(store.packs()[1].version, "1.0.1");
    // a pack the builder does not hold is not added
    assert!(before
        .replace_pack(ParsedPack::from_parts(
            manifest("zz", "1.0.0", vec![]),
            vec![]
        ))
        .is_none());
}

#[test]
fn an_emptied_pack_can_be_filled_again_in_its_place() {
    let m = |v: &str| manifest("b", v, vec![Ruleset::Dnd2024]);
    let base = ContentStoreBuilder::new()
        .add_parsed_pack(ParsedPack::from_parts(
            manifest("a", "1", vec![]),
            vec![("content.json".into(), content(&["X"]))],
        ))
        .add_parsed_pack(ParsedPack::from_parts(m("1"), vec![]))
        .add_parsed_pack(ParsedPack::from_parts(
            manifest("c", "1", vec![]),
            vec![("content.json".into(), content(&["Z"]))],
        ));
    assert!(base.has_pack("b"));
    let filled = base
        .replace_pack(ParsedPack::from_parts(
            m("2"),
            vec![("content.json".into(), content(&["Y"]))],
        ))
        .unwrap();
    let want = ContentStoreBuilder::new()
        .add_parsed_pack(ParsedPack::from_parts(
            manifest("a", "1", vec![]),
            vec![("content.json".into(), content(&["X"]))],
        ))
        .add_parsed_pack(ParsedPack::from_parts(
            m("2"),
            vec![("content.json".into(), content(&["Y"]))],
        ))
        .add_parsed_pack(ParsedPack::from_parts(
            manifest("c", "1", vec![]),
            vec![("content.json".into(), content(&["Z"]))],
        ));
    assert!(filled.same_content(&want));
}

#[test]
fn validating_some_elements_reports_only_theirs() {
    let mut bad = Element::new("ID_T_FEAT_BAD", "Bad", "Feat", "Test");
    bad.requirements = "ID_NOWHERE".into();
    let mut other = Element::new("ID_T_FEAT_OTHER", "Other", "Feat", "Test");
    other.requirements = "ID_ALSO_NOWHERE".into();
    let store = ContentStoreBuilder::new()
        .add_file(
            "t.json",
            PackOrigin::Custom,
            ContentFile::new(vec![bad, other]),
        )
        .build();
    let only: HashSet<&str> = ["ID_T_FEAT_BAD"].into_iter().collect();
    let issues = validate_elements(&store, &only);
    assert!(!issues.is_empty());
    assert!(issues
        .iter()
        .all(|i| i.element.as_deref() == Some("ID_T_FEAT_BAD")));
    let all = store.validate();
    let mine: Vec<_> = all
        .iter()
        .filter(|i| i.element.as_deref() == Some("ID_T_FEAT_BAD") && i.severity != Severity::Info)
        .collect();
    assert_eq!(
        mine.len(),
        issues
            .iter()
            .filter(|i| i.severity != Severity::Info)
            .count()
    );
}
