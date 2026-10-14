//! Env-gated targeted checks on the official Aurora repo (`aurorabuilder__elements` under
//! `BOREALIS_CORPUS_DIR`, local only): `[type:spell]` requirement terms and `Companion` selects.
//!
//! * the store carries at most one `InvalidExpression` and the engine no `content.bad_expression`
//!   on the repo's elements;
//! * "can cast a spell" feats (Battlecaster, Dual-Focused, Elemental Adept, its second pick) are
//!   offered to a wizard and not to a fighter;
//! * the Beast Master's Ranger's Companion offers at least 10 beasts (Aurora companions plus the
//!   bundled SRD beasts), and a chosen beast becomes part of the character; no `Companion` select
//!   of the repo is empty.
//!
//! Prints counts and element ids only.

use borealis_content::{ContentStore, ContentStoreBuilder, IssueCode, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterSnapshot, CharacterState, Choice, Engine};
use borealis_migrate::{migrate_path, write_output, MigrateOptions};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const REPO: &str = "aurorabuilder__elements";
const PACK: &str = "dl-aurorabuilder-elements";
const BUNDLED: &[&str] = &[
    "core",
    "srd-5.1",
    "srd-5.2.1",
    "srd-5.1-monsters",
    "srd-5.2.1-monsters",
];
const SCORES: [i32; 6] = [13, 14, 14, 15, 12, 10];
const FILL_PASSES: usize = 60;
const NOT_FILLED: &[&str] = &[
    "Race",
    "Sub Race",
    "Race Variant",
    "Class",
    "Archetype",
    "Feat",
];
const WIZARD: &str = "ID_WOTC_PHB_CLASS_WIZARD";
const FIGHTER: &str = "ID_WOTC_PHB_CLASS_FIGHTER";
const RANGER: &str = "ID_WOTC_PHB_CLASS_RANGER";
const BEAST_MASTER: &str = "ID_WOTC_ARCHETYPE_RANGER_BEAST_MASTER";
const HUMAN: &str = "ID_RACE_HUMAN";
const HUMAN_VARIANT: &str = "ID_RACE_VARIANT_HUMAN_VARIANT";
const CASTER_FEATS: &[&str] = &[
    "ID_KBP_DMBM_FEAT_BATTLECASTER",
    "ID_CRIT_TCS_FEAT_DUAL_FOCUSED",
    "ID_PHB_FEAT_ELEMENTAL_ADEPT",
    "ID_PHB_FEAT_SPELLSNIPER",
];
const ELEMENTAL_ADEPT: &str = "ID_PHB_FEAT_ELEMENTAL_ADEPT";
const ELEMENTAL_ADEPT_2: &str = "ID_PHB_FEAT_ELEMENTAL_ADEPT_2";
const MIN_COMPANIONS: usize = 10;
const PARTNERED: &str = "FreshRoyal__elements-partnered";
const PARTNERED_PACK: &str = "dl-freshroyal-elements-partnered";
/// Grant references left dangling in the partnered repo stacked on aurorabuilder: PHB 2024
/// content outside the SRD, typos and ids of other packs (was 436 before the core pack defined
/// the newer Aurora internal ids and PHB 2024 ids resolved to SRD 5.2.1).
const MAX_PARTNERED_DANGLING_GRANTS: usize = 49;

fn repo_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var("BOREALIS_CORPUS_DIR").ok()?);
    let repos = if dir.join("repos").is_dir() {
        dir.join("repos")
    } else {
        dir
    };
    Some(repos.join(REPO))
}

fn convert(tmp: &Path, repo: &Path, pack: &str) -> PathBuf {
    let opts = MigrateOptions {
        pack_id: Some(pack.into()),
        pack_name: Some(pack.into()),
        origin: PackOrigin::Custom,
        ..MigrateOptions::default()
    };
    let out = migrate_path(repo, &opts).expect("migrate");
    let dir = tmp.join(pack);
    write_output(&dir, &out).expect("write pack");
    dir
}

fn build(packs: &[PathBuf]) -> ContentStore {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut b = ContentStoreBuilder::new();
    for p in BUNDLED {
        b = b.add_pack_dir(&content.join(p)).expect("bundled pack");
    }
    for dir in packs {
        b = b.add_pack_dir(dir).expect("repo pack");
    }
    b.build()
}

fn load(tmp: &Path, repo: &Path) -> ContentStore {
    build(&[convert(tmp, repo, PACK)])
}

struct Host<'a> {
    engine: &'a Engine,
}

impl Host<'_> {
    fn new_char(&self, class: &str) -> CharacterState {
        let e = self.engine;
        let s = commands::new_character(e, Ruleset::Dnd2014);
        let s = commands::set_base_scores(e, &s, SCORES).map_or(s, |o| o.state);
        let key = slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
        commands::select(e, &s, &key, Choice::Element(class.into()))
            .expect("class")
            .state
    }

    fn pick(&self, s: &CharacterState, target: &str) -> Option<CharacterState> {
        let snap = self.engine.derive(s);
        let slot = snap
            .slots
            .iter()
            .find(|x| x.chosen.is_none() && x.options.iter().any(|o| o.id == target))?;
        commands::select(self.engine, s, &slot.key, Choice::Element(target.into()))
            .ok()
            .map(|o| o.state)
    }

    fn fill(&self, mut s: CharacterState) -> CharacterState {
        let mut refused = HashSet::new();
        for _ in 0..FILL_PASSES {
            let snap = self.engine.derive(&s);
            let Some(slot) = snap.slots.iter().find(|x| {
                x.chosen.is_none()
                    && x.list_choice.is_none()
                    && !NOT_FILLED.contains(&x.ty.as_str())
                    && !refused.contains(&x.key)
                    && (!x.options.is_empty() || !x.list_items.is_empty())
            }) else {
                break;
            };
            let choice = match (slot.options.first(), slot.list_items.first()) {
                (Some(o), _) => Choice::Element(o.id.clone()),
                (None, Some((i, _))) => Choice::ListItem(*i),
                (None, None) => break,
            };
            match commands::select(self.engine, &s, &slot.key, choice) {
                Ok(o) => s = o.state,
                Err(_) => {
                    refused.insert(slot.key.clone());
                }
            }
        }
        s
    }

    /// A level-1 variant human of the class, open selects filled (feats left open).
    fn variant_human(&self, class: &str) -> CharacterState {
        let mut s = self.new_char(class);
        for race in [HUMAN, HUMAN_VARIANT] {
            s = self.pick(&s, race).expect("variant human");
        }
        self.fill(s)
    }

    /// Level up without filling (the ASI select stays open for `take_asi_feat`).
    fn level_to(&self, mut s: CharacterState, level: u32) -> CharacterState {
        while self.engine.derive(&s).level < level {
            s = commands::level_up_main(self.engine, &s, None)
                .expect("level up")
                .state;
        }
        s
    }

    /// Ids offered by the open `Feat` selects.
    fn feat_options(&self, s: &CharacterState) -> HashSet<String> {
        self.engine
            .derive(s)
            .slots
            .iter()
            .filter(|x| x.ty == "Feat" && x.chosen.is_none())
            .flat_map(|x| x.options.iter().map(|o| o.id.clone()))
            .collect()
    }

    /// Choose the "Feat" option of the open ASI class feature select.
    fn take_asi_feat(&self, s: &CharacterState) -> CharacterState {
        let snap = self.engine.derive(s);
        let (key, id) = snap
            .slots
            .iter()
            .find_map(|x| {
                let o = x
                    .options
                    .iter()
                    .find(|o| o.name == "Feat" || o.name.starts_with("Feat ("))?;
                (x.chosen.is_none() && x.ty == "Class Feature")
                    .then(|| (x.key.clone(), o.id.clone()))
            })
            .unwrap_or_else(|| {
                let open: Vec<_> = snap
                    .slots
                    .iter()
                    .filter(|x| x.chosen.is_none())
                    .map(|x| {
                        (
                            x.ty.clone(),
                            x.name.clone(),
                            x.options
                                .iter()
                                .take(4)
                                .map(|o| o.name.clone())
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect();
                panic!(
                    "no ASI select with a Feat option at level {}: {open:?}",
                    snap.level
                )
            });
        commands::select(self.engine, s, &key, Choice::Element(id))
            .expect("feat option")
            .state
    }
}

fn store_invalid_expressions(store: &ContentStore, ours: &HashSet<String>) -> Vec<String> {
    store
        .validate()
        .into_iter()
        .chain(store.diagnostics().iter().cloned())
        .filter(|i| i.code == IssueCode::InvalidExpression)
        .filter_map(|i| {
            let el = i.element.filter(|e| ours.contains(e))?;
            Some(format!("{el}: {}", i.message))
        })
        .collect()
}

fn bad_expressions(snap: &CharacterSnapshot, ours: &HashSet<String>) -> Vec<String> {
    snap.diagnostics
        .iter()
        .filter(|d| d.code == "content.bad_expression")
        .filter_map(|d| {
            let el = d.element.as_ref().filter(|e| ours.contains(*e))?;
            Some(format!("{el}: {}", d.message))
        })
        .collect()
}

fn check_caster_feats(h: &Host<'_>, failures: &mut Vec<String>) {
    let wizard = h.variant_human(WIZARD);
    let fighter = h.variant_human(FIGHTER);
    let (w, f) = (h.feat_options(&wizard), h.feat_options(&fighter));
    if f.is_empty() {
        failures.push("the level-1 variant human fighter is offered no feats".into());
    }
    for id in CASTER_FEATS {
        if !w.contains(*id) {
            failures.push(format!("{id} not offered to a level-1 wizard"));
        }
        if f.contains(*id) {
            failures.push(format!("{id} offered to a level-1 fighter"));
        }
    }
    // the second Elemental Adept needs the first one and a spell
    let w4 = h.pick(&wizard, ELEMENTAL_ADEPT).map(|s| h.level_to(s, 4));
    match w4.map(|s| h.feat_options(&h.take_asi_feat(&s))) {
        Some(o) if o.contains(ELEMENTAL_ADEPT_2) => {}
        _ => failures.push(format!(
            "{ELEMENTAL_ADEPT_2} not offered to a level-4 wizard"
        )),
    }
    let f4 = h.take_asi_feat(&h.level_to(fighter, 4));
    if h.feat_options(&f4).contains(ELEMENTAL_ADEPT_2) {
        failures.push(format!("{ELEMENTAL_ADEPT_2} offered to a level-4 fighter"));
    }
    println!(
        "caster feats: wizard offered {}, fighter offered {}",
        CASTER_FEATS.iter().filter(|id| w.contains(**id)).count(),
        CASTER_FEATS.iter().filter(|id| f.contains(**id)).count()
    );
}

fn check_rangers_companion(h: &Host<'_>, ours: &HashSet<String>, failures: &mut Vec<String>) {
    let mut s = h.new_char(RANGER);
    s = h.pick(&s, HUMAN).expect("human");
    s = h.level_to(h.fill(s), 3);
    let Some(s) = h.pick(&s, BEAST_MASTER) else {
        failures.push("Beast Master not offered at ranger 3".into());
        return;
    };
    let snap = h.engine.derive(&s);
    let Some(slot) = snap.slots.iter().find(|x| x.ty == "Companion") else {
        failures.push("no Companion select on a level-3 Beast Master".into());
        return;
    };
    let n = slot.options.len();
    let bundled = slot
        .options
        .iter()
        .filter(|o| o.id.starts_with("ID_SRD"))
        .count();
    println!(
        "Ranger's Companion: {n} options ({bundled} bundled SRD), e.g. {:?}",
        slot.options
            .iter()
            .take(5)
            .map(|o| &o.id)
            .collect::<Vec<_>>()
    );
    if n < MIN_COMPANIONS {
        failures.push(format!("Ranger's Companion offers {n} options"));
    }
    for empty in snap
        .slots
        .iter()
        .filter(|x| x.ty == "Companion" && ours.contains(&x.owner.id) && x.options.is_empty())
    {
        failures.push(format!("Companion select of {} is empty", empty.owner.id));
    }
    let Some(first) = slot.options.first() else {
        return;
    };
    let chosen = commands::select(h.engine, &s, &slot.key, Choice::Element(first.id.clone()))
        .expect("companion pick")
        .state;
    let after = h.engine.derive(&chosen);
    if !after.elements.iter().any(|r| r.element.id == first.id) {
        failures.push(format!(
            "chosen companion {} is not part of the character",
            first.id
        ));
    }
}

#[test]
fn aurora_corpus_requirements_and_companions() {
    let Some(repo) = repo_dir().filter(|d| d.is_dir()) else {
        eprintln!("SKIPPED: BOREALIS_CORPUS_DIR is not set or has no {REPO}");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let store = load(tmp.path(), &repo);
    let prefix = format!("{PACK}/");
    let ours: HashSet<String> = store
        .iter()
        .filter(|s| s.file.starts_with(&prefix))
        .map(|s| s.element.id.clone())
        .collect();
    let mut failures = Vec::new();

    let invalid = store_invalid_expressions(&store, &ours);
    println!(
        "store InvalidExpression on {REPO}: {} {invalid:?}",
        invalid.len()
    );
    if invalid.len() > 1 {
        failures.push(format!("{} invalid expressions", invalid.len()));
    }

    let engine = Engine::new(&store);
    let h = Host { engine: &engine };
    let bad = bad_expressions(&engine.derive(&h.new_char(FIGHTER)), &ours);
    println!("content.bad_expression on {REPO}: {} {bad:?}", bad.len());
    if !bad.is_empty() {
        failures.push(format!("{} bad expressions", bad.len()));
    }

    check_caster_feats(&h, &mut failures);
    check_rangers_companion(&h, &ours, &mut failures);
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Newer Aurora internal ids (race tags, background ASI, custom ASI options) and PHB 2024 ids
/// used by the partnered repo resolve once it is stacked on aurorabuilder.
#[test]
fn partnered_content_resolves_newer_aurora_ids() {
    let Some(base) = repo_dir().filter(|d| d.is_dir()) else {
        eprintln!("SKIPPED: BOREALIS_CORPUS_DIR is not set or has no {REPO}");
        return;
    };
    let partnered = base.with_file_name(PARTNERED);
    if !partnered.is_dir() {
        eprintln!("SKIPPED: no {PARTNERED}");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let store = build(&[
        convert(tmp.path(), &base, PACK),
        convert(tmp.path(), &partnered, PARTNERED_PACK),
    ]);
    let prefix = format!("{PARTNERED_PACK}/");
    let ours: HashSet<String> = store
        .iter()
        .filter(|s| s.file.starts_with(&prefix))
        .map(|s| s.element.id.clone())
        .collect();
    let dangling = store
        .validate()
        .into_iter()
        .filter(|i| i.code == IssueCode::DanglingReference)
        .filter(|i| i.message.starts_with("grant references"))
        .filter(|i| i.element.as_ref().is_some_and(|e| ours.contains(e)))
        .count();
    println!("dangling grant references in {PARTNERED}: {dangling}");
    let mut failures = Vec::new();
    if dangling > MAX_PARTNERED_DANGLING_GRANTS {
        failures.push(format!("{dangling} dangling grant references"));
    }

    let engine = Engine::new(&store);
    let h = Host { engine: &engine };
    let fighter = h.new_char(FIGHTER);
    for race in store
        .by_type("Race")
        .filter(|r| ours.contains(&r.element.id))
    {
        let id = &race.element.id;
        let Some(s) = h.pick(&fighter, id) else {
            failures.push(format!("{id} not offered"));
            continue;
        };
        let snap = engine.derive(&s);
        for d in &snap.diagnostics {
            // PHB 2024 spells outside the SRD need the user's own PHB 2024 content
            let phb24 = d.message.contains("ID_WOTC_PHB24_");
            if d.code == "content.missing_element" && !phb24 {
                failures.push(format!("{id}: {}", d.message));
            }
        }
        for slot in &snap.slots {
            if slot.ty == "Ability Score Improvement"
                && slot.options.is_empty()
                && ours.contains(&slot.owner.id)
            {
                failures.push(format!("{id}: empty ASI select {}", slot.name));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
