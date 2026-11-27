//! HC20 overrides (homebrew-creator spec 22.3): a file's `overrides` replace loaded elements in place, after all other
//! content regardless of load order, over core and custom files alike.

use borealis_content::element::Append;
use borealis_content::{
    ContentFile, ContentStoreBuilder, Element, IssueCode, PackOrigin, Ruleset, Severity,
};
use std::collections::BTreeMap;

fn el(id: &str, ty: &str, source: &str, text: &str) -> Element {
    let mut e = Element::new(id, id, ty, source);
    e.description = text.into();
    e
}

fn with_overrides(elements: Vec<Element>, overrides: Vec<Element>) -> ContentFile {
    let mut f = ContentFile::new(elements);
    f.overrides = overrides;
    f
}

fn srd() -> ContentFile {
    ContentFile::new(vec![
        el("ID_SRD_SPELL_A", "Spell", "SRD", "original a"),
        el("ID_SRD_SPELL_B", "Spell", "SRD", "original b"),
        el("ID_SRD_SPELL_C", "Spell", "SRD", "original c"),
    ])
}

#[test]
fn an_override_of_a_core_element_wins_in_place_without_a_duplicate_error() {
    let store = ContentStoreBuilder::new()
        .add_file("srd-5.1/spells.json", PackOrigin::Core, srd())
        .add_file(
            "homebrew-hb/content.json",
            PackOrigin::Custom,
            with_overrides(
                vec![],
                vec![el("ID_SRD_SPELL_B", "Spell", "SRD", "changed b")],
            ),
        )
        .build();
    let ids: Vec<_> = store.iter().map(|s| s.element.id.as_str()).collect();
    assert_eq!(
        ids,
        ["ID_SRD_SPELL_A", "ID_SRD_SPELL_B", "ID_SRD_SPELL_C"],
        "keeps the original's position"
    );
    let b = store.get("ID_SRD_SPELL_B").unwrap();
    assert_eq!(b.element.description, "changed b");
    assert_eq!(&*b.file, "homebrew-hb/content.json");
    let original = b.overridden.as_ref().expect("records the original");
    assert_eq!(original.element.description, "original b");
    assert_eq!(&*original.file, "srd-5.1/spells.json");
    assert!(store.get("ID_SRD_SPELL_A").unwrap().overridden.is_none());
    assert!(
        store
            .diagnostics()
            .iter()
            .all(|d| d.severity != Severity::Error),
        "{:?}",
        store.diagnostics()
    );
}

#[test]
fn overrides_load_after_every_other_file_whatever_the_folder_order() {
    // the override's pack sorts before the book it changes, and a later custom file redefines nothing of it
    let book = ContentFile::new(vec![el("ID_BOOK_FEAT", "Feat", "Book", "book text")]);
    let store = ContentStoreBuilder::new()
        .add_file(
            "a-homebrew/content.json",
            PackOrigin::Custom,
            with_overrides(vec![], vec![el("ID_BOOK_FEAT", "Feat", "Book", "my text")]),
        )
        .add_file("z-book/feats.json", PackOrigin::Custom, book)
        .build();
    let feat = store.get("ID_BOOK_FEAT").unwrap();
    assert_eq!(feat.element.description, "my text");
    assert_eq!(
        feat.overridden.as_ref().unwrap().element.description,
        "book text"
    );
    assert_eq!(
        &*feat.overridden.as_ref().unwrap().file,
        "z-book/feats.json"
    );
}

#[test]
fn of_two_overrides_of_one_id_the_later_wins_with_an_info() {
    let store = ContentStoreBuilder::new()
        .add_file("srd-5.1/spells.json", PackOrigin::Core, srd())
        .add_file(
            "homebrew-a/content.json",
            PackOrigin::Custom,
            with_overrides(vec![], vec![el("ID_SRD_SPELL_A", "Spell", "SRD", "first")]),
        )
        .add_file(
            "homebrew-b/content.json",
            PackOrigin::Custom,
            with_overrides(vec![], vec![el("ID_SRD_SPELL_A", "Spell", "SRD", "second")]),
        )
        .build();
    let a = store.get("ID_SRD_SPELL_A").unwrap();
    assert_eq!(a.element.description, "second");
    assert_eq!(
        a.overridden.as_ref().unwrap().element.description,
        "original a",
        "the original is the loaded element, not the other override"
    );
    let replaced: Vec<_> = store
        .diagnostics()
        .iter()
        .filter(|d| d.code == IssueCode::ElementReplaced)
        .collect();
    assert_eq!(replaced.len(), 1, "{:?}", store.diagnostics());
    assert_eq!(replaced[0].severity, Severity::Info);
    assert_eq!(replaced[0].file.as_deref(), Some("homebrew-b/content.json"));
}

#[test]
fn an_override_without_its_original_is_not_loaded() {
    let store = ContentStoreBuilder::new()
        .add_file(
            "homebrew-hb/content.json",
            PackOrigin::Custom,
            with_overrides(
                vec![el("ID_HB_OWN", "Feat", "Hb", "own")],
                vec![el("ID_GONE", "Feat", "Book", "changed")],
            ),
        )
        .build();
    assert!(store.get("ID_GONE").is_none());
    assert!(store.get("ID_HB_OWN").is_some());
    let missing: Vec<_> = store
        .diagnostics()
        .iter()
        .filter(|d| d.code == IssueCode::OverrideTargetMissing)
        .collect();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].severity, Severity::Warning);
    assert_eq!(missing[0].element.as_deref(), Some("ID_GONE"));
}

#[test]
fn appends_still_apply_to_an_overridden_element() {
    let mut list = ContentFile::new(vec![]);
    list.appends = vec![Append {
        id: "ID_SRD_SPELL_C".into(),
        supports: vec!["Homebrew List".into()],
        ..Append::default()
    }];
    let store = ContentStoreBuilder::new()
        .add_file("srd-5.1/spells.json", PackOrigin::Core, srd())
        .add_file("homebrew-a/content.json", PackOrigin::Custom, list)
        .add_file(
            "homebrew-b/content.json",
            PackOrigin::Custom,
            with_overrides(
                vec![],
                vec![el("ID_SRD_SPELL_C", "Spell", "SRD", "changed c")],
            ),
        )
        .build();
    let c = store.get("ID_SRD_SPELL_C").unwrap();
    assert_eq!(c.element.description, "changed c");
    assert!(c.element.supports.iter().any(|t| t == "Homebrew List"));
    assert!(store
        .with_tag("Homebrew List")
        .any(|s| s.element.id == "ID_SRD_SPELL_C"));
}

#[test]
fn an_override_of_a_book_with_a_user_edition_takes_that_edition() {
    let mut book = ContentFile::new(vec![
        el("ID_BOOK_SOURCE", "Source", "Book", ""),
        el("ID_BOOK_FEAT", "Feat", "Book", "book text"),
    ]);
    book.elements[0].name = "Book".into();
    book.ruleset = Some(Ruleset::Dnd2014);
    let mut changed = el("ID_BOOK_FEAT", "Feat", "Book", "my text");
    changed.ruleset = Some(Ruleset::Dnd2014);
    let store = ContentStoreBuilder::new()
        .add_file("book/content.json", PackOrigin::Custom, book)
        .add_file(
            "homebrew-hb/content.json",
            PackOrigin::Custom,
            with_overrides(vec![], vec![changed]),
        )
        .with_source_editions(BTreeMap::from([("book".to_string(), Ruleset::Dnd2024)]))
        .build();
    let feat = store.get("ID_BOOK_FEAT").unwrap();
    assert_eq!(feat.element.description, "my text");
    assert_eq!(feat.element.ruleset, Some(Ruleset::Dnd2024));
    assert_eq!(feat.user_edition_book.as_deref(), Some("book"));
}

#[test]
fn overrides_round_trip_through_json_and_stay_out_when_empty() {
    let file = with_overrides(vec![], vec![el("ID_X", "Feat", "Book", "t")]);
    let text = file.to_json_pretty().unwrap();
    assert!(text.contains("\"overrides\""));
    assert_eq!(ContentFile::from_json_str(&text).unwrap(), file);
    let plain = ContentFile::new(vec![el("ID_Y", "Feat", "Hb", "t")])
        .to_json_pretty()
        .unwrap();
    assert!(!plain.contains("overrides"));
}

#[test]
fn an_ignored_file_s_overrides_are_skipped() {
    let mut f = with_overrides(
        vec![],
        vec![el("ID_SRD_SPELL_A", "Spell", "SRD", "changed")],
    );
    f.ignore = Some(true);
    let store = ContentStoreBuilder::new()
        .add_file("srd-5.1/spells.json", PackOrigin::Core, srd())
        .add_file("homebrew-hb/content.json", PackOrigin::Custom, f)
        .build();
    assert_eq!(
        store.element("ID_SRD_SPELL_A").unwrap().description,
        "original a"
    );
}

/// A homebrew pack folder as the creator writes it: `pack.json`, `content.json` with the pack's own elements, and the
/// overrides in their own file.
fn pack_folder(root: &std::path::Path) -> std::path::PathBuf {
    use borealis_content::{OverridesFile, OverridesFormat, OVERRIDES_FILE};
    let dir = root.join("homebrew-hb");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("pack.json"),
        r#"{"format": "borealis-pack/1", "id": "homebrew-hb", "name": "Hb", "version": "1.0.1", "origin": "custom",
            "rulesets": ["dnd2024"], "files": [{"path": "content.json", "kind": "content"}]}"#,
    )
    .unwrap();
    let own = ContentFile::new(vec![el("ID_HB_FEAT_OWN", "Feat", "Hb", "mine")]);
    std::fs::write(dir.join("content.json"), own.to_json_pretty().unwrap()).unwrap();
    let overrides = OverridesFile {
        format: OverridesFormat,
        ruleset: None,
        overrides: vec![el("ID_SRD_SPELL_A", "Spell", "SRD", "changed a")],
        records: serde_json::Map::new(),
    };
    std::fs::write(
        dir.join(OVERRIDES_FILE),
        overrides.to_json_pretty().unwrap(),
    )
    .unwrap();
    dir
}

#[test]
fn a_pack_with_overrides_still_loads_the_1_5_0_way_with_the_originals() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = pack_folder(tmp.path());
    // the content file holds no key 1.5.0 doesn't know
    let content = std::fs::read_to_string(dir.join("content.json")).unwrap();
    assert!(!content.contains("overrides"), "{content}");
    // 1.5.0's folder scan takes `*.json` only: the overrides file is not picked up, the pack's own elements load
    let scanned = ContentStoreBuilder::new()
        .add_file("srd-5.1/spells.json", PackOrigin::Core, srd())
        .add_dir(tmp.path(), PackOrigin::Custom)
        .unwrap()
        .build();
    assert_eq!(
        scanned.files(),
        ["srd-5.1/spells.json", "homebrew-hb/content.json"]
    );
    assert!(scanned.get("ID_HB_FEAT_OWN").is_some());
    assert_eq!(
        scanned.element("ID_SRD_SPELL_A").unwrap().description,
        "original a"
    );
    assert!(scanned
        .diagnostics()
        .iter()
        .all(|d| d.severity != Severity::Error));
    // 1.5.0's pack read takes the files `pack.json` lists: the same content file, nothing else
    let manifest: borealis_content::PackManifest = borealis_content::PackManifest::from_json_str(
        &std::fs::read_to_string(dir.join("pack.json")).unwrap(),
    )
    .unwrap();
    assert!(manifest.files.iter().all(|f| f.path == "content.json"));
}

#[test]
fn this_version_reads_the_overrides_file_of_a_pack_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = pack_folder(tmp.path());
    let store = ContentStoreBuilder::new()
        .add_file("srd-5.1/spells.json", PackOrigin::Core, srd())
        .add_pack_dir(&dir)
        .unwrap()
        .build();
    assert_eq!(
        store.element("ID_SRD_SPELL_A").unwrap().description,
        "changed a"
    );
    assert!(store
        .get("ID_SRD_SPELL_A")
        .unwrap()
        .file
        .starts_with("homebrew-hb/"));
    assert!(store.get("ID_HB_FEAT_OWN").is_some());
    // a broken overrides file is an error of the pack, as a broken content file is
    std::fs::write(dir.join(borealis_content::OVERRIDES_FILE), "{").unwrap();
    assert!(ContentStoreBuilder::new().add_pack_dir(&dir).is_err());
}
