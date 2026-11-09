//! A single 5etools-format homebrew file (`_meta` + entity arrays) on its own, as the app stages
//! it: a data folder holding only `homebrew/<file>.json`. Invented content (B-238).

use borealis_import_5etools::{import_folder, scan_folder, ImportOptions};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/homebrew/quill-brew.json")
}

/// A data folder with nothing but the homebrew file in `homebrew/`.
fn staged() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let brew = dir.path().join("homebrew");
    std::fs::create_dir_all(&brew).unwrap();
    std::fs::copy(fixture(), brew.join("quill-brew.json")).unwrap();
    dir
}

#[test]
fn the_scan_names_the_source_from_meta_and_what_is_not_imported() {
    let dir = staged();
    let sources = scan_folder(dir.path()).unwrap();
    assert_eq!(sources.len(), 1, "{sources:?}");
    let s = &sources[0];
    assert_eq!(s.abbr, "QuillBrew");
    assert_eq!(s.name, "The Quill Brew");
    assert_eq!(s.category, "homebrew");
    assert_eq!(s.ruleset, "dnd2014");
    for kind in [
        "class",
        "subclass",
        "race",
        "background",
        "feat",
        "spell",
        "item",
        "monster",
    ] {
        assert_eq!(s.counts.get(kind), Some(&1), "{kind}: {:?}", s.counts);
    }
    let unsupported: BTreeMap<String, usize> =
        [("deity".to_string(), 1), ("vehicle".to_string(), 1)].into();
    assert_eq!(s.unsupported, unsupported);
    // `_meta.dependencies` names a source that is not among the chosen files
    assert_eq!(s.needs, vec!["InkwellExtras".to_string()]);
}

#[test]
fn a_dependency_in_the_folder_is_not_reported_missing() {
    let dir = staged();
    std::fs::write(
        dir.path().join("homebrew/extras.json"),
        r#"{"_meta": {"sources": [{"json": "InkwellExtras", "full": "Inkwell Extras"}]},
            "feat": [{"name": "Blotter", "source": "InkwellExtras", "entries": ["Dry."]}]}"#,
    )
    .unwrap();
    let sources = scan_folder(dir.path()).unwrap();
    let quill = sources.iter().find(|s| s.abbr == "QuillBrew").unwrap();
    assert!(quill.needs.is_empty(), "{:?}", quill.needs);
    assert_eq!(sources.len(), 2);
}

#[test]
fn every_supported_kind_imports_into_one_pack_named_after_the_file_source() {
    let dir = staged();
    let options = ImportOptions::default().with_sources(&["QuillBrew".to_string()]);
    let result = import_folder(dir.path(), &options).unwrap();
    assert_eq!(result.packs.len(), 1);
    let pack = &result.packs[0];
    assert_eq!(pack.manifest.id, "5etools-quillbrew");
    assert!(
        pack.manifest.name.contains("The Quill Brew"),
        "{}",
        pack.manifest.name
    );
    let types: Vec<&str> = pack.elements().map(|e| e.ty.as_str()).collect();
    for ty in [
        "Class",
        "Class Feature",
        "Archetype",
        "Archetype Feature",
        "Race",
        "Background",
        "Feat",
        "Spell",
        "Magic Item",
        "Monster",
        "Source",
    ] {
        assert!(types.contains(&ty), "{ty} missing from {types:?}");
    }
    // the subclass hangs off the class of the same file
    let subclass = pack.elements().find(|e| e.ty == "Archetype").unwrap();
    assert!(subclass
        .supports
        .iter()
        .any(|s| s == "ID_5ET_QUILLBREW_CLASS_QUILLMANCER"));
    assert!(
        result.report.external_references.is_empty(),
        "{:?}",
        result.report.external_references
    );
    // what the file holds that Borealis does not import is skipped with a reason (sorted by entry)
    let skipped: Vec<(&str, &str, &str)> = result
        .report
        .skipped
        .iter()
        .map(|n| (n.kind.as_str(), n.element.as_str(), n.reason.as_str()))
        .collect();
    assert_eq!(
        skipped,
        vec![
            (
                "vehicle",
                "Paper Skiff|QuillBrew",
                "not imported: Borealis has no vehicles"
            ),
            (
                "deity",
                "The First Scribe|QuillBrew",
                "not imported: Borealis has no deities"
            ),
        ]
    );
    assert!(
        result.report.partial.is_empty(),
        "{:?}",
        result.report.partial
    );
}

#[test]
fn unsupported_entries_of_an_unchosen_source_are_not_reported() {
    let dir = staged();
    let options = ImportOptions::default().with_sources(&["Other".to_string()]);
    let result = import_folder(dir.path(), &options).unwrap();
    assert!(result.packs.is_empty());
    assert!(result.report.skipped.is_empty());
}
