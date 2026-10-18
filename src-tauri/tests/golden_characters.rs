//! Acceptance 3e: the 26 SRD golden characters (`tests/goldens/`), built through the builder
//! commands from the bundled SRD packs only and compared with the hand-derived expected sheets.
//!
//! Every spec choice is a step in `goldens/mapping.json` (spec name -> Borealis option name ->
//! element id). Pass = every expected fact matches exactly and every step applied. With
//! `BOREALIS_OUTCOMES` set, each fact is one gate outcome (`golden/<id>/<fact>`).
//!
//! Env: `GOLDEN_ONLY=<id>[,<id>]` builds a subset, `GOLDEN_DUMP=<dir>` writes each built
//! snapshot, the build log and the mismatches, `GOLDEN_REPORT=<file>` the mismatch report,
//! `GOLDEN_RESOLVE=<file>` re-resolves every step's id by name (writes a new mapping there),
//! `GOLDEN_MAPPING=<file>` reads the steps from another file (the output of
//! `support/golden/gen_mapping.py`).

#[path = "support/golden/mod.rs"]
mod golden;
#[path = "../../tools/regression-gate/outcome.rs"]
mod outcome;

use borealis_lib::state::{AppPaths, AppState};
use golden::compare;
use serde_json::{json, Value};
use std::path::PathBuf;

const SUITE: &str = "golden";

fn app() -> (tempfile::TempDir, AppState) {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().join("data"),
    };
    (dir, AppState::new(paths).expect("state"))
}

fn metric(id: &str, name: &str, n: usize) {
    if std::env::var_os(outcome::ENV).is_some() {
        eprintln!("METRIC\tsrd\t3e\t{id}\t{name}\t{n}");
    }
}

/// One golden: build, compare, record. Returns the report lines (empty = pass).
fn check(
    app: &AppState,
    id: &str,
    mapping: &Value,
    resolve: bool,
    resolved: &mut Value,
) -> Vec<String> {
    let dir = golden::goldens_dir();
    let spec = golden::read_json(&dir.join(format!("specs/{id}.json")));
    let expected = golden::read_json(&dir.join(format!("expected/{id}.json")));
    let m = &mapping["goldens"][id];
    let built = golden::build::build(app, &spec, m, resolve);
    if let Some(r) = built.log.resolved.first() {
        resolved["goldens"][id]["levels"] = r["levels"].clone();
        resolved["goldens"][id]["post"] = r["post"].clone();
    }
    let snap = serde_json::to_value(&built.snap).expect("snapshot json");
    let want = compare::expected(&expected, m);
    let mut got = compare::actual(&snap, &want);
    for (k, v) in golden::weapons::probe(app, &built.snap) {
        got.entry(k).or_insert(v);
    }
    let bad = compare::diff(&want, &got);
    for (key, value) in &want {
        let miss = bad.iter().find(|b| &b.key == key);
        let detail = miss.map_or(String::new(), |b| {
            format!(
                "expected {value}, Borealis {}",
                b.actual.as_ref().map_or("none".into(), ToString::to_string)
            )
        });
        outcome::record(SUITE, &format!("{id}/{key}"), miss.is_none(), &detail);
    }
    let log = &built.log;
    for f in log.failed.iter().chain(&log.open).chain(&log.gaps) {
        outcome::record(SUITE, &format!("{id}/build/{f}"), false, "");
    }
    metric(id, "facts", want.len());
    metric(id, "mismatched", bad.len());
    metric(
        id,
        "build_problems",
        log.failed.len() + log.open.len() + log.gaps.len(),
    );
    let mut lines: Vec<String> = bad
        .iter()
        .map(|b| {
            let got = b
                .actual
                .as_ref()
                .map_or("(not shown)".to_string(), ToString::to_string);
            format!("{id}  {}: expected {}, Borealis {got}", b.key, b.expected)
        })
        .collect();
    lines.extend(
        log.failed
            .iter()
            .map(|f| format!("{id}  build step failed: {f}")),
    );
    lines.extend(
        log.open
            .iter()
            .map(|f| format!("{id}  required slot left open: {f}")),
    );
    lines.extend(log.gaps.iter().map(|f| format!("{id}  builder gap: {f}")));
    if let Some(d) = std::env::var_os("GOLDEN_DUMP").map(PathBuf::from) {
        let out = json!({
            "facts": want.len(),
            "mismatches": lines,
            "failed": log.failed,
            "open": log.open,
            "gaps": log.gaps,
            "notes": log.notes,
            "openOptions": log.open_options,
            "snap": snap,
        });
        std::fs::create_dir_all(&d).expect("dump dir");
        std::fs::write(
            d.join(format!("{id}.json")),
            serde_json::to_string_pretty(&out).expect("json"),
        )
        .expect("write dump");
    }
    let _ = app.close_character(&built.snap.id, true);
    lines
}

#[test]
fn golden_characters() {
    let (_dir, app) = app();
    let mapping_path = std::env::var_os("GOLDEN_MAPPING")
        .map_or_else(|| golden::goldens_dir().join("mapping.json"), PathBuf::from);
    let mapping = golden::read_json(&mapping_path);
    let only = std::env::var("GOLDEN_ONLY").ok();
    let resolve = std::env::var("GOLDEN_RESOLVE").ok().map(PathBuf::from);
    let mut resolved = mapping.clone();
    let mut report = Vec::new();
    let mut passed = 0usize;
    let ids = golden::ids();
    for id in &ids {
        if only
            .as_deref()
            .is_some_and(|o| !o.split(',').any(|x| x == id))
        {
            continue;
        }
        let lines = check(&app, id, &mapping, resolve.is_some(), &mut resolved);
        if lines.is_empty() {
            passed += 1;
        }
        report.extend(lines);
    }
    if let Some(path) = resolve {
        std::fs::write(path, serde_json::to_string_pretty(&resolved).expect("json"))
            .expect("write mapping");
    }
    if let Some(path) = std::env::var_os("GOLDEN_REPORT") {
        std::fs::write(path, report.join("\n") + "\n").expect("write report");
    }
    println!(
        "golden characters: {passed}/{} pass, {} problems",
        ids.len(),
        report.len()
    );
    assert!(
        report.is_empty(),
        "{} golden problems:\n{}",
        report.len(),
        report.join("\n")
    );
}
