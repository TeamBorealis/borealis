//! Local-only sweep over a real, full 5etools `data/` folder. Enabled by
//! `BOREALIS_5ETOOLS_DIR`, which must point at the `data/` directory of a 5etools checkout.
//! The data is never part of the repository: this test prints counts only, never content text.
//!
//! Checks: every source imports without a folder-level error; every written file validates
//! against the schema; all packs load together with the bundled core + SRD packs, and the only
//! dangling references are the documented `external_references`; two runs are byte-identical;
//! single-source PHB / XPHB imports work. Thresholds were set from a full 2026-10 5etools
//! checkout with a little headroom: at least 97% of entries converted, at most 260 skipped and
//! 60 partial (none about additional spells or the gaps closed by 5etools-gaps-1), at least 85 packs and 12,000 elements (see the constants below).

mod support;

use borealis_content::schema::{validate_content, validate_document, DocumentKind};
use borealis_content::{ContentStoreBuilder, IssueCode, Severity};
use borealis_import_5etools::{import_folder, ImportOptions, ImportResult};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use support::write_packs;

type Counts = BTreeMap<String, usize>;

/// Lowest acceptable share of converted entries (converted / total), in percent.
const MIN_CONVERTED_PERCENT: usize = 97;
/// Highest acceptable number of skipped entries for the whole folder (243 on the 2026-10
/// corpus, nearly all catalog entries and languages the core pack already defines).
const MAX_SKIPPED: usize = 260;
/// Highest acceptable number of partial-conversion notes (412 on the 2026-10 corpus before
/// additional spells were mapped, 324 after, 104 after prerequisites, 49 after 5etools-gaps-1,
/// 152 after fix-w9d made item gaps that used to drop silently report a note: 111 of them items).
const MAX_PARTIAL: usize = 160;
/// Notes that must not appear any more (closed by 5etools-gaps-1): (starts with, contains).
const CLOSED_GAPS: &[(&str, &str)] = &[
    ("ability score maximum", ""),
    ("ability increase of", "separate +1 picks"),
    ("spell choice filter", "was not converted"),
    ("subrace replaces the parent race's", ""),
];
/// Highest acceptable number of unenforced prerequisites (220 on the 2026-10 corpus before
/// prerequisites were mapped, 5 after: 4 house renown tiers and 1 unknown race at most).
const MAX_UNENFORCED_PREREQS: usize = 8;
/// Lower bounds on what a full folder must produce (92 packs, 12443 elements in 2026-10).
const MIN_PACKS: usize = 85;
const MIN_ELEMENTS: usize = 12_000;

fn bump(map: &mut Counts, key: impl Into<String>) {
    *map.entry(key.into()).or_default() += 1;
}

fn table(title: &str, map: &Counts, limit: usize) {
    println!("--- {title}");
    let mut rows: Vec<_> = map.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (k, v) in rows.into_iter().take(limit) {
        println!("{v:>8}  {k}");
    }
}

fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

fn options() -> ImportOptions {
    ImportOptions {
        srd_dirs: vec![
            content_dir().join("srd-5.1"),
            content_dir().join("srd-5.2.1"),
        ],
        ..ImportOptions::default()
    }
}

fn tree(root: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
    let mut entries: Vec<_> = std::fs::read_dir(root).unwrap().flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for e in entries {
        let p = e.path();
        if p.is_dir() {
            tree(&p, out);
        } else {
            out.insert(p.to_string_lossy().to_string(), std::fs::read(&p).unwrap());
        }
    }
}

fn schema_check(dirs: &[PathBuf]) -> usize {
    let mut bad = 0;
    for pack_dir in dirs {
        let text = std::fs::read_to_string(pack_dir.join("pack.json")).unwrap();
        let manifest: Value = serde_json::from_str(&text).unwrap();
        bad += validate_document(DocumentKind::Pack, &manifest)
            .unwrap()
            .len();
        for entry in manifest["files"].as_array().unwrap() {
            let path = pack_dir.join(entry["path"].as_str().unwrap());
            let doc: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            bad += validate_content(&doc).unwrap().len();
        }
    }
    bad
}

/// Load core + SRD + the imported packs; return (issues by severity/code, undocumented ones).
fn store_check(result: &ImportResult, dirs: &[PathBuf]) -> (Counts, Vec<String>) {
    let mut builder = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        builder = builder
            .add_pack_dir(&content_dir().join(pack))
            .expect("bundled pack loads");
    }
    for d in dirs {
        builder = builder.add_pack_dir(d).expect("imported pack loads");
    }
    let store = builder.build();
    let mut issues = store.diagnostics().to_vec();
    issues.extend(store.validate());
    let external = &result.report.external_references;
    let mut counts = Counts::new();
    let mut undocumented = Vec::new();
    for i in issues.iter().filter(|i| i.severity >= Severity::Warning) {
        bump(&mut counts, format!("{:?}/{:?}", i.severity, i.code));
        let documented = i.code == IssueCode::DanglingReference
            && external.iter().any(|id| i.message.contains(id.as_str()));
        if !documented {
            undocumented.push(format!(
                "{:?}/{:?} [{}]",
                i.severity,
                i.code,
                i.element.as_deref().unwrap_or("-")
            ));
        }
    }
    let produced: usize = result.packs.iter().map(|p| p.element_count()).sum();
    let loaded = store
        .iter()
        .filter(|e| e.element.id.starts_with("ID_5ET_"))
        .count();
    assert_eq!(loaded, produced, "every produced element is in the store");
    (counts, undocumented)
}

fn summarize(label: &str, result: &ImportResult) -> usize {
    let r = &result.report;
    let total = r.converted + r.skipped.len();
    let mut partial = Counts::new();
    let mut skipped = Counts::new();
    for n in &r.partial {
        bump(&mut partial, n.kind.clone());
    }
    for n in &r.skipped {
        bump(&mut skipped, n.kind.clone());
    }
    let elements: usize = result.packs.iter().map(|p| p.element_count()).sum();
    println!(
        "=== {label}: {} packs, {elements} elements; converted {} of {total} entries, {} partial notes, {} skipped, {} warnings, {} external refs",
        result.packs.len(),
        r.converted,
        r.partial.len(),
        r.skipped.len(),
        r.warnings.len(),
        r.external_references.len()
    );
    table("partial notes by kind", &partial, 20);
    table("skipped by kind", &skipped, 20);
    elements
}

#[test]
fn five_etools_corpus() {
    let Ok(dir) = std::env::var("BOREALIS_5ETOOLS_DIR") else {
        eprintln!("five_etools_corpus: BOREALIS_5ETOOLS_DIR not set, skipping (local-only corpus)");
        return;
    };
    let data = PathBuf::from(dir);
    assert!(data.is_dir(), "BOREALIS_5ETOOLS_DIR is not a directory");

    let scan = borealis_import_5etools::scan_folder(&data).expect("scan");
    assert!(scan
        .iter()
        .any(|s| s.abbr == "PHB" && s.ruleset == "dnd2014"));
    assert!(scan
        .iter()
        .any(|s| s.abbr == "XPHB" && s.ruleset == "dnd2024"));
    println!("=== scan: {} sources", scan.len());

    let first = import_folder(&data, &options()).expect("full import");
    let elements = summarize("all sources", &first);
    let r = &first.report;
    let total = r.converted + r.skipped.len();
    assert!(
        first.packs.len() >= MIN_PACKS,
        "packs: {}",
        first.packs.len()
    );
    assert!(elements >= MIN_ELEMENTS, "elements: {elements}");
    assert!(
        r.converted * 100 >= total * MIN_CONVERTED_PERCENT,
        "converted {} of {total}",
        r.converted
    );
    assert!(
        r.skipped.len() <= MAX_SKIPPED,
        "skipped {}",
        r.skipped.len()
    );
    assert!(
        r.partial.len() <= MAX_PARTIAL,
        "partial notes {}",
        r.partial.len()
    );
    for (prefix, needle) in CLOSED_GAPS {
        let open = r
            .partial
            .iter()
            .filter(|n| n.reason.starts_with(prefix) && n.reason.contains(needle))
            .count();
        assert_eq!(open, 0, "closed gap reappeared: {prefix:?} ({open})");
    }
    let prereq_notes = r
        .partial
        .iter()
        .filter(|n| n.reason.starts_with("prerequisite not enforced"))
        .count();
    println!("unenforced prerequisite notes: {prereq_notes}");
    assert!(
        prereq_notes <= MAX_UNENFORCED_PREREQS,
        "unenforced prerequisites {prereq_notes}"
    );
    let spell_notes: Vec<&str> = r
        .partial
        .iter()
        .map(|n| n.reason.as_str())
        .filter(|n| {
            !n.starts_with("prerequisite")
                && !n.starts_with("spell choice filter")
                && (n.contains("spell") || n.contains("keyed"))
        })
        .collect();
    // additional spells (expanded lists, innate/limited spells, alternative groups) convert
    // completely; spell choice filters on fields without tags are a separate gap
    assert!(
        spell_notes.is_empty(),
        "additional spell notes: {spell_notes:?}"
    );
    // a full folder defines everything it links to
    assert!(
        r.external_references.is_empty(),
        "external refs: {:?}",
        r.external_references
    );
    assert!(first.packs.iter().all(|p| !p.redistributable));

    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let dirs = write_packs(&first, a.path());
    assert_eq!(schema_check(&dirs), 0, "schema violations");
    let (issues, undocumented) = store_check(&first, &dirs);
    table("store issues (severity/code)", &issues, 20);
    assert!(
        undocumented.is_empty(),
        "{} undocumented store issues, first: {:?}",
        undocumented.len(),
        &undocumented[..undocumented.len().min(20)]
    );

    let second = import_folder(&data, &options()).expect("second import");
    write_packs(&second, b.path());
    let (mut ta, mut tb) = (BTreeMap::new(), BTreeMap::new());
    tree(a.path(), &mut ta);
    tree(b.path(), &mut tb);
    let strip = |m: BTreeMap<String, Vec<u8>>, root: &Path| -> BTreeMap<String, Vec<u8>> {
        let prefix = root.to_string_lossy().to_string();
        m.into_iter()
            .map(|(k, v)| (k.replacen(&prefix, "", 1), v))
            .collect()
    };
    assert!(
        strip(ta, a.path()) == strip(tb, b.path()),
        "two runs differ"
    );
    assert_eq!(first.report, second.report);

    let fluff = import_folder(
        &data,
        &ImportOptions {
            include_fluff: true,
            ..options()
        },
    )
    .expect("fluff import");
    summarize("all sources with fluff", &fluff);
    let f = tempfile::tempdir().unwrap();
    let fluff_dirs = write_packs(&fluff, f.path());
    assert_eq!(schema_check(&fluff_dirs), 0, "schema violations (fluff)");

    for source in ["PHB", "XPHB"] {
        let one = import_folder(&data, &options().with_sources([source])).expect("one source");
        summarize(source, &one);
        assert_eq!(one.packs.len(), 1);
        let d = tempfile::tempdir().unwrap();
        let dirs = write_packs(&one, d.path());
        let (_, undocumented) = store_check(&one, &dirs);
        assert!(undocumented.is_empty(), "{source}: {undocumented:?}");
    }
}
