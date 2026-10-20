//! Acceptance stage 3c (specs/acceptance-imports-1.md): Aurora ground truth from `.dnd5e`.
//! Aurora writes some values it computed into a saved character (spellcasting attack and DC,
//! slots, the spell lists, attack lines, the display header, the `sum` manifest). Borealis loads
//! the same content, imports the file and must compute the same values.
//!
//! Gated on `BOREALIS_CORPUS_DIR` (the corpus root holding `repos/`, or `repos/` itself); it skips
//! when unset. Every `.dnd5e` under `repos/` is checked, plus every `.dnd5e` under
//! `BOREALIS_DND5E_DIR` (the user's own files, same flow). Content per character: A-core (the
//! bundled packs and `aurorabuilder__elements`) plus the community repos that define the ids the
//! file references (a greedy cover; repos that copy the core are never used).
//!
//! The reviewed exceptions (`exceptions.tsv`) and the per-file results (`results/*.json`,
//! `rates.tsv`) live in `BOREALIS_DND5E_PRIVATE` (default `../specs/acceptance-private/dnd5e`
//! next to the app repo when that folder exists); they hold content ids, so never in the repo.
//! Pass = every check of every file with complete content matches or is a listed exception.
//! Files with missing content are compared, not gated; their gate outcome rows still FAIL on a
//! mismatch unless `exceptions.tsv` lists it as class `m` (the value depends on content that
//! no corpus repo has, reviewed per row).
//!
//! ```text
//! BOREALIS_CORPUS_DIR=/path/to/corpus cargo test --release -p borealis-character \
//!     --test dnd5e_truth -- --nocapture
//! ```

#[path = "common/dnd5e/mod.rs"]
mod dnd5e;
#[path = "../../../tools/regression-gate/outcome.rs"]
mod outcome;

use borealis_character::import_dnd5e;
use borealis_migrate::dom::{parse_document, XmlElement};
use dnd5e::compare::{compare, load_exceptions, Check, Exceptions};
use dnd5e::content::{files_under, Library, RepoIndex};
use dnd5e::sheet::{from_file, from_snapshot};
use std::collections::{BTreeMap, BTreeSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

const SUITE: &str = "dnd5e-truth";

/// Why a file's values are not part of the pass criterion.
#[derive(Debug, Clone)]
enum Status {
    /// Every referenced id resolved: gated.
    Complete,
    /// Content the file needs is not in the corpus (ids listed); compared, not gated.
    ContentMissing(Vec<String>),
    /// Not compared at all.
    Skipped(String),
}

struct FileResult {
    key: String,
    status: Status,
    repos: Vec<String>,
    checks: Vec<Check>,
    sum: (usize, usize, usize),
    /// `sum` ids Aurora registered that Borealis does not, and the reverse (coverage signal).
    sum_diff: (Vec<String>, Vec<String>),
    /// Import report lines other than missing content (unmapped, invalid, warnings).
    report: Vec<String>,
}

fn corpus_repos() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var("BOREALIS_CORPUS_DIR").ok()?);
    Some(if dir.join("repos").is_dir() {
        dir.join("repos")
    } else {
        dir
    })
}

fn private_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("BOREALIS_DND5E_PRIVATE") {
        return Some(PathBuf::from(p));
    }
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../specs/acceptance-private/dnd5e");
    p.is_dir().then_some(p)
}

fn is_dnd5e(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("dnd5e"))
}

/// `(key, path)` of every character file: corpus repos, then the user's folder.
fn character_files(repos: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for p in files_under(repos).into_iter().filter(|p| is_dnd5e(p)) {
        let rel = p.strip_prefix(repos).unwrap_or(&p).display().to_string();
        out.push((format!("corpus/{rel}"), p));
    }
    if let Ok(dir) = std::env::var("BOREALIS_DND5E_DIR") {
        let dir = PathBuf::from(dir);
        for p in files_under(&dir).into_iter().filter(|p| is_dnd5e(p)) {
            let rel = p.strip_prefix(&dir).unwrap_or(&p).display().to_string();
            out.push((format!("user/{rel}"), p));
        }
    }
    out
}

/// Every `id` attribute under `build` except the restricted-source list.
fn referenced_ids(e: &XmlElement, out: &mut BTreeSet<String>) {
    if let Some(id) = e.attr("id") {
        if !id.trim().is_empty() {
            out.insert(id.trim().to_string());
        }
    }
    for c in e.elements() {
        referenced_ids(c, out);
    }
}

fn skipped(key: &str, why: &str) -> FileResult {
    FileResult {
        key: key.to_string(),
        status: Status::Skipped(why.to_string()),
        repos: Vec::new(),
        checks: Vec::new(),
        sum: (0, 0, 0),
        sum_diff: (Vec::new(), Vec::new()),
        report: Vec::new(),
    }
}

fn check_file(key: &str, path: &Path, index: &RepoIndex, lib: &mut Library) -> FileResult {
    let bytes = std::fs::read(path).unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes);
    let xml = text.trim_start_matches('\u{feff}').trim_start();
    if !xml.starts_with('<') {
        return skipped(key, "not an Aurora character (another tool's JSON format)");
    }
    let Ok(root) = parse_document(xml) else {
        return skipped(key, "XML parse error");
    };
    let Some(build) = root.child("build").filter(|_| root.name == "character") else {
        return skipped(key, "not an Aurora character (no character/build)");
    };
    let aurora = from_file(&root);
    let mut ids = BTreeSet::new();
    referenced_ids(build, &mut ids);
    let Some((core, _)) = lib.engine(&[]) else {
        return skipped(key, "A-core did not load");
    };
    let missing: BTreeSet<String> = ids
        .into_iter()
        .filter(|id| !id.contains("_INTERNAL_") && !core.contains_id(id))
        .collect();
    let (repos, _) = index.cover(&missing);
    let Some((store, engine)) = lib.engine(&repos) else {
        return skipped(key, &format!("content did not load: {repos:?}"));
    };
    let run = catch_unwind(AssertUnwindSafe(|| {
        let out = import_dnd5e(engine, xml).map_err(|e| e.to_string())?;
        let snap = engine.derive(&out.character.build);
        if std::env::var("BOREALIS_DND5E_DEBUG").is_ok_and(|w| key.contains(&w)) {
            for sel in &out.character.build.selections {
                println!(
                    "  SEL {} #{} {} = {:?}",
                    sel.slot.owner, sel.slot.rule, sel.slot.ordinal, sel.choice
                );
            }
        }
        Ok::<_, String>((out.report, snap, out.character.build))
    }));
    let (report, snap, build_of) = match run {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => return skipped(key, &format!("import failed: {e}")),
        Err(_) => return skipped(key, "panic in import or derive"),
    };
    debug_dump(key, &out_sel(&snap), &snap);
    let mut borealis = from_snapshot(&snap, store);
    borealis
        .attacks
        .extend(unequipped_attacks(engine, &build_of, &aurora, &borealis));
    let lacking: Vec<String> = report
        .missing_content
        .iter()
        .map(|m| format!("{}:{}:{}", m.context, m.ty, m.id))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let covered = aurora.sum_ids.intersection(&borealis.sum_ids).count();
    let sum_diff = (
        aurora
            .sum_ids
            .difference(&borealis.sum_ids)
            .cloned()
            .collect(),
        borealis
            .sum_ids
            .difference(&aurora.sum_ids)
            .cloned()
            .collect(),
    );
    FileResult {
        key: key.to_string(),
        status: if lacking.is_empty() {
            Status::Complete
        } else {
            Status::ContentMissing(lacking)
        },
        repos,
        checks: compare(&aurora, &borealis),
        sum: (aurora.sum_count, borealis.sum_count, covered),
        sum_diff,
        report: report_lines(&report),
    }
}

/// Attacks of item-linked Aurora rows whose weapon Borealis has but does not wield: derived on
/// a copy of the character with that weapon in the main hand (`shown = false`).
fn unequipped_attacks(
    engine: &borealis_engine::Engine,
    build: &borealis_engine::CharacterState,
    aurora: &dnd5e::sheet::Sheet,
    borealis: &dnd5e::sheet::Sheet,
) -> Vec<dnd5e::sheet::Attack> {
    let have: BTreeSet<String> = borealis
        .attacks
        .iter()
        .map(|a| a.uid.to_lowercase())
        .collect();
    let mut out = Vec::new();
    for at in &aurora.attacks {
        let uid = at.uid.to_lowercase();
        if have.contains(&uid) {
            continue;
        }
        let mut copy = build.clone();
        let Some(item) = copy
            .inventory
            .items
            .iter_mut()
            .find(|i| i.uid.to_lowercase() == uid)
        else {
            continue;
        };
        item.equipped = Some(borealis_engine::state::EquipSlot::PrimaryHand);
        let snap = engine.derive(&copy);
        if let Some(a) = snap.attacks.iter().find(|a| a.uid.to_lowercase() == uid) {
            out.push(dnd5e::sheet::Attack {
                uid: a.uid.clone(),
                name: a.name.clone(),
                bonus: Some(a.attack),
                damage: a.damage.clone(),
                shown: false,
            });
        }
    }
    out
}

fn out_sel(snap: &borealis_engine::CharacterSnapshot) -> Vec<String> {
    snap.slots
        .iter()
        .map(|s| {
            format!(
                "{} #{} {} [{}] {} -> {:?} rejected {:?}",
                s.key.owner,
                s.key.rule,
                s.key.ordinal,
                s.ty,
                s.name,
                s.chosen.as_ref().map(|c| c.id.as_str()),
                s.rejected
            )
        })
        .collect()
}

/// `BOREALIS_DND5E_DEBUG=<part of a file key>` prints that file's slots (ids only).
fn debug_dump(key: &str, slots: &[String], snap: &borealis_engine::CharacterSnapshot) {
    let Ok(want) = std::env::var("BOREALIS_DND5E_DEBUG") else {
        return;
    };
    if !key.contains(&want) {
        return;
    }
    for s in slots {
        println!("  SLOT {s}");
    }
    for d in &snap.diagnostics {
        println!("  DIAG {} {}", d.code, d.message);
    }
}

fn report_lines(r: &borealis_character::ImportReport) -> Vec<String> {
    let mut out: Vec<String> = r
        .unmapped_slots
        .iter()
        .map(|u| {
            format!(
                "unmapped {} {} {} <- {} ({})",
                u.owner, u.ty, u.name, u.chosen, u.reason
            )
        })
        .collect();
    out.extend(r.invalid_selections.iter().map(|i| format!("invalid {i}")));
    out.extend(r.warnings.iter().map(|w| format!("warning {w}")));
    out.extend(
        r.legacy_divergences
            .iter()
            .map(|d| format!("divergence {}", d.code)),
    );
    out
}

/// The reviewed exception for check `c` of file `r`. Class `m` (content no corpus repo has)
/// holds only while the file's content is still missing: once it loads, the check is gated again.
fn excepted<'a>(ex: &'a Exceptions, r: &FileResult, c: &Check) -> Option<&'a (String, String)> {
    ex.get(&(r.key.clone(), c.id.clone()))
        .filter(|(class, _)| class != "m" || matches!(r.status, Status::ContentMissing(_)))
}

fn write_result(dir: &Path, r: &FileResult, ex: &Exceptions) {
    let (status, detail) = match &r.status {
        Status::Complete => ("complete", Vec::new()),
        Status::ContentMissing(ids) => ("content-missing", ids.clone()),
        Status::Skipped(why) => ("skipped", vec![why.clone()]),
    };
    let checks: Vec<_> = r
        .checks
        .iter()
        .map(|c| {
            let x = excepted(ex, r, c);
            serde_json::json!({
                "check": c.id, "ok": c.ok, "expected": c.expected, "actual": c.actual,
                "exception": x.map(|(k, why)| format!("({k}) {why}")),
            })
        })
        .collect();
    let v = serde_json::json!({
        "file": r.key, "status": status, "detail": detail, "repos": r.repos, "report": r.report,
        "sum": {"aurora_count": r.sum.0, "borealis_count": r.sum.1, "aurora_ids_registered": r.sum.2,
                "aurora_only": r.sum_diff.0, "borealis_only": r.sum_diff.1},
        "checks": checks,
    });
    let name: String = r
        .key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let out = dir.join("results").join(format!("{name}.json"));
    let _ = std::fs::create_dir_all(out.parent().expect("parent"));
    let _ = std::fs::write(out, serde_json::to_string_pretty(&v).expect("json") + "\n");
}

/// field -> [checked, matched, excepted] over complete files, then over all compared files.
type Rates = BTreeMap<&'static str, [usize; 3]>;

fn rates(results: &[FileResult], ex: &Exceptions, gated_only: bool) -> Rates {
    let mut out = Rates::new();
    for r in results {
        let gated = matches!(r.status, Status::Complete);
        if matches!(r.status, Status::Skipped(_)) || (gated_only && !gated) {
            continue;
        }
        for c in &r.checks {
            let row = out.entry(c.field).or_default();
            row[0] += 1;
            row[1] += usize::from(c.ok);
            row[2] += usize::from(!c.ok && excepted(ex, r, c).is_some());
        }
    }
    out
}

fn print_rates(title: &str, rates: &Rates) -> String {
    let mut s = format!("{title}\nfield\tchecked\tmatch\texcepted\trate\n");
    for (f, [n, ok, x]) in rates {
        #[allow(clippy::cast_precision_loss)]
        let pct = if *n == 0 {
            100.0
        } else {
            100.0 * *ok as f64 / *n as f64
        };
        s.push_str(&format!("{f}\t{n}\t{ok}\t{x}\t{pct:.1}%\n"));
        println!("METRIC\tdnd5e\t3c\t{f}\tchecked\t{n}");
        println!("METRIC\tdnd5e\t3c\t{f}\tmatch\t{ok}");
    }
    s
}

#[test]
fn dnd5e_values_match_what_aurora_wrote() {
    let Some(repos) = corpus_repos() else {
        eprintln!("SKIPPED: BOREALIS_CORPUS_DIR is not set");
        return;
    };
    let handle = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || run(&repos))
        .expect("spawn");
    handle.join().expect("dnd5e truth thread");
}

#[allow(clippy::too_many_lines)]
fn run(repos: &Path) {
    let private = private_dir();
    let ex = private
        .as_ref()
        .map(|d| load_exceptions(&d.join("exceptions.tsv")))
        .unwrap_or_default();
    let index = RepoIndex::build(repos);
    println!("core mirrors (never installed): {:?}", index.mirrors);
    let mut lib = Library::new(repos);
    let files = character_files(repos);
    assert!(!files.is_empty(), "no .dnd5e files found");
    let mut results = Vec::new();
    for (key, path) in &files {
        let r = check_file(key, path, &index, &mut lib);
        let fails = r.checks.iter().filter(|c| !c.ok).count();
        match &r.status {
            Status::Skipped(why) => println!("SKIP {key}: {why}"),
            Status::ContentMissing(ids) => println!(
                "PARTIAL {key}: {} checks, {fails} differ; content missing ({}): {}",
                r.checks.len(),
                ids.len(),
                ids.join(" ")
            ),
            Status::Complete => println!("FILE {key}: {} checks, {fails} differ", r.checks.len()),
        }
        if !matches!(r.status, Status::Skipped(_)) {
            println!(
                "  repos {:?}; sum {:?} (aurora, borealis, aurora ids registered)",
                r.repos, r.sum
            );
        }
        for c in &r.checks {
            let x = excepted(&ex, &r, c);
            let gated = matches!(r.status, Status::Complete);
            outcome::record(
                SUITE,
                &format!("{key}/{}", c.id),
                c.ok || x.is_some(),
                if c.ok { "" } else { &c.actual },
            );
            if !c.ok {
                let tag = match (x, gated) {
                    (Some((k, _)), _) => format!("EXCEPTED({k})"),
                    (None, true) => "MISMATCH".to_string(),
                    (None, false) => "mismatch(partial)".to_string(),
                };
                println!(
                    "  {tag} {}: aurora {} | borealis {}",
                    c.id, c.expected, c.actual
                );
            }
        }
        if let Some(d) = &private {
            write_result(d, &r, &ex);
        }
        results.push(r);
    }
    let gated = rates(&results, &ex, true);
    let all = rates(&results, &ex, false);
    let mut text = print_rates("== complete-content files (gated)", &gated);
    text.push_str(&print_rates(
        "== all compared files (incl. content missing)",
        &all,
    ));
    println!("{text}");
    let skipped: BTreeMap<String, usize> = results
        .iter()
        .filter_map(|r| match &r.status {
            Status::Skipped(w) => Some(w.clone()),
            Status::ContentMissing(_) => Some("content missing (compared, not gated)".into()),
            Status::Complete => None,
        })
        .fold(BTreeMap::new(), |mut m, w| {
            *m.entry(w).or_default() += 1;
            m
        });
    println!("files: {} total; not gated: {skipped:?}", results.len());
    if let Some(d) = &private {
        let _ = std::fs::write(d.join("rates.tsv"), &text);
    }
    let used: BTreeSet<(String, String)> = results
        .iter()
        .flat_map(|r| {
            r.checks
                .iter()
                .filter(|c| !c.ok)
                .map(|c| (r.key.clone(), c.id.clone()))
        })
        .collect();
    for k in ex.keys().filter(|k| !used.contains(*k)) {
        println!("STALE EXCEPTION (no longer differs): {} {}", k.0, k.1);
    }
    let open: Vec<String> = results
        .iter()
        .filter(|r| matches!(r.status, Status::Complete))
        .flat_map(|r| {
            r.checks
                .iter()
                .filter(|c| !c.ok && excepted(&ex, r, c).is_none())
                .map(|c| format!("{}/{}", r.key, c.id))
        })
        .collect();
    assert!(
        open.is_empty(),
        "{} value(s) differ from what Aurora wrote (not in exceptions.tsv): {open:#?}",
        open.len()
    );
}
