//! Differential test: characters built from the imported 5etools SRD fixtures
//! (`tests/fixtures/data`) must come out like the same characters built from the bundled
//! SRD 5.1 / SRD 5.2.1 packs.
//!
//! For every matched pair (class + subclass, race/species (+ subrace), background, both
//! editions) one character is built from bundled ids and one from imported ids, in one
//! store holding core, both SRD packs and the imported packs (as the app loads them). Both
//! are levelled 1 to 20 with the same deterministic choices (by option *name*, so both
//! sides pick the same thing) and compared at levels 1, 3, 5, 11 and 20 on normalised
//! outputs: ability scores and saves, skills, HP, AC, initiative, speed, spell slots and
//! pact slots, prepare limits, spell choice slots, feat / ability-increase slots and the
//! option counts of every other select.
//!
//! Differences are accepted only when listed: [`LOSSY`] for differences that are inherent (data
//! interpretation, fixture subset, store composition, modelling), [`KNOWN_GAPS`] for open
//! importer / content bugs (to be removed when fixed). Every listed entry must still be observed,
//! so a fixed gap fails until its entry is removed. Nothing is skipped: missing fixtures or packs
//! fail the test.

mod support;

use borealis_content::{ContentStore, ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::SlotSnapshot;
use borealis_engine::state::SlotKey;
use borealis_engine::{CharacterSnapshot, CharacterState, Choice, Engine};
use borealis_import_5etools::{import_folder, ImportOptions};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use support::{core_pack, fixtures};

/// Base scores STR..CHA (odd values so that +1 increases change modifiers).
const SCORES: [i32; 6] = [15, 13, 14, 12, 10, 8];
const CHECK_LEVELS: [u32; 5] = [1, 3, 5, 11, 20];
const FILL_PASSES: usize = 120;
/// Stack of the test threads (deep settles on the full packs).
const STACK: usize = 64 << 20;
/// Slot types filled only with the element the pair names.
const NAMED_TYPES: [&str; 5] = ["Race", "Sub Race", "Class", "Archetype", "Background"];

/// An accepted difference: pair label substring, normalised key prefix, check levels (empty: all)
/// and the reason.
type Accepted = (&'static str, &'static str, &'static [u32], &'static str);

/// Inherent differences between an import of the fixtures and the bundled packs.
const LOSSY: &[Accepted] = &[
    (
        "Dnd2014",
        "select.Language",
        &[],
        "5etools encodes the SRD 5.1 Acolyte's 'two of your choice' as anyStandard (bundled: \
         Standard||Exotic); the High Elf extra-language count follows from the Acolyte picks",
    ),
    (
        "Dnd2014 Fighter",
        "select.Class Feature",
        &[],
        "the fixtures hold 1 SRD-flagged 2014 Fighting Style optional feature, the bundled \
         pack 6: Fighting Style option counts differ",
    ),
];

/// Open bugs found by this test (see specs/handoffs/diff-5etools-bundled-1.md and
/// specs/handoffs/import-gaps-2.md).
const KNOWN_GAPS: &[Accepted] = &[];

/// One side of a pair: the elements a character is built from.
#[derive(Clone, Copy)]
struct Side {
    race: &'static str,
    subrace: Option<&'static str>,
    background: &'static str,
    class: &'static str,
    subclass: &'static str,
}

impl Side {
    fn wanted(&self) -> Vec<&'static str> {
        let mut v = vec![self.race, self.background, self.class, self.subclass];
        v.extend(self.subrace);
        v
    }
}

struct Pair {
    label: String,
    ruleset: Ruleset,
    bundled: Side,
    imported: Side,
}

type Origin = (
    &'static str,
    (&'static str, Option<&'static str>),
    (&'static str, Option<&'static str>),
);
type Class = (
    &'static str,
    (&'static str, &'static str),
    (&'static str, &'static str),
);

/// 2014: SRD 5.1 bundled vs 5etools PHB.
const ORIGINS_2014: [Origin; 2] = [
    (
        "Hill Dwarf",
        ("ID_SRD_RACE_DWARF", Some("ID_SUB_RACE_HILL_DWARF")),
        (
            "ID_5ET_PHB_RACE_DWARF",
            Some("ID_5ET_PHB_SUBRACE_DWARF_HILL"),
        ),
    ),
    (
        "High Elf",
        ("ID_RACE_ELF", Some("ID_SUB_RACE_HIGH_ELF")),
        ("ID_5ET_PHB_RACE_ELF", Some("ID_5ET_PHB_SUBRACE_ELF_HIGH")),
    ),
];
const CLASSES_2014: [Class; 2] = [
    (
        "Fighter/Champion",
        (
            "ID_WOTC_PHB_CLASS_FIGHTER",
            "ID_WOTC_PHB_ARCHETYPE_CHAMPION",
        ),
        (
            "ID_5ET_PHB_CLASS_FIGHTER",
            "ID_5ET_PHB_SUBCLASS_FIGHTER_CHAMPION",
        ),
    ),
    (
        "Wizard/Evocation",
        (
            "ID_WOTC_PHB_CLASS_WIZARD",
            "ID_WOTC_PHB_ARCHETYPE_WIZARD_SCHOOL_OF_EVOCATION",
        ),
        (
            "ID_5ET_PHB_CLASS_WIZARD",
            "ID_5ET_PHB_SUBCLASS_WIZARD_EVOCATION",
        ),
    ),
];
const BACKGROUNDS_2014: [(&str, &str, &str); 1] = [(
    "Acolyte",
    "ID_BACKGROUND_ACOLYTE",
    "ID_5ET_PHB_BACKGROUND_ACOLYTE",
)];

/// 2024: SRD 5.2.1 bundled vs 5etools XPHB.
const ORIGINS_2024: [Origin; 2] = [
    (
        "Dwarf",
        ("ID_2024_SPECIES_DWARF", None),
        ("ID_5ET_XPHB_SPECIES_DWARF", None),
    ),
    (
        "Human",
        ("ID_2024_SPECIES_HUMAN", None),
        ("ID_5ET_XPHB_SPECIES_HUMAN", None),
    ),
];
const CLASSES_2024: [Class; 2] = [
    (
        "Fighter/Champion",
        ("ID_2024_CLASS_FIGHTER", "ID_2024_SUBCLASS_FIGHTER_CHAMPION"),
        (
            "ID_5ET_XPHB_CLASS_FIGHTER",
            "ID_5ET_XPHB_SUBCLASS_FIGHTER_CHAMPION",
        ),
    ),
    (
        "Wizard/Evoker",
        ("ID_2024_CLASS_WIZARD", "ID_2024_SUBCLASS_WIZARD_EVOKER"),
        (
            "ID_5ET_XPHB_CLASS_WIZARD",
            "ID_5ET_XPHB_SUBCLASS_WIZARD_EVOKER",
        ),
    ),
];
const BACKGROUNDS_2024: [(&str, &str, &str); 1] = [(
    "Acolyte",
    "ID_2024_BACKGROUND_ACOLYTE",
    "ID_5ET_XPHB_BACKGROUND_ACOLYTE",
)];

fn pairs(
    ruleset: Ruleset,
    origins: &[Origin],
    classes: &[Class],
    backgrounds: &[(&'static str, &'static str, &'static str)],
) -> Vec<Pair> {
    let mut out = Vec::new();
    for (cname, (bc, bs), (ic, is)) in classes {
        for (oname, (br, bsr), (ir, isr)) in origins {
            for (bgname, bb, ib) in backgrounds {
                out.push(Pair {
                    label: format!("{ruleset:?} {cname} {oname} {bgname}"),
                    ruleset,
                    bundled: Side {
                        race: br,
                        subrace: *bsr,
                        background: bb,
                        class: bc,
                        subclass: bs,
                    },
                    imported: Side {
                        race: ir,
                        subrace: *isr,
                        background: ib,
                        class: ic,
                        subclass: is,
                    },
                });
            }
        }
    }
    out
}

fn content_dir() -> PathBuf {
    core_pack().join("..")
}

/// Core + both SRD packs + the imported fixture packs, as the app loads them.
fn store(tmp: &std::path::Path) -> ContentStore {
    let content = content_dir();
    let options = ImportOptions {
        srd_dirs: vec![content.join("srd-5.1"), content.join("srd-5.2.1")],
        ..ImportOptions::default()
    };
    let result = import_folder(&fixtures(), &options).expect("the 5etools SRD fixtures import");
    assert_eq!(result.packs.len(), 4, "fixture packs missing");
    let mut builder = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        builder = builder
            .add_pack_dir(&content.join(pack))
            .unwrap_or_else(|e| panic!("bundled pack {pack}: {e}"));
    }
    for pack in &result.packs {
        let dir = pack.write_to(tmp).expect("write imported pack");
        builder = builder.add_pack_dir(&dir).expect("imported pack loads");
    }
    builder.build()
}

/// The choice for an open slot: the named element when offered, otherwise the option (or
/// list item) with the smallest name. Slots of the [`NAMED_TYPES`] take only named elements.
fn choice_for(slot: &SlotSnapshot, wanted: &[&str]) -> Option<Choice> {
    if let Some(o) = slot
        .options
        .iter()
        .find(|o| wanted.contains(&o.id.as_str()))
    {
        return Some(Choice::Element(o.id.clone()));
    }
    if NAMED_TYPES.contains(&slot.ty.as_str()) {
        return None;
    }
    if let Some(o) = slot
        .options
        .iter()
        .min_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)))
    {
        return Some(Choice::Element(o.id.clone()));
    }
    slot.list_items
        .iter()
        .min_by(|a, b| a.1.cmp(&b.1))
        .map(|(id, _)| Choice::ListItem(*id))
}

/// Fill the open slots: every pass applies the choices of one snapshot in slot order; a slot
/// whose choice is refused in a pass that changed nothing is given up.
fn fill(e: &Engine, mut s: CharacterState, wanted: &[&str]) -> CharacterState {
    let mut refused: HashSet<SlotKey> = HashSet::new();
    for _ in 0..FILL_PASSES {
        let snap = e.derive(&s);
        let open: Vec<(SlotKey, Choice)> = snap
            .slots
            .iter()
            .filter(|x| x.chosen.is_none() && x.list_choice.is_none() && !refused.contains(&x.key))
            .filter_map(|x| choice_for(x, wanted).map(|c| (x.key.clone(), c)))
            .collect();
        if open.is_empty() {
            return s;
        }
        let mut changed = false;
        let mut failed = Vec::new();
        for (key, choice) in open {
            match commands::select(e, &s, &key, choice) {
                Ok(o) => {
                    s = o.state;
                    changed = true;
                }
                Err(_) => failed.push(key),
            }
        }
        if !changed {
            refused.extend(failed);
        }
    }
    panic!("fill did not settle in {FILL_PASSES} passes");
}

/// Normalised, id-free outputs of a snapshot.
fn normalise(snap: &CharacterSnapshot) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    m.insert("level".into(), snap.level.to_string());
    m.insert("proficiency".into(), snap.proficiency_bonus.to_string());
    m.insert("hp".into(), snap.hit_points.to_string());
    m.insert("ac".into(), snap.armor_class.to_string());
    m.insert("initiative".into(), snap.initiative.to_string());
    m.insert("speed".into(), format!("{:?}", snap.speed));
    for a in &snap.abilities {
        m.insert(format!("ability.{}", a.name), a.score.to_string());
        m.insert(
            format!("save.{}", a.name),
            format!("{} proficient={}", a.save, a.save_proficient),
        );
    }
    for s in &snap.skills {
        m.insert(
            format!("skill.{}", s.name),
            format!(
                "{} proficient={} expertise={}",
                s.total, s.proficient, s.expertise
            ),
        );
    }
    // spellcasting blocks and selects are named differently by the two sides: compare them
    // as multisets (sorted), selects keyed by slot type
    // a block without slots, pact slots or a prepare limit only names the ability of granted
    // spells (racial / feat spells): the importer and the packs model those differently
    let mut casting: Vec<String> = snap
        .spellcasting
        .iter()
        .filter(|c| c.slots != [0; 9] || c.pact.is_some() || c.prepare_limit.is_some())
        .map(|c| {
            format!(
                "{} slots={:?} pact={:?} prepare_limit={:?}",
                c.ability, c.slots, c.pact, c.prepare_limit
            )
        })
        .collect();
    casting.sort();
    m.insert("casting".into(), format!("{casting:?}"));
    let mut spell_choices: BTreeMap<String, usize> = BTreeMap::new();
    let mut selects: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for s in &snap.slots {
        if s.ty == "Spell" {
            let by = s.spellcasting.clone().unwrap_or_default();
            *spell_choices.entry(by).or_default() += 1;
        } else {
            let n = if s.list_items.is_empty() {
                s.options.len()
            } else {
                s.list_items.len()
            };
            // a one-of-N choice is a `List` select in the packs and an element select of
            // generated choices (`Grants`) in imports (Magic Initiate spellcasting ability)
            let ty = if s.ty == "Grants" {
                "List"
            } else {
                s.ty.as_str()
            };
            selects.entry(ty.to_string()).or_default().push(n);
        }
    }
    let mut per_block: Vec<usize> = spell_choices.into_values().collect();
    per_block.sort_unstable();
    m.insert("spell_choices".into(), format!("{per_block:?}"));
    for (ty, mut v) in selects {
        v.sort_unstable();
        m.insert(format!("select.{ty}"), format!("{v:?}"));
    }
    m
}

fn build(e: &Engine, rs: Ruleset, side: &Side) -> Vec<(u32, BTreeMap<String, String>)> {
    let wanted = side.wanted();
    let s = commands::new_character(e, rs);
    let mut s = commands::set_base_scores(e, &s, SCORES)
        .expect("scores")
        .state;
    for (name, id) in [
        ("Race", side.race),
        ("Background", side.background),
        ("Class", side.class),
    ] {
        let key =
            slot_key(&e.derive(&s), "ID_LEVEL_1", name, 1).unwrap_or_else(|| panic!("{name} slot"));
        s = commands::select(e, &s, &key, Choice::Element(id.into()))
            .unwrap_or_else(|err| panic!("{id}: {err}"))
            .state;
    }
    let mut out = Vec::new();
    for level in 1..=20 {
        if level > 1 {
            s = commands::level_up_main(e, &s, None)
                .expect("level up")
                .state;
        }
        s = fill(e, s, &wanted);
        if CHECK_LEVELS.contains(&level) {
            let snap = e.derive(&s);
            // every named element is on the character (the subclass from level 3 on)
            for id in wanted
                .iter()
                .filter(|id| level >= 3 || **id != side.subclass)
            {
                assert!(
                    snap.elements.iter().any(|r| r.element.id == *id),
                    "{id} not on the character at level {level}"
                );
            }
            out.push((level, normalise(&snap)));
        }
    }
    out
}

/// Index of the accepted entry that covers a difference, if any (LOSSY first, then KNOWN_GAPS).
fn accepted(label: &str, level: u32, key: &str) -> Option<usize> {
    LOSSY
        .iter()
        .chain(KNOWN_GAPS)
        .position(|(l, k, levels, _)| {
            // a prefix ends at a word: `select.Class` does not cover `select.Class Feature`
            label.contains(l)
                && key
                    .strip_prefix(k)
                    .is_some_and(|rest| !rest.starts_with(' '))
                && (levels.is_empty() || levels.contains(&level))
        })
}

/// Differences of one pair, as readable lines; marks the accepted entries it used.
fn compare(e: &Engine, p: &Pair, used: &mut HashSet<usize>) -> Vec<String> {
    let bundled = build(e, p.ruleset, &p.bundled);
    let imported = build(e, p.ruleset, &p.imported);
    let mut out = Vec::new();
    for ((level, b), (_, i)) in bundled.iter().zip(&imported) {
        let keys: std::collections::BTreeSet<_> = b.keys().chain(i.keys()).collect();
        for k in keys {
            let (bv, iv) = (b.get(k), i.get(k));
            if bv == iv {
                continue;
            }
            if let Some(idx) = accepted(&p.label, *level, k) {
                used.insert(idx);
                continue;
            }
            out.push(format!(
                "{} L{level} {k}: bundled {} / imported {}",
                p.label,
                bv.map_or("-", String::as_str),
                iv.map_or("-", String::as_str)
            ));
        }
    }
    out
}

fn run(ruleset: Ruleset, pairs: &[Pair]) {
    let tmp = tempfile::tempdir().expect("tmp");
    let store = store(tmp.path());
    let engine = Engine::new(&store);
    // one thread per pair (the wizards dominate the runtime)
    let results: Vec<(Vec<String>, HashSet<usize>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = pairs
            .iter()
            .map(|p| {
                let engine = &engine;
                std::thread::Builder::new()
                    .stack_size(STACK)
                    .spawn_scoped(scope, move || {
                        let mut used = HashSet::new();
                        (compare(engine, p, &mut used), used)
                    })
                    .expect("spawn")
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("pair thread"))
            .collect()
    });
    let mut used = HashSet::new();
    let mut problems = Vec::new();
    for (diffs, u) in results {
        problems.extend(diffs);
        used.extend(u);
    }
    for p in problems.iter().take(60) {
        eprintln!("  {p}");
    }
    assert!(
        problems.is_empty(),
        "{ruleset:?}: {} differences between imported and bundled characters",
        problems.len()
    );
    // an entry that applies to this edition but no longer matches anything is stale
    let stale: Vec<&str> = LOSSY
        .iter()
        .chain(KNOWN_GAPS)
        .enumerate()
        .filter(|(i, (l, ..))| !used.contains(i) && pairs.iter().any(|p| p.label.contains(l)))
        .map(|(_, (l, k, _, why))| {
            eprintln!("  stale: {l:?} {k:?}: {why}");
            *k
        })
        .collect();
    assert!(
        stale.is_empty(),
        "{ruleset:?}: stale accepted differences {stale:?}"
    );
}

fn on_big_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("differential run");
}

#[test]
fn imported_srd_51_characters_match_bundled() {
    on_big_stack(|| {
        let p = pairs(
            Ruleset::Dnd2014,
            &ORIGINS_2014,
            &CLASSES_2014,
            &BACKGROUNDS_2014,
        );
        run(Ruleset::Dnd2014, &p);
    });
}

#[test]
fn imported_srd_521_characters_match_bundled() {
    on_big_stack(|| {
        let p = pairs(
            Ruleset::Dnd2024,
            &ORIGINS_2024,
            &CLASSES_2024,
            &BACKGROUNDS_2024,
        );
        run(Ruleset::Dnd2024, &p);
    });
}
