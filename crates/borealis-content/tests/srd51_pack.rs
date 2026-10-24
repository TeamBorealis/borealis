//! Validation of the bundled `srd-5.1` content pack (authored from the CC-BY-4.0 SRD 5.1).
//!
//! Loads `content/core` first, then `content/srd-5.1`, checks every file against the JSON schema and
//! runs the store validator. The pack must load with zero errors and zero dangling references.
//! A readable report is written to `target/srd51_validation.txt`.

use borealis_content::schema::{validate_document, DocumentKind};
use borealis_content::{ContentStoreBuilder, IssueCode, PackManifest, Severity};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

#[test]
fn srd51_pack_is_schema_valid_and_resolves_every_reference() {
    let root = content_dir();
    let pack = root.join("srd-5.1");
    if !pack.join("pack.json").exists() {
        eprintln!("SKIPPED: content/srd-5.1 not present");
        return;
    }
    let mut report = String::new();

    // 1. schema validation of the manifest and every listed file
    let manifest_text = fs::read_to_string(pack.join("pack.json")).unwrap();
    let manifest_json: serde_json::Value = serde_json::from_str(&manifest_text).unwrap();
    let mut schema_problems = Vec::new();
    for v in validate_document(DocumentKind::Pack, &manifest_json).unwrap() {
        schema_problems.push(format!("pack.json {}: {}", v.pointer, v.message));
    }
    let manifest = PackManifest::from_json_str(&manifest_text).unwrap();
    for f in &manifest.files {
        let text =
            fs::read_to_string(pack.join(&f.path)).unwrap_or_else(|e| panic!("{}: {e}", f.path));
        let doc: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", f.path));
        let kind = match f.kind {
            borealis_content::PackFileKind::Content => DocumentKind::Content,
            borealis_content::PackFileKind::Index => DocumentKind::Index,
        };
        for v in validate_document(kind, &doc).unwrap() {
            schema_problems.push(format!("{} {}: {}", f.path, v.pointer, v.message));
        }
    }
    writeln!(report, "schema violations: {}", schema_problems.len()).unwrap();
    for p in schema_problems.iter().take(50) {
        writeln!(report, "  {p}").unwrap();
    }

    // 2. store + validator over core + srd-5.1
    let mut builder = ContentStoreBuilder::new();
    if root.join("core/pack.json").exists() {
        builder = builder.add_pack_dir(&root.join("core")).unwrap();
    }
    let store = builder.add_pack_dir(&pack).unwrap().build();
    let issues = store.validate();

    let mut by_code: BTreeMap<String, usize> = BTreeMap::new();
    for i in &issues {
        *by_code
            .entry(format!("{:?}/{:?}", i.severity, i.code))
            .or_default() += 1;
    }
    writeln!(report, "elements in store: {}", store.len()).unwrap();
    let mut per_type: BTreeMap<String, usize> = BTreeMap::new();
    for s in store.by_source("SRD 5.1") {
        *per_type.entry(s.element.ty.clone()).or_default() += 1;
    }
    for (t, n) in &per_type {
        writeln!(report, "  SRD 5.1 {t}: {n}").unwrap();
    }
    writeln!(report, "issues by code: {by_code:?}").unwrap();
    let mut bad = 0;
    for i in &issues {
        let interesting = i.severity >= Severity::Warning
            && !matches!(i.code, IssueCode::UndefinedSource)
            && i.file
                .as_deref()
                .is_none_or(|f| f.starts_with("srd-5.1") || f.contains("srd-5.1"));
        if interesting {
            bad += 1;
            if bad <= 200 {
                writeln!(
                    report,
                    "{:?} {:?} {:?} {:?} {}",
                    i.severity, i.code, i.file, i.element, i.message
                )
                .unwrap();
            }
        }
    }
    writeln!(report, "warnings+errors in srd-5.1: {bad}").unwrap();
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/srd51_validation.txt");
    let _ = fs::write(&out, &report);
    println!("{report}");

    assert!(
        schema_problems.is_empty(),
        "schema violations, see {}",
        out.display()
    );
    let errors = issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .count();
    assert_eq!(errors, 0, "validator errors, see {}", out.display());
    let dangling = issues
        .iter()
        .filter(|i| i.code == IssueCode::DanglingReference)
        .count();
    assert_eq!(dangling, 0, "dangling references, see {}", out.display());
}
