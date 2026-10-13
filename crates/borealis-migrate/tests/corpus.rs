//! Local-only corpus sweep. Enabled by `BOREALIS_CORPUS_DIR`, which must point at a
//! directory whose children are independent content repositories. The corpus is never part
//! of the repository: this test prints counts only and never any content text or file name.
//!
//! Per file: no panic, schema-valid output, deterministic conversion. Per repository:
//! the converted content is loaded with the bundled core + SRD packs into a `ContentStore`
//! and validated (dangling ids, expressions, appends, ...).

use borealis_content::schema::{validate_document, DocumentKind};
use borealis_content::{ContentFile, ContentStoreBuilder, IndexFile, IssueCode, PackOrigin};
use borealis_migrate::{convert_xml, migrate_path, Document, MigrateOptions};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

type Counts = BTreeMap<String, usize>;

fn bump(map: &mut Counts, key: impl Into<String>) {
    *map.entry(key.into()).or_default() += 1;
}

fn add(map: &mut Counts, key: impl Into<String>, n: usize) {
    *map.entry(key.into()).or_default() += n;
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = read.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_symlink() || entry.file_name() == ".git" {
            continue;
        }
        if kind.is_dir() {
            collect_files(&path, out);
        } else {
            let lower = path.to_string_lossy().to_lowercase();
            if lower.ends_with(".xml") || lower.ends_with(".index") {
                out.push(path);
            }
        }
    }
}

#[derive(Default)]
struct Sweep {
    files: Counts,
    issues: Counts,
    unknown_children: Counts,
    unknown_attrs: Counts,
    normalized: Counts,
    failures: Vec<String>,
}

fn sweep_file(path: &Path, sweep: &mut Sweep) {
    let name = path.to_string_lossy().to_string();
    let is_index = name.to_lowercase().ends_with(".index");
    let Ok(bytes) = std::fs::read(path) else {
        bump(&mut sweep.files, "unreadable");
        return;
    };
    let Ok(text) = String::from_utf8(bytes) else {
        bump(&mut sweep.files, "not-utf8");
        return;
    };
    let kind = if is_index { "index" } else { "xml" };
    bump(&mut sweep.files, kind);
    let run = || {
        std::panic::catch_unwind(|| {
            let a = convert_xml(&text, &name);
            let b = convert_xml(&text, &name);
            (a, b)
        })
    };
    let Ok((first, second)) = run() else {
        bump(&mut sweep.files, "PANIC");
        sweep.failures.push(format!("panic in a {kind} file"));
        return;
    };
    if first != second {
        bump(&mut sweep.files, "NONDETERMINISTIC");
        sweep.failures.push(format!("nondeterministic {kind} file"));
    }
    for issue in &first.issues {
        bump(
            &mut sweep.issues,
            format!("{:?}/{:?}", issue.severity, issue.code),
        );
    }
    for (k, v) in &first.stats.unknown_children {
        add(&mut sweep.unknown_children, k.clone(), *v);
    }
    for (k, v) in &first.stats.unknown_attributes {
        add(&mut sweep.unknown_attrs, k.clone(), *v);
    }
    for (k, v) in &first.stats.normalized {
        add(&mut sweep.normalized, k.clone(), *v);
    }
    let (doc_kind, json) = match &first.document {
        None => {
            bump(&mut sweep.files, "no-document");
            return;
        }
        Some(Document::Content(f)) => (DocumentKind::Content, f.to_json_pretty()),
        Some(Document::Index(i)) => (DocumentKind::Index, i.to_json_pretty()),
    };
    let Ok(json) = json else {
        bump(&mut sweep.files, "SERIALIZE-FAIL");
        sweep
            .failures
            .push(format!("serialize failure in a {kind} file"));
        return;
    };
    let value: serde_json::Value = serde_json::from_str(&json).expect("own JSON parses");
    match validate_document(doc_kind, &value) {
        Ok(violations) if violations.is_empty() => {}
        _ => {
            bump(&mut sweep.files, "SCHEMA-INVALID");
            sweep.failures.push(format!("schema-invalid {kind} output"));
        }
    }
}

fn repo_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .map(|r| {
            r.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();
    dirs
}

fn base_builder() -> ContentStoreBuilder {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut builder = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        builder = builder
            .add_pack_dir(&content.join(pack))
            .expect("bundled pack loads");
    }
    builder
}

#[derive(Default)]
struct Semantic {
    repos: usize,
    repos_loaded: usize,
    elements: usize,
    issues: Counts,
    unknown_rules: usize,
    unknown_children: Counts,
    unknown_attrs: Counts,
    dangling_by_target: Counts,
}

/// Bucket a dangling target id by its prefix (core/SRD-looking ids vs the rest).
fn target_bucket(message: &str) -> &'static str {
    let id = message.split_whitespace().last().unwrap_or("");
    if id.starts_with("ID_INTERNAL") {
        "ID_INTERNAL_*"
    } else if id.starts_with("ID_WOTC") {
        "ID_WOTC_*"
    } else if id.starts_with("ID_PHB") {
        "ID_PHB_*"
    } else {
        "other"
    }
}

fn semantic_repo(repo: &Path, sem: &mut Semantic) {
    sem.repos += 1;
    let opts = MigrateOptions {
        validate: false,
        ..MigrateOptions::default()
    };
    let Ok(out) = migrate_path(repo, &opts) else {
        return;
    };
    let mut builder = base_builder();
    for file in &out.files {
        if file.path == "pack.json" {
            continue;
        }
        if file.path.ends_with(".index.json") {
            if let Ok(index) = IndexFile::from_json_str(&file.contents) {
                builder = builder.add_index(file.path.clone(), index);
            }
        } else if let Ok(content) = ContentFile::from_json_str(&file.contents) {
            builder = builder.add_file(file.path.clone(), PackOrigin::Custom, content);
        }
    }
    let store = builder.build();
    sem.repos_loaded += 1;
    sem.elements += store.len();
    let mut seen = std::collections::BTreeSet::new();
    for issue in store.diagnostics().iter().cloned().chain(store.validate()) {
        // Elements that exist only in the bundled packs are not the corpus's fault.
        let key = format!("{:?}/{:?}", issue.severity, issue.code);
        if seen.insert(format!("{key}|{}|{:?}", issue.message, issue.element)) {
            if issue.code == IssueCode::DanglingReference {
                bump(&mut sem.dangling_by_target, target_bucket(&issue.message));
            }
            bump(&mut sem.issues, key);
        }
    }
    sem.unknown_rules += out
        .report
        .totals
        .rules_by_kind
        .get("unknown")
        .copied()
        .unwrap_or(0);
    for (k, v) in &out.report.totals.unknown_children {
        add(&mut sem.unknown_children, k.clone(), *v);
    }
    for (k, v) in &out.report.totals.unknown_attributes {
        add(&mut sem.unknown_attrs, k.clone(), *v);
    }
}

fn table(title: &str, rows: &Counts, top: usize) {
    println!("--- {title}");
    let mut sorted: Vec<_> = rows.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (k, v) in sorted.into_iter().take(top) {
        println!("{v:>8}  {k}");
    }
}

#[test]
fn corpus_sweep() {
    let Ok(dir) = std::env::var("BOREALIS_CORPUS_DIR") else {
        eprintln!("corpus_sweep: BOREALIS_CORPUS_DIR not set, skipping (local-only corpus)");
        return;
    };
    let root = PathBuf::from(dir);
    assert!(root.is_dir(), "BOREALIS_CORPUS_DIR is not a directory");
    // Deep documents need more stack than the default test thread has.
    let handle = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || run_sweep(&root))
        .expect("spawn");
    handle.join().expect("sweep thread");
}

fn run_sweep(root: &Path) {
    let repos = repo_dirs(root);
    let mut sweep = Sweep::default();
    let mut sem = Semantic::default();
    for repo in &repos {
        let mut files = Vec::new();
        collect_files(repo, &mut files);
        for f in &files {
            sweep_file(f, &mut sweep);
        }
        semantic_repo(repo, &mut sem);
    }
    println!("=== corpus sweep: {} repos", repos.len());
    table("files", &sweep.files, 20);
    table("conversion diagnostics (severity/code)", &sweep.issues, 30);
    table("normalized", &sweep.normalized, 20);
    table(
        "unknown children preserved (owner/tag)",
        &sweep.unknown_children,
        15,
    );
    table(
        "unknown attributes preserved (owner@attr)",
        &sweep.unknown_attrs,
        15,
    );
    println!(
        "=== semantic: {} repos loaded of {}, {} elements (incl. bundled packs, per repo)",
        sem.repos_loaded, sem.repos, sem.elements
    );
    table(
        "store issues, distinct per repo (severity/code)",
        &sem.issues,
        30,
    );
    table("dangling by target prefix", &sem.dangling_by_target, 10);
    println!("unknown rule kinds preserved: {}", sem.unknown_rules);
    let errors: usize = sem
        .issues
        .iter()
        .filter(|(k, _)| k.starts_with("Error"))
        .map(|(_, v)| v)
        .sum();
    let dangling = sem
        .issues
        .iter()
        .filter(|(k, _)| k.ends_with(&format!("{:?}", IssueCode::DanglingReference)))
        .map(|(_, v)| v)
        .sum::<usize>();
    println!("store errors: {errors}, dangling references: {dangling}");
    assert!(
        sweep.failures.is_empty(),
        "corpus failures: {:?}",
        sweep.failures
    );
    assert_eq!(sem.repos, sem.repos_loaded, "every repo loads into a store");
}
