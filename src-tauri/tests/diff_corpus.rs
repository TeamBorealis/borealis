//! Corpus differentials with Aurora content (acceptance step 3d (ii) and (iii),
//! specs/acceptance-imports-1.md):
//!
//! - (ii) `acore_srd_overlap_matches_bundled`: the A-core classes that overlap the SRD (PHB
//!   Fighter / Champion, Wizard / Evocation, ... with the SRD subclass) against the bundled
//!   SRD 5.1 pack. A-core uses the bundled ids, so both sides name the same elements; the left
//!   side is built on the bundled packs alone, the right side on the app's store after Update
//!   indexes (A-core replaces the bundled elements of the same id).
//! - (iii) `acore_matches_five_tools`: A-core against F-full (a 5etools `data/` folder) for the
//!   same non-SRD PHB class / subclass. Local only: the pair list and the findings that name
//!   non-SRD content are read from `specs/acceptance-private/diff/three_way.json`.
//!
//! Both build and normalise characters exactly as `diff_bundled.rs` (shared
//! `crates/borealis-import-5etools/tests/support/diff.rs`) at levels 1, 3, 5, 11 and 20; options
//! are taken by name, SRD 5.1 names first. Accepted: the LOSSY list, option-count supersets
//! (the corpus holds more books than the SRD), and findings classified as source differences;
//! every other difference is a classified Borealis bug (the test fails while one is observed)
//! or unclassified (fails).
//!
//! Sources (each test skips when its variables are unset; `BOREALIS_ACCEPT_STRICT=1` fails on a
//! skip): A-core from `BOREALIS_ACORE_DATA` (the data dir of an app that already ran Update
//! indexes on core.index, such as `target/tmp/acceptance-core/data` after the acceptance
//! harness) or else `BOREALIS_LIVE_INDEX_URL` (installed here through `update_indexes`, as the
//! app does); F-full from `BOREALIS_5ETOOLS_DIR`; the private pair list from
//! `BOREALIS_DIFF_PRIVATE` (default `../specs/acceptance-private/diff` next to the app repo).
//! Prints ids, keys and numbers only. Run in release mode:
//!
//! ```text
//! BOREALIS_ACORE_DATA=... BOREALIS_5ETOOLS_DIR=... cargo test --release -p borealis \
//!     --test diff_corpus -- --nocapture --test-threads 1
//! ```

#[path = "../../crates/borealis-import-5etools/tests/support/diff.rs"]
mod diff;
#[path = "../../crates/borealis-import-5etools/tests/support/diff_findings.rs"]
mod diff_findings;
#[path = "../../tools/regression-gate/outcome.rs"]
mod outcome;

use borealis_content::{ContentStore, ContentStoreBuilder, Ruleset};
use borealis_engine::Engine;
use borealis_import_5etools::{import_folder, ImportOptions};
use borealis_lib::content_index::HttpsFetcher;
use borealis_lib::content_packs::NullSink;
use borealis_lib::state::{AppPaths, AppState};
use diff::{Diff, Finding, Owner, Pair, Prefer, Side, Verdicts, LOSSY};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const SUITE: &str = "diff_corpus";
const BUNDLED: [&str; 3] = ["core", "srd-5.1", "srd-5.2.1"];

/// The SRD classes and subclasses (bundled SRD 5.1 ids, which A-core shares).
const SRD_CLASSES: [(&str, &str, &str); 12] = [
    (
        "Barbarian",
        "ID_WOTC_PHB_CLASS_BARBARIAN",
        "ID_WOTC_PHB_ARCHETYPE_PATH_OF_THE_BERSERKER",
    ),
    (
        "Bard",
        "ID_WOTC_PHB_CLASS_BARD",
        "ID_WOTC_PHB_ARCHETYPE_COLLEGE_OF_LORE",
    ),
    (
        "Cleric",
        "ID_WOTC_PHB_CLASS_CLERIC",
        "ID_WOTC_PHB_ARCHETYPE_LIFEDOMAIN",
    ),
    (
        "Druid",
        "ID_WOTC_PHB_CLASS_DRUID",
        "ID_WOTC_PHB_ARCHETYPE_CIRCLEOFTHELAND",
    ),
    (
        "Fighter",
        "ID_WOTC_PHB_CLASS_FIGHTER",
        "ID_WOTC_PHB_ARCHETYPE_CHAMPION",
    ),
    (
        "Monk",
        "ID_WOTC_PHB_CLASS_MONK",
        "ID_WOTC_PHB_ARCHETYPE_WAY_OF_THE_OPEN_HAND",
    ),
    (
        "Paladin",
        "ID_WOTC_PHB_CLASS_PALADIN",
        "ID_WOTC_PHB_ARCHETYPE_PALADIN_OATH_OF_DEVOTION",
    ),
    (
        "Ranger",
        "ID_WOTC_PHB_CLASS_RANGER",
        "ID_WOTC_ARCHETYPE_RANGER_HUNTER",
    ),
    (
        "Rogue",
        "ID_WOTC_PHB_CLASS_ROGUE",
        "ID_WOTC_PHB_ARCHETYPE_THIEF",
    ),
    (
        "Sorcerer",
        "ID_WOTC_PHB_CLASS_SORCERER",
        "ID_WOTC_PHB_ARCHETYPE_DRACONIC_BLOODLINE",
    ),
    (
        "Warlock",
        "ID_WOTC_PHB_CLASS_WARLOCK",
        "ID_WOTC_PHB_ARCHETYPE_OTHERWORLDLY_PATRON_FIEND",
    ),
    (
        "Wizard",
        "ID_WOTC_PHB_CLASS_WIZARD",
        "ID_WOTC_PHB_ARCHETYPE_WIZARD_SCHOOL_OF_EVOCATION",
    ),
];
/// Origin of every pair (bundled SRD 5.1 ids = A-core ids).
const HILL_DWARF: (&str, &str) = ("ID_SRD_RACE_DWARF", "ID_SUB_RACE_HILL_DWARF");
const ACOLYTE: &str = "ID_BACKGROUND_ACOLYTE";

/// A-core vs bundled SRD 5.1 differences, classified (2026-10-18 run at 139047b on a core.index
/// install). Removed as no longer observed: A2 (bundled Ranger selects, 2becf31) and A3 (A-core's
/// PHB 2024 content in a dnd2014-only pack, 180b74e: the Warlock now takes Eldritch Blast).
const ACORE_FINDINGS: &[Finding] = &[
    ("A1", "", &["select.Background Feature", "select.Feat"], &[], Owner::Source,
     "A-core's 2024 PHB content gives the 2014 Acolyte a 'Variant Feature' select whose option opens an origin-feat select (10 feats); the SRD 5.1 Acolyte has neither"),
    ("A4", "Paladin", &["save."], &[], Owner::Source,
     "Aurora's core XML computes 'aura of protection:save bonus' (minimum 1) but never adds it to the paladin's own saves; the bundled pack adds it (Cha 8 here, so +1)"),
    ("A5", "Wizard", &["spell_choices"], &[20], Owner::Source,
     "Aurora's Spell Mastery / Signature Spells features carry no spell selects; the bundled pack models them as four spell selects"),
    ("A6", "Warlock", &["select.Condition"], &[11, 20], Owner::Source,
     "Aurora's core XML Fiendish Resilience (Fiend 10) has no rules (the resistance is chosen at each rest); the bundled pack models it as a damage-resistance Condition select"),
    ("A7", "Fighter", &["initiative"], &[11, 20], Owner::Source,
     "Aurora's core XML Remarkable Athlete adds half proficiency only to the Str/Dex skills (acrobatics, athletics, sleight of hand, stealth); SRD 5.1 applies it to every Str/Dex/Con check, initiative included, as the bundled pack does since 93f8c9b (bundled side right)"),
];

fn content_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content")
}

fn skip(test: &str, why: &str) {
    assert!(
        std::env::var_os("BOREALIS_ACCEPT_STRICT").is_none(),
        "{test}: {why} (strict mode)"
    );
    eprintln!("{test}: {why}, skipping");
}

fn bundled_builder() -> ContentStoreBuilder {
    let mut b = ContentStoreBuilder::new();
    for p in BUNDLED {
        b = b
            .add_pack_dir(&content_dir().join(p))
            .unwrap_or_else(|e| panic!("bundled pack {p}: {e}"));
    }
    b
}

/// Every installed pack directory below an app data dir.
fn pack_dirs(data: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![data.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().is_some_and(|n| n == "pack.json") {
                out.push(dir.clone());
            }
        }
    }
    out.sort();
    out
}

/// The A-core app data dir: `BOREALIS_ACORE_DATA`, or a fresh Update indexes run on
/// `BOREALIS_LIVE_INDEX_URL` (kept in `tmp`). `None` when neither is set.
fn acore_data(tmp: &Path) -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("BOREALIS_ACORE_DATA") {
        let d = PathBuf::from(d);
        assert!(
            d.is_dir(),
            "BOREALIS_ACORE_DATA {} is not a directory",
            d.display()
        );
        return Some(d);
    }
    let url = std::env::var("BOREALIS_LIVE_INDEX_URL").ok()?;
    let data = tmp.join("data");
    let app = AppState::new(AppPaths {
        content_root: content_dir(),
        data: data.clone(),
    })
    .expect("app state");
    app.add_index(&url).expect("add index");
    let result = app
        .update_indexes(&HttpsFetcher, &NullSink)
        .expect("update indexes");
    assert!(
        result.failed.is_empty(),
        "update indexes: {} failed downloads",
        result.failed.len()
    );
    drop(app);
    Some(data)
}

/// The bundled packs plus every pack installed under `data`.
fn acore_store(data: &Path) -> ContentStore {
    let dirs = pack_dirs(data);
    assert!(
        !dirs.is_empty(),
        "no installed pack under {}",
        data.display()
    );
    let mut b = bundled_builder();
    for d in &dirs {
        b = b
            .add_pack_dir(d)
            .unwrap_or_else(|e| panic!("installed pack {}: {e}", d.display()));
    }
    b.build()
}

/// The bundled packs plus a 5etools folder imported as the app does.
fn five_store(dir: &Path, tmp: &Path) -> ContentStore {
    let options = ImportOptions {
        srd_dirs: vec![
            content_dir().join("srd-5.1"),
            content_dir().join("srd-5.2.1"),
        ],
        ..ImportOptions::default()
    };
    let result = import_folder(dir, &options).expect("5etools import");
    let mut b = bundled_builder();
    for pack in &result.packs {
        b = b
            .add_pack_dir(&pack.write_to(tmp).expect("write pack"))
            .expect("imported pack loads");
    }
    b.build()
}

fn side(class: &str, subclass: &str) -> Side {
    Side {
        race: HILL_DWARF.0.into(),
        subrace: Some(HILL_DWARF.1.into()),
        background: ACOLYTE.into(),
        class: class.into(),
        subclass: subclass.into(),
    }
}

/// Why a difference is accepted (`None`: a finding).
fn accept(d: &Diff) -> Option<String> {
    if let Some(i) = diff::covered(LOSSY, d) {
        return Some(format!("LOSSY {}", LOSSY[i].1));
    }
    if d.key.starts_with("select.") && diff::dominates(&d.left, &d.right) {
        return Some("CORPUS select superset".into());
    }
    None
}

/// Compare every pair; `findings` are tried in order (the first that covers a difference wins).
fn verdicts(
    pairs: &[Pair],
    left: &Engine,
    right: &Engine,
    prefer: Prefer<'_>,
    findings: &[Finding],
    stage: &str,
) -> Verdicts {
    let results = diff::par_map(pairs, diff::threads(), &|p: &Pair| {
        diff::differences(left, right, p, prefer)
    });
    let mut v = Verdicts::default();
    for (p, r) in pairs.iter().zip(results) {
        let mut bad = 0;
        match r {
            Err(e) => {
                v.build_errors.push(format!("{}: {e}", p.label));
                bad += 1;
            }
            Ok(diffs) => {
                for d in diffs {
                    let id = format!("{stage}/{}/L{}/{}", d.label, d.level, d.key);
                    if let Some(why) = accept(&d) {
                        outcome::record(SUITE, &id, true, &why);
                        *v.accepted.entry(why).or_default() += 1;
                    } else if let Some(i) = diff::classify(findings, &d) {
                        let (fid, .., owner, _) = findings[i];
                        outcome::record(
                            SUITE,
                            &id,
                            owner == Owner::Source,
                            &format!("{fid} {owner:?}: {d}"),
                        );
                        *v.classified.entry(i).or_default() += 1;
                        bad += usize::from(owner != Owner::Source);
                    } else {
                        outcome::record(SUITE, &id, false, &d.to_string());
                        v.unclassified.push(d.to_string());
                        bad += 1;
                    }
                }
            }
        }
        outcome::record(
            SUITE,
            &format!("{stage}/{}", p.label),
            bad == 0,
            &bad.to_string(),
        );
    }
    v
}

fn prefer_2014(bundled: &ContentStore) -> BTreeSet<String> {
    diff::names(bundled, &diff::edition_packs(Ruleset::Dnd2014))
}

fn run_acore(data: &Path) {
    let t0 = std::time::Instant::now();
    let bundled = bundled_builder().build();
    let acore = acore_store(data);
    let (left, right) = (Engine::new(&bundled), Engine::new(&acore));
    let prefer = prefer_2014(&bundled);
    let pairs: Vec<Pair> = SRD_CLASSES
        .iter()
        .map(|(name, c, s)| Pair {
            label: format!("Dnd2014 {name}/SRD Hill Dwarf Acolyte"),
            ruleset: Ruleset::Dnd2014,
            left: side(c, s),
            right: side(c, s),
        })
        .collect();
    if probe(&pairs, &left, &right, Some(&prefer)) {
        return;
    }
    let v = verdicts(
        &pairs,
        &left,
        &right,
        Some(&prefer),
        ACORE_FINDINGS,
        "acore-srd",
    );
    let labels: Vec<String> = pairs.iter().map(|p| p.label.clone()).collect();
    println!(
        "=== diff_corpus (ii) A-core vs bundled SRD 5.1: {} pairs",
        pairs.len()
    );
    let fails = v.report(ACORE_FINDINGS, &labels, true);
    println!("wall {:.1}s", t0.elapsed().as_secs_f64());
    assert!(fails.is_empty(), "{fails:#?}");
}

/// `BOREALIS_DIFF_PROBE=<label substrings, |-separated>` (+ `BOREALIS_DIFF_PROBE_LEVEL`):
/// print both sides of the matching pairs instead of comparing. Returns whether it probed.
fn probe(pairs: &[Pair], left: &Engine, right: &Engine, prefer: Prefer<'_>) -> bool {
    let Ok(probe) = std::env::var("BOREALIS_DIFF_PROBE") else {
        return false;
    };
    let level: u32 = std::env::var("BOREALIS_DIFF_PROBE_LEVEL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    for p in pairs
        .iter()
        .filter(|p| probe.split('|').any(|x| p.label.contains(x)))
    {
        for (name, e, s) in [("left", left, &p.left), ("right", right, &p.right)] {
            println!("--- probe {} {name} L{level}", p.label);
            for line in diff::probe(e, p.ruleset, s, level, prefer) {
                println!("  {line}");
            }
        }
    }
    true
}

/// The private three-way file: pairs and their classified findings.
struct ThreeWay {
    pairs: Vec<(String, String, String, String)>,
    findings: Vec<Finding>,
}

fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

fn three_way(path: &Path) -> ThreeWay {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let v: serde_json::Value = serde_json::from_str(&text).expect("three_way.json parses");
    let s = |x: &serde_json::Value, k: &str| {
        x[k].as_str()
            .unwrap_or_else(|| panic!("{k} missing"))
            .to_string()
    };
    let pairs = v["pairs"]
        .as_array()
        .expect("pairs")
        .iter()
        .map(|p| {
            (
                s(p, "class"),
                s(p, "acore_class"),
                s(p, "acore_subclass"),
                s(p, "five_subclass"),
            )
        })
        .collect();
    let findings = v["findings"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|f| {
            let keys: Vec<&'static str> = f["keys"]
                .as_array()
                .expect("keys")
                .iter()
                .map(|k| leak(k.as_str().expect("key")))
                .collect();
            let levels: Vec<u32> = f["levels"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter_map(|l| l.as_u64().and_then(|n| u32::try_from(n).ok()))
                .collect();
            let owner = match f["owner"].as_str() {
                Some("importer") => Owner::Importer,
                Some("migrate") => Owner::Migrate,
                Some("engine") => Owner::Engine,
                Some("bundled") => Owner::Bundled,
                Some("source") => Owner::Source,
                other => panic!("owner {other:?}"),
            };
            let finding: Finding = (
                leak(&s(f, "id")),
                leak(&s(f, "label")),
                Box::leak(keys.into_boxed_slice()),
                Box::leak(levels.into_boxed_slice()),
                owner,
                leak(&s(f, "reason")),
            );
            finding
        })
        .collect();
    ThreeWay { pairs, findings }
}

fn private_dir() -> PathBuf {
    std::env::var_os("BOREALIS_DIFF_PRIVATE").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../specs/acceptance-private/diff"),
        PathBuf::from,
    )
}

fn run_three_way(data: &Path, five: &Path, file: &Path) {
    let t0 = std::time::Instant::now();
    let tw = three_way(file);
    let tmp = tempfile::tempdir().expect("tmp");
    let bundled = bundled_builder().build();
    let acore = acore_store(data);
    let fstore = five_store(five, tmp.path());
    let (left, right) = (Engine::new(&acore), Engine::new(&fstore));
    let prefer = prefer_2014(&bundled);
    let race = diff::by_name(&fstore, "Race", "Dwarf", "ID_5ET_PHB_RACE_").expect("5etools dwarf");
    let sub = diff::by_name(&fstore, "Sub Race", "Dwarf (Hill)", "ID_5ET_PHB_SUBRACE_")
        .expect("hill dwarf");
    let bg =
        diff::by_name(&fstore, "Background", "Acolyte", "ID_5ET_PHB_BACKGROUND_").expect("acolyte");
    let mut missing = Vec::new();
    let mut pairs = Vec::new();
    for (class, ac, asub, fsub) in &tw.pairs {
        let fc = diff::by_name(&fstore, "Class", class, "ID_5ET_PHB_CLASS_");
        let fs = fc.as_ref().ok().and_then(|c| {
            fstore
                .iter()
                .map(|se| &se.element)
                .find(|e| {
                    e.ty == "Archetype" && &e.name == fsub && e.supports.iter().any(|s| s == c)
                })
                .map(|e| e.id.clone())
        });
        let (Ok(fc), Some(fs)) = (fc, fs) else {
            missing.push(format!("{class}/{fsub}"));
            continue;
        };
        pairs.push(Pair {
            label: format!("Dnd2014 {class}/{fsub} Hill Dwarf Acolyte"),
            ruleset: Ruleset::Dnd2014,
            left: side(ac, asub),
            right: Side {
                race: race.clone(),
                subrace: Some(sub.clone()),
                background: bg.clone(),
                class: fc,
                subclass: fs,
            },
        });
    }
    assert!(
        missing.is_empty(),
        "5etools subclasses not found: {missing:?}"
    );
    if probe(&pairs, &left, &right, Some(&prefer)) {
        return;
    }
    // private (non-SRD) findings first (owned here, stale-checked), then the A-core and the
    // shared F-full class findings (owned by the other two tests)
    let groups: [(&[Finding], bool); 3] = [
        (&tw.findings, true),
        (ACORE_FINDINGS, false),
        (diff_findings::FINDINGS, false),
    ];
    let all: Vec<Finding> = groups.iter().flat_map(|(g, _)| g.iter().copied()).collect();
    let v = verdicts(&pairs, &left, &right, Some(&prefer), &all, "acore-five");
    let labels: Vec<String> = pairs.iter().map(|p| p.label.clone()).collect();
    println!(
        "=== diff_corpus (iii) A-core vs F-full: {} pairs",
        pairs.len()
    );
    let mut fails = Vec::new();
    let mut offset = 0;
    for (i, (g, stale)) in groups.iter().enumerate() {
        let part = Verdicts {
            accepted: if i == 0 {
                v.accepted.clone()
            } else {
                Default::default()
            },
            classified: v
                .classified
                .iter()
                .filter(|(k, _)| (offset..offset + g.len()).contains(*k))
                .map(|(k, n)| (k - offset, *n))
                .collect(),
            unclassified: if i == 0 {
                v.unclassified.clone()
            } else {
                Vec::new()
            },
            build_errors: if i == 0 {
                v.build_errors.clone()
            } else {
                Vec::new()
            },
        };
        fails.extend(part.report(g, &labels, *stale));
        offset += g.len();
    }
    println!("wall {:.1}s", t0.elapsed().as_secs_f64());
    assert!(fails.is_empty(), "{fails:#?}");
}

fn on_big_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(diff::STACK)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("differential run");
}

#[test]
fn acore_srd_overlap_matches_bundled() {
    let tmp = tempfile::tempdir().expect("tmp");
    let Some(data) = acore_data(tmp.path()) else {
        return skip(
            "acore_srd_overlap_matches_bundled",
            "BOREALIS_ACORE_DATA / BOREALIS_LIVE_INDEX_URL not set",
        );
    };
    on_big_stack(move || run_acore(&data));
    drop(tmp);
}

#[test]
fn acore_matches_five_tools() {
    let Some(five) = std::env::var_os("BOREALIS_5ETOOLS_DIR").map(PathBuf::from) else {
        return skip("acore_matches_five_tools", "BOREALIS_5ETOOLS_DIR not set");
    };
    let file = private_dir().join("three_way.json");
    if !file.is_file() {
        return skip(
            "acore_matches_five_tools",
            "private three_way.json not found",
        );
    }
    let tmp = tempfile::tempdir().expect("tmp");
    let Some(data) = acore_data(tmp.path()) else {
        return skip(
            "acore_matches_five_tools",
            "BOREALIS_ACORE_DATA / BOREALIS_LIVE_INDEX_URL not set",
        );
    };
    on_big_stack(move || run_three_way(&data, &five, &file));
    drop(tmp);
}
