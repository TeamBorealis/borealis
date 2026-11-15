//! Item Isq-Y (phones with a large imported library): a store built to replace another shares every unchanged element
//! with it and is otherwise the store a plain build gives, a pack inserted at its load position gives the builder a
//! full load gives, and a builder's copy shares its files.

use borealis_content::{
    Append, ContentFile, ContentStore, ContentStoreBuilder, Element, PackFile, PackFileKind,
    PackFormat, PackManifest, PackOrigin, ParsedPack, Ruleset,
};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

fn manifest(id: &str, rulesets: Vec<Ruleset>) -> PackManifest {
    PackManifest {
        format: PackFormat,
        id: id.into(),
        name: id.to_uppercase(),
        version: "1.0.0".into(),
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

fn pack(id: &str, names: &[&str]) -> ParsedPack {
    let elements = names
        .iter()
        .map(|n| Element::new(format!("ID_T_LANGUAGE_{n}"), *n, "Language", "Test"))
        .collect();
    ParsedPack::from_parts(
        manifest(id, vec![Ruleset::Dnd2024]),
        vec![("content.json".into(), ContentFile::new(elements))],
    )
}

fn bundled() -> ContentStoreBuilder {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .unwrap()
        .add_pack_dir(&root.join("srd-5.2.1"))
        .unwrap()
}

/// Same elements, in the same order, with the same load facts.
fn same_store(a: &ContentStore, b: &ContentStore) {
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b.iter()) {
        assert_eq!(x, y);
    }
    assert_eq!(a.files(), b.files());
    assert_eq!(a.packs(), b.packs());
    assert_eq!(a.indexes(), b.indexes());
    assert_eq!(a.diagnostics(), b.diagnostics());
}

#[test]
fn a_store_built_to_replace_another_equals_a_plain_build_and_shares_what_did_not_change() {
    let before = bundled().add_parsed_pack(pack("hb", &["A", "B"]));
    let old = before.clone().build();
    let next = before
        .replace_pack(pack("hb", &["A", "C"]))
        .unwrap()
        .with_source_editions(BTreeMap::new());
    let plain = next.clone().build();
    let reused = next.build_reusing(Some(&old));
    same_store(&plain, &reused);
    let shared = reused
        .iter()
        .filter(|s| {
            old.get_exact(&s.element.id)
                .is_some_and(|o| Arc::ptr_eq(&o.element, &s.element))
        })
        .count();
    // C is new; everything else is the very element of the store it replaces
    assert_eq!(shared, reused.len() - 1);
    assert!(!Arc::ptr_eq(
        &old.get_exact("ID_T_LANGUAGE_B").unwrap().element,
        &reused.get_exact("ID_T_LANGUAGE_C").unwrap().element
    ));
}

#[test]
fn an_element_changed_by_an_append_or_an_edition_is_shared_only_when_it_comes_out_the_same() {
    let mut appended = ContentFile::new(vec![]);
    appended.appends.push(Append {
        id: "ID_T_LANGUAGE_A".into(),
        supports: vec!["Extra".into()],
        ..Append::default()
    });
    let appender = ParsedPack::from_parts(
        manifest("ap", vec![Ruleset::Dnd2024]),
        vec![("content.json".into(), appended)],
    );
    let builder = bundled()
        .add_parsed_pack(pack("hb", &["A"]))
        .add_parsed_pack(appender);
    let old = builder.clone().build();
    let again = builder.clone().build_reusing(Some(&old));
    same_store(&builder.clone().build(), &again);
    let a = |s: &ContentStore| Arc::clone(&s.get_exact("ID_T_LANGUAGE_A").unwrap().element);
    assert!(
        Arc::ptr_eq(&a(&old), &a(&again)),
        "the appended element, unchanged"
    );
    // the user's edition for the book re-tags its elements: not the old element any more
    let editions = BTreeMap::from([("test".to_string(), Ruleset::Dnd2014)]);
    let retagged = builder.with_source_editions(editions);
    let plain = retagged.clone().build();
    let reused = retagged.build_reusing(Some(&old));
    same_store(&plain, &reused);
    assert_eq!(a(&reused).ruleset, Some(Ruleset::Dnd2014));
    assert!(!Arc::ptr_eq(&a(&old), &a(&reused)));
}

#[test]
fn a_pack_inserted_at_its_load_position_gives_the_builder_a_full_load_gives() {
    let full = bundled()
        .add_parsed_pack(pack("a", &["A"]))
        .add_parsed_pack(pack("b", &["B"]))
        .add_parsed_pack(pack("c", &["C"]));
    let without_b = bundled()
        .add_parsed_pack(pack("a", &["A"]))
        .add_parsed_pack(pack("c", &["C"]));
    let at = without_b.pack_ids().position(|id| id == "c").unwrap();
    let inserted = without_b.insert_parsed_pack(at, pack("b", &["B"]));
    assert!(inserted.same_content(&full));
    same_store(&inserted.build(), &full.build());
    // at the end, as adding it last
    let last = bundled().insert_parsed_pack(usize::MAX, pack("z", &["Z"]));
    assert!(last.same_content(&bundled().add_parsed_pack(pack("z", &["Z"]))));
}

#[test]
fn a_reload_shares_the_files_that_did_not_change() {
    let old = bundled().add_parsed_pack(pack("hb", &["A"]));
    let again = ContentStoreBuilder::new()
        .add_parsed_pack_sharing(
            ParsedPack::read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/core"))
                .unwrap(),
            Some(&old),
        )
        .add_parsed_pack_sharing(pack("hb", &["A"]), Some(&old));
    let plain = ContentStoreBuilder::new()
        .add_pack_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/core"))
        .unwrap()
        .add_parsed_pack(pack("hb", &["A"]));
    assert!(again.same_content(&plain));
    let shared = again.clone().sharing_files_with(&old);
    assert!(shared.same_content(&plain));
}
