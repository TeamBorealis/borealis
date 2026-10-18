//! Differential test over the full 5etools corpus (acceptance step 3d (i)): the PHB / XPHB
//! classes, species and backgrounds that overlap the SRD, imported from a real 5etools `data/`
//! folder (F-full), must build the same characters as the bundled SRD 5.1 / SRD 5.2.1 packs.
//!
//! Local-only: enabled by `BOREALIS_5ETOOLS_DIR` (skipped otherwise; `BOREALIS_ACCEPT_STRICT=1`
//! fails on a skip). Prints ids, keys and numbers only. Run it in release mode:
//!
//! ```text
//! BOREALIS_5ETOOLS_DIR=/path/to/5etools/data cargo test --release -p borealis-import-5etools \
//!     --test diff_corpus -- --nocapture
//! ```
//!
//! The whole folder is imported as the app does (bundled SRD dirs given) into a store with core
//! and both SRD packs (the app after an import); the reference side is built on a store of the
//! bundled packs alone, so its selects offer only SRD options. Pairs: every SRD class with its SRD subclass (on a fixed origin),
//! every SRD race / species (on Fighter / Champion) and every SRD background. Both sides are
//! built and normalised exactly as `diff_bundled.rs` does (`support/diff.rs`) and compared at
//! levels 1, 3, 5, 11 and 20.
//!
//! Accepted differences: `diff_bundled.rs`'s LOSSY list (same entries), plus [`CORPUS`]: what
//! follows from the store holding every book instead of the SRD fixture subset. Nothing else is
//! accepted; every difference is printed with its pair, level and key, and recorded as a gate
//! outcome. The always-on test `lossy_copy_matches_diff_bundled` keeps the LOSSY copy in step.

#[path = "support/diff.rs"]
mod diff;
#[path = "support/diff_findings.rs"]
mod diff_findings;
#[path = "../../../tools/regression-gate/outcome.rs"]
mod outcome;
mod support;

use borealis_content::{ContentStore, ContentStoreBuilder, Ruleset};
use borealis_engine::Engine;
use borealis_import_5etools::{import_folder, ImportOptions};
use diff::{Accepted, Diff, Owner, Pair, Side, Verdicts, LOSSY};
use diff_findings::FINDINGS;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use support::core_pack;

const SUITE: &str = "diff_corpus";

/// Differences that follow from the corpus holding every book (not a Borealis defect).
/// `select.*` keys other than these are accepted only when the imported side offers at least
/// as many options as the bundled one on the same selects ([`diff::dominates`]).
const CORPUS: &[Accepted] = &[(
    "",
    "select.Sub Race",
    &[],
    "the corpus adds other books' subraces to every SRD race, so the imported race opens a Sub \
     Race select with more options (or one the bundled race lacks; the imported side then picks \
     the race's own unnamed PHB subrace)",
)];

/// SRD classes: name, bundled class, bundled subclass, imported subclass name.
type ClassRow = (&'static str, &'static str, &'static str, &'static str);
/// SRD origins: label, bundled race, bundled subrace, imported race name, imported subrace name.
type OriginRow = (
    &'static str,
    &'static str,
    Option<&'static str>,
    &'static str,
    Option<&'static str>,
);

const CLASSES_2014: [ClassRow; 12] = [
    (
        "Barbarian",
        "ID_WOTC_PHB_CLASS_BARBARIAN",
        "ID_WOTC_PHB_ARCHETYPE_PATH_OF_THE_BERSERKER",
        "Path of the Berserker",
    ),
    (
        "Bard",
        "ID_WOTC_PHB_CLASS_BARD",
        "ID_WOTC_PHB_ARCHETYPE_COLLEGE_OF_LORE",
        "College of Lore",
    ),
    (
        "Cleric",
        "ID_WOTC_PHB_CLASS_CLERIC",
        "ID_WOTC_PHB_ARCHETYPE_LIFEDOMAIN",
        "Life Domain",
    ),
    (
        "Druid",
        "ID_WOTC_PHB_CLASS_DRUID",
        "ID_WOTC_PHB_ARCHETYPE_CIRCLEOFTHELAND",
        "Circle of the Land",
    ),
    (
        "Fighter",
        "ID_WOTC_PHB_CLASS_FIGHTER",
        "ID_WOTC_PHB_ARCHETYPE_CHAMPION",
        "Champion",
    ),
    (
        "Monk",
        "ID_WOTC_PHB_CLASS_MONK",
        "ID_WOTC_PHB_ARCHETYPE_WAY_OF_THE_OPEN_HAND",
        "Way of the Open Hand",
    ),
    (
        "Paladin",
        "ID_WOTC_PHB_CLASS_PALADIN",
        "ID_WOTC_PHB_ARCHETYPE_PALADIN_OATH_OF_DEVOTION",
        "Oath of Devotion",
    ),
    (
        "Ranger",
        "ID_WOTC_PHB_CLASS_RANGER",
        "ID_WOTC_ARCHETYPE_RANGER_HUNTER",
        "Hunter",
    ),
    (
        "Rogue",
        "ID_WOTC_PHB_CLASS_ROGUE",
        "ID_WOTC_PHB_ARCHETYPE_THIEF",
        "Thief",
    ),
    (
        "Sorcerer",
        "ID_WOTC_PHB_CLASS_SORCERER",
        "ID_WOTC_PHB_ARCHETYPE_DRACONIC_BLOODLINE",
        "Draconic Bloodline",
    ),
    (
        "Warlock",
        "ID_WOTC_PHB_CLASS_WARLOCK",
        "ID_WOTC_PHB_ARCHETYPE_OTHERWORLDLY_PATRON_FIEND",
        "The Fiend",
    ),
    (
        "Wizard",
        "ID_WOTC_PHB_CLASS_WIZARD",
        "ID_WOTC_PHB_ARCHETYPE_WIZARD_SCHOOL_OF_EVOCATION",
        "School of Evocation",
    ),
];

const CLASSES_2024: [ClassRow; 12] = [
    (
        "Barbarian",
        "ID_2024_CLASS_BARBARIAN",
        "ID_2024_SUBCLASS_BARBARIAN_PATH_OF_THE_BERSERKER",
        "Path of the Berserker",
    ),
    (
        "Bard",
        "ID_2024_CLASS_BARD",
        "ID_2024_SUBCLASS_BARD_COLLEGE_OF_LORE",
        "College of Lore",
    ),
    (
        "Cleric",
        "ID_2024_CLASS_CLERIC",
        "ID_2024_SUBCLASS_CLERIC_LIFE_DOMAIN",
        "Life Domain",
    ),
    (
        "Druid",
        "ID_2024_CLASS_DRUID",
        "ID_2024_SUBCLASS_DRUID_CIRCLE_OF_THE_LAND",
        "Circle of the Land",
    ),
    (
        "Fighter",
        "ID_2024_CLASS_FIGHTER",
        "ID_2024_SUBCLASS_FIGHTER_CHAMPION",
        "Champion",
    ),
    (
        "Monk",
        "ID_2024_CLASS_MONK",
        "ID_2024_SUBCLASS_MONK_WARRIOR_OF_THE_OPEN_HAND",
        "Warrior of the Open Hand",
    ),
    (
        "Paladin",
        "ID_2024_CLASS_PALADIN",
        "ID_2024_SUBCLASS_PALADIN_OATH_OF_DEVOTION",
        "Oath of Devotion",
    ),
    (
        "Ranger",
        "ID_2024_CLASS_RANGER",
        "ID_2024_SUBCLASS_RANGER_HUNTER",
        "Hunter",
    ),
    (
        "Rogue",
        "ID_2024_CLASS_ROGUE",
        "ID_2024_SUBCLASS_ROGUE_THIEF",
        "Thief",
    ),
    (
        "Sorcerer",
        "ID_2024_CLASS_SORCERER",
        "ID_2024_SUBCLASS_SORCERER_DRACONIC_SORCERY",
        "Draconic Sorcery",
    ),
    (
        "Warlock",
        "ID_2024_CLASS_WARLOCK",
        "ID_2024_SUBCLASS_WARLOCK_FIEND_PATRON",
        "Fiend Patron",
    ),
    (
        "Wizard",
        "ID_2024_CLASS_WIZARD",
        "ID_2024_SUBCLASS_WIZARD_EVOKER",
        "Evoker",
    ),
];

const ORIGINS_2014: [OriginRow; 9] = [
    (
        "Hill Dwarf",
        "ID_SRD_RACE_DWARF",
        Some("ID_SUB_RACE_HILL_DWARF"),
        "Dwarf",
        Some("Dwarf (Hill)"),
    ),
    (
        "High Elf",
        "ID_RACE_ELF",
        Some("ID_SUB_RACE_HIGH_ELF"),
        "Elf",
        Some("Elf (High)"),
    ),
    (
        "Lightfoot Halfling",
        "ID_RACE_HALFLING",
        Some("ID_SUB_RACE_LIGHTFOOT_HALFLING"),
        "Halfling",
        Some("Halfling (Lightfoot)"),
    ),
    (
        "Human",
        "ID_RACE_HUMAN",
        None,
        "Human",
        Some("Human (Standard)"),
    ),
    (
        "Dragonborn",
        "ID_RACE_DRAGONBORN",
        None,
        "Dragonborn",
        Some("Dragonborn (Standard)"),
    ),
    (
        "Rock Gnome",
        "ID_RACE_GNOME",
        Some("ID_SUB_RACE_ROCK_GNOME"),
        "Gnome",
        Some("Gnome (Rock)"),
    ),
    (
        "Half-Elf",
        "ID_RACE_HALFELF",
        None,
        "Half-Elf",
        Some("Half-Elf (Standard)"),
    ),
    (
        "Half-Orc",
        "ID_RACE_HALFORC",
        None,
        "Half-Orc",
        Some("Half-Orc (Standard)"),
    ),
    (
        "Tiefling",
        "ID_RACE_TIEFLING",
        None,
        "Tiefling",
        Some("Tiefling (Standard)"),
    ),
];

const ORIGINS_2024: [OriginRow; 9] = [
    ("Dwarf", "ID_2024_SPECIES_DWARF", None, "Dwarf", None),
    (
        "Dragonborn",
        "ID_2024_SPECIES_DRAGONBORN",
        None,
        "Dragonborn",
        None,
    ),
    ("Elf", "ID_2024_SPECIES_ELF", None, "Elf", None),
    ("Gnome", "ID_2024_SPECIES_GNOME", None, "Gnome", None),
    ("Goliath", "ID_2024_SPECIES_GOLIATH", None, "Goliath", None),
    (
        "Halfling",
        "ID_2024_SPECIES_HALFLING",
        None,
        "Halfling",
        None,
    ),
    ("Human", "ID_2024_SPECIES_HUMAN", None, "Human", None),
    ("Orc", "ID_2024_SPECIES_ORC", None, "Orc", None),
    (
        "Tiefling",
        "ID_2024_SPECIES_TIEFLING",
        None,
        "Tiefling",
        None,
    ),
];

const BACKGROUNDS_2014: [(&str, &str); 1] = [("Acolyte", "ID_BACKGROUND_ACOLYTE")];
const BACKGROUNDS_2024: [(&str, &str); 4] = [
    ("Acolyte", "ID_2024_BACKGROUND_ACOLYTE"),
    ("Criminal", "ID_2024_BACKGROUND_CRIMINAL"),
    ("Sage", "ID_2024_BACKGROUND_SAGE"),
    ("Soldier", "ID_2024_BACKGROUND_SOLDIER"),
];

/// One edition's table.
struct Edition {
    ruleset: Ruleset,
    book: &'static str,
    race_kind: (&'static str, &'static str),
    classes: &'static [ClassRow],
    origins: &'static [OriginRow],
    backgrounds: &'static [(&'static str, &'static str)],
}

const EDITIONS: [Edition; 2] = [
    Edition {
        ruleset: Ruleset::Dnd2014,
        book: "PHB",
        race_kind: ("Race", "RACE"),
        classes: &CLASSES_2014,
        origins: &ORIGINS_2014,
        backgrounds: &BACKGROUNDS_2014,
    },
    Edition {
        ruleset: Ruleset::Dnd2024,
        book: "XPHB",
        race_kind: ("Species", "SPECIES"),
        classes: &CLASSES_2024,
        origins: &ORIGINS_2024,
        backgrounds: &BACKGROUNDS_2024,
    },
];

fn content_dir() -> PathBuf {
    core_pack().join("..")
}

fn bundled_builder() -> ContentStoreBuilder {
    let content = content_dir();
    let mut builder = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        builder = builder
            .add_pack_dir(&content.join(pack))
            .unwrap_or_else(|e| panic!("bundled pack {pack}: {e}"));
    }
    builder
}

/// The bundled packs alone (the reference side: what the SRD offers) and the bundled packs
/// plus the imported folder (the app after `import_5etools`). Separate stores, so that the
/// reference side's selects do not also offer imported options.
fn stores(data: &Path, tmp: &Path) -> (ContentStore, ContentStore) {
    let content = content_dir();
    let options = ImportOptions {
        srd_dirs: vec![content.join("srd-5.1"), content.join("srd-5.2.1")],
        ..ImportOptions::default()
    };
    let result = import_folder(data, &options).expect("the 5etools folder imports");
    let mut builder = bundled_builder();
    for pack in &result.packs {
        let dir = pack.write_to(tmp).expect("write imported pack");
        builder = builder.add_pack_dir(&dir).expect("imported pack loads");
    }
    (bundled_builder().build(), builder.build())
}

/// The imported subclass of `class` named `name`.
fn subclass_of(
    store: &ContentStore,
    class: &str,
    name: &str,
    book: &str,
) -> Result<String, String> {
    let prefix = format!("ID_5ET_{book}_SUBCLASS_");
    let hits: Vec<String> = store
        .iter()
        .map(|se| &se.element)
        .filter(|e| {
            e.ty == "Archetype"
                && e.name == name
                && e.id.starts_with(&prefix)
                && e.supports.iter().any(|s| s == class)
        })
        .map(|e| e.id.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    match hits.as_slice() {
        [one] => Ok(one.clone()),
        _ => Err(format!("{} subclasses {name:?} of {class}", hits.len())),
    }
}

/// The imported side of every bundled row of one edition.
struct Resolved {
    classes: Vec<(&'static str, Side)>,
    origins: Vec<(&'static str, Side)>,
    backgrounds: Vec<(&'static str, Side)>,
}

/// Bundled and imported sides of the rows; `Err` lists the rows whose imported ids are missing.
fn resolve(store: &ContentStore, ed: &Edition) -> Result<(Resolved, Resolved), Vec<String>> {
    let mut errors = Vec::new();
    let mut ok = |r: Result<String, String>| r.map_err(|e| errors.push(e)).unwrap_or_default();
    let book = ed.book;
    let (race_ty, race_token) = ed.race_kind;
    let five_race = |name: &str| {
        diff::by_name(
            store,
            race_ty,
            name,
            &format!("ID_5ET_{book}_{race_token}_"),
        )
    };
    let five_sub =
        |name: &str| diff::by_name(store, "Sub Race", name, &format!("ID_5ET_{book}_SUBRACE_"));
    let five_bg = |name: &str| {
        diff::by_name(
            store,
            "Background",
            name,
            &format!("ID_5ET_{book}_BACKGROUND_"),
        )
    };
    let five_class =
        |name: &str| diff::by_name(store, "Class", name, &format!("ID_5ET_{book}_CLASS_"));

    let (o_label, br, bsr, ir, isr) = ed.origins[0];
    let (bg_label, bbg) = ed.backgrounds[0];
    let i_race = ok(five_race(ir));
    let i_sub = isr.map(|n| ok(five_sub(n)));
    let i_bg = ok(five_bg(bg_label));
    let fighter = ed
        .classes
        .iter()
        .find(|c| c.0 == "Fighter")
        .expect("fighter row");
    let i_fighter = ok(five_class("Fighter"));
    let i_champion = ok(subclass_of(store, &i_fighter, fighter.3, book));
    let side = |race: &str, sub: Option<&str>, bg: &str, class: &str, subclass: &str| Side {
        race: race.into(),
        subrace: sub.map(str::to_string),
        background: bg.into(),
        class: class.into(),
        subclass: subclass.into(),
    };
    let mut bundled = Resolved {
        classes: vec![],
        origins: vec![],
        backgrounds: vec![],
    };
    let mut imported = Resolved {
        classes: vec![],
        origins: vec![],
        backgrounds: vec![],
    };
    let _ = o_label;
    for (name, bc, bs, isub) in ed.classes {
        let ic = ok(five_class(name));
        let is = ok(subclass_of(store, &ic, isub, book));
        bundled.classes.push((name, side(br, bsr, bbg, bc, bs)));
        imported
            .classes
            .push((name, side(&i_race, i_sub.as_deref(), &i_bg, &ic, &is)));
    }
    for (label, r, sr, name, sub) in ed.origins {
        let ir = ok(five_race(name));
        let isr = sub.map(|n| ok(five_sub(n)));
        bundled
            .origins
            .push((label, side(r, *sr, bbg, fighter.1, fighter.2)));
        imported.origins.push((
            label,
            side(&ir, isr.as_deref(), &i_bg, &i_fighter, &i_champion),
        ));
    }
    for (label, b) in ed.backgrounds {
        let ib = ok(five_bg(label));
        bundled
            .backgrounds
            .push((label, side(br, bsr, b, fighter.1, fighter.2)));
        imported.backgrounds.push((
            label,
            side(&i_race, i_sub.as_deref(), &ib, &i_fighter, &i_champion),
        ));
    }
    if errors.is_empty() {
        Ok((bundled, imported))
    } else {
        Err(errors)
    }
}

/// Pairs of one edition (class rows on the first origin and background, origin and background
/// rows on Fighter / Champion), each label once.
fn pairs(ed: &Edition, b: &Resolved, i: &Resolved) -> Vec<Pair> {
    let rs = ed.ruleset;
    let (origin0, bg0) = (ed.origins[0].0, ed.backgrounds[0].0);
    let fighter = "Fighter/Champion";
    let mut out: Vec<Pair> = Vec::new();
    let mut push = |label: String, left: &Side, right: &Side| {
        if !out.iter().any(|p| p.label == label) {
            out.push(Pair {
                label,
                ruleset: rs,
                left: left.clone(),
                right: right.clone(),
            });
        }
    };
    for ((name, l), (_, r)) in b.classes.iter().zip(&i.classes) {
        let sub = ed.classes.iter().find(|c| c.0 == *name).map_or("", |c| c.3);
        push(format!("{rs:?} {name}/{sub} {origin0} {bg0}"), l, r);
    }
    for ((label, l), (_, r)) in b.origins.iter().zip(&i.origins) {
        push(format!("{rs:?} {fighter} {label} {bg0}"), l, r);
    }
    for ((label, l), (_, r)) in b.backgrounds.iter().zip(&i.backgrounds) {
        push(format!("{rs:?} {fighter} {origin0} {label}"), l, r);
    }
    out
}

/// Why a difference is accepted (`None`: it is a finding).
fn accept(d: &Diff) -> Option<String> {
    if let Some(i) = diff::covered(LOSSY, d) {
        return Some(format!("LOSSY {}", LOSSY[i].1));
    }
    if let Some(i) = diff::covered(CORPUS, d) {
        return Some(format!("CORPUS {}", CORPUS[i].1));
    }
    (d.key.starts_with("select.") && diff::dominates(&d.left, &d.right))
        .then(|| "CORPUS select superset".to_string())
}

fn run(data: &Path) {
    let t0 = std::time::Instant::now();
    let tmp = tempfile::tempdir().expect("tmp");
    let (bundled, store) = stores(data, tmp.path());
    let (left, engine) = (Engine::new(&bundled), Engine::new(&store));
    let prefer: BTreeMap<Ruleset, BTreeSet<String>> = [Ruleset::Dnd2014, Ruleset::Dnd2024]
        .into_iter()
        .map(|rs| (rs, diff::names(&bundled, &diff::edition_packs(rs))))
        .collect();
    assert!(
        prefer.values().all(|n| n.len() > 100),
        "bundled names not found by pack"
    );
    println!(
        "=== diff_corpus (F-full vs bundled SRD): store built in {:.1}s",
        t0.elapsed().as_secs_f64()
    );
    let mut all = Vec::new();
    let mut missing = Vec::new();
    for ed in &EDITIONS {
        match resolve(&store, ed) {
            Ok((b, i)) => all.extend(pairs(ed, &b, &i)),
            Err(e) => missing.extend(e),
        }
    }
    assert!(
        missing.is_empty(),
        "imported SRD-overlap elements not found: {missing:#?}"
    );
    if let Ok(probe) = std::env::var("BOREALIS_DIFF_PROBE") {
        let level: u32 = std::env::var("BOREALIS_DIFF_PROBE_LEVEL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        for p in all
            .iter()
            .filter(|p| probe.split('|').any(|x| p.label.contains(x)))
        {
            for (side, s) in [("left", &p.left), ("right", &p.right)] {
                println!("--- probe {} {side} L{level}", p.label);
                let e = if side == "left" { &left } else { &engine };
                for line in diff::probe(e, p.ruleset, s, level, prefer.get(&p.ruleset)) {
                    println!("  {line}");
                }
            }
        }
        return;
    }
    let results = diff::par_map(&all, diff::threads(), &|p: &Pair| {
        diff::differences(&left, &engine, p, prefer.get(&p.ruleset))
    });
    let mut v = Verdicts::default();
    for (p, r) in all.iter().zip(results) {
        let mut bad = 0;
        match r {
            Err(e) => {
                v.build_errors.push(format!("{}: {e}", p.label));
                bad += 1;
            }
            Ok(diffs) => {
                for d in diffs {
                    let id = format!("{}/L{}/{}", d.label, d.level, d.key);
                    if let Some(why) = accept(&d) {
                        outcome::record(SUITE, &id, true, &why);
                        *v.accepted.entry(why).or_default() += 1;
                    } else if let Some(i) = diff::classify(FINDINGS, &d) {
                        let (fid, .., owner, _) = FINDINGS[i];
                        if std::env::var_os("BOREALIS_DIFF_ALL").is_some() {
                            println!("  {fid}: {d}");
                        }
                        outcome::record(
                            SUITE,
                            &id,
                            owner == Owner::Source,
                            &format!("{fid} {owner:?}"),
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
        outcome::record(SUITE, &p.label, bad == 0, &bad.to_string());
    }
    let labels: Vec<String> = all.iter().map(|p| p.label.clone()).collect();
    println!("pairs: {}", all.len());
    let fails = v.report(FINDINGS, &labels, true);
    println!("wall {:.1}s", t0.elapsed().as_secs_f64());
    assert!(fails.is_empty(), "{fails:#?}");
}

#[test]
fn corpus_srd_overlap_matches_bundled() {
    let Ok(dir) = std::env::var("BOREALIS_5ETOOLS_DIR") else {
        assert!(
            std::env::var_os("BOREALIS_ACCEPT_STRICT").is_none(),
            "BOREALIS_5ETOOLS_DIR not set (strict mode)"
        );
        return eprintln!("diff_corpus: BOREALIS_5ETOOLS_DIR not set, skipping");
    };
    std::thread::Builder::new()
        .stack_size(diff::STACK)
        .spawn(move || run(Path::new(&dir)))
        .expect("spawn")
        .join()
        .expect("differential run");
}

#[test]
fn lossy_copy_matches_diff_bundled() {
    let src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/diff_bundled.rs"),
    )
    .expect("diff_bundled.rs");
    let problems = diff::lossy_matches_diff_bundled(&src);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn select_superset_reads_option_counts() {
    assert!(diff::dominates("[1, 6]", "[2, 6]"));
    assert!(!diff::dominates("[6]", "[1]"));
    assert!(!diff::dominates("-", "[3]"));
    assert!(diff::dominates("-", "-"));
}
