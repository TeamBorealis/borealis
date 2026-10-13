//! Output validation: JSON schema and loading into a `ContentStore` together with the core
//! pack. Dangling references are only allowed for ids the report documents as external.

mod support;

use borealis_content::schema::{validate_content, validate_document, DocumentKind};
use borealis_content::{IssueCode, Severity};
use borealis_import_5etools::core_ids;
use serde_json::Value;
use support::*;

#[test]
fn every_written_file_validates_against_the_schema() {
    let result = import_fixtures();
    let dir = tempfile::tempdir().unwrap();
    let dirs = write_packs(&result, dir.path());
    assert_eq!(dirs.len(), result.packs.len());
    let mut files = 0;
    for pack_dir in &dirs {
        let manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(pack_dir.join("pack.json")).unwrap())
                .unwrap();
        let violations = validate_document(DocumentKind::Pack, &manifest).unwrap();
        assert!(violations.is_empty(), "pack.json: {violations:?}");
        for entry in manifest["files"].as_array().unwrap() {
            let path = pack_dir.join(entry["path"].as_str().unwrap());
            let doc: Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let violations = validate_content(&doc).unwrap();
            assert!(violations.is_empty(), "{}: {violations:?}", path.display());
            files += 1;
        }
    }
    assert!(files >= 10, "expected content files, got {files}");
}

#[test]
fn packs_load_with_the_core_pack_and_have_no_undocumented_problems() {
    let result = import_fixtures();
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(&write_packs(&result, dir.path()));

    let mut issues = store.diagnostics().to_vec();
    issues.extend(store.validate());
    let external = &result.report.external_references;
    let unexpected: Vec<_> = issues
        .iter()
        .filter(|i| i.severity >= Severity::Warning)
        .filter(|i| {
            let documented = i.code == IssueCode::DanglingReference
                && external.iter().any(|id| i.message.contains(id.as_str()));
            !documented
        })
        .collect();
    assert!(unexpected.is_empty(), "{}", show(&unexpected));

    // every element the importer reports is in the store, and nothing is silently lost
    let produced: usize = result.packs.iter().map(|p| p.element_count()).sum();
    let imported = store
        .iter()
        .filter(|e| e.element.id.starts_with("ID_5ET_"))
        .count();
    assert_eq!(imported, produced);
}

#[test]
fn fixture_import_has_no_external_references() {
    // The fixtures contain every entity they link to, so nothing may dangle.
    let result = import_fixtures();
    assert!(
        result.report.external_references.is_empty(),
        "{:?}",
        result.report.external_references
    );
}

#[test]
fn every_mapped_core_id_exists_in_the_core_pack() {
    let store = store_with(&[]);
    let missing: Vec<String> = core_ids::all_mapped_ids()
        .into_iter()
        .filter(|id| !store.contains_id(id))
        .collect();
    assert!(
        missing.is_empty(),
        "core ids missing from the core pack: {missing:?}"
    );
}
