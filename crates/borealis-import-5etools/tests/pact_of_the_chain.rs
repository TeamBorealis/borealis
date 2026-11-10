//! A 2024 Warlock with the imported Eldritch Invocation Pact of the Chain (reported on the
//! Borealis Discord): Find Familiar is one of the Warlock's spells, ready to cast without
//! expending a spell slot, and never counts against the prepared spells.
//!
//! An invocation that casts a spell at will (Armor of Shadows: Mage Armor) is the same.
//!
//! The SRD fixture data (`tests/fixtures/data`) plus `tests/fixtures/warlock` (the SRD 5.2.1
//! Warlock class, the Pact of the Chain and Armor of Shadows invocations and their spells, all
//! SRD-flagged), imported as the app imports a 5etools folder.

mod support;

use borealis_content::{ContentStoreBuilder, Element, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterSnapshot, CharacterState, Choice, Engine};
use borealis_import_5etools::{import_folder, ImportOptions, ImportResult};
use serde_json::Value;
use std::path::{Path, PathBuf};
use support::fixtures;

const SCORES: [i32; 6] = [8, 14, 13, 12, 10, 15];

fn warlock_fixtures() -> PathBuf {
    fixtures().join("../warlock")
}

fn json_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            json_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "json") {
            out.push(p);
        }
    }
}

/// `from` copied below `to`; a file both trees hold gets the arrays of both.
fn merge_tree(from: &Path, to: &Path) {
    let mut files = Vec::new();
    json_files(from, &mut files);
    for f in files {
        let dest = to.join(f.strip_prefix(from).unwrap());
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        let add: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        let merged = if dest.exists() {
            let mut base: Value =
                serde_json::from_str(&std::fs::read_to_string(&dest).unwrap()).unwrap();
            for (key, list) in add.as_object().unwrap() {
                let target = base
                    .as_object_mut()
                    .unwrap()
                    .entry(key.clone())
                    .or_insert_with(|| Value::Array(Vec::new()));
                target
                    .as_array_mut()
                    .unwrap()
                    .extend(list.as_array().unwrap().iter().cloned());
            }
            base
        } else {
            add
        };
        std::fs::write(&dest, merged.to_string()).unwrap();
    }
}

fn content() -> PathBuf {
    support::core_pack().join("..")
}

fn import(tmp: &Path) -> ImportResult {
    let data = tmp.join("data");
    merge_tree(&fixtures(), &data);
    merge_tree(&warlock_fixtures(), &data);
    let options = ImportOptions {
        srd_dirs: vec![content().join("srd-5.1"), content().join("srd-5.2.1")],
        ..ImportOptions::default()
    };
    import_folder(&data, &options).expect("fixtures import")
}

/// The imported element of this type and name from 5etools source `source` (`XPHB`).
fn element<'a>(result: &'a ImportResult, ty: &str, name: &str, source: &str) -> &'a Element {
    let prefix = format!("ID_5ET_{source}_");
    result
        .packs
        .iter()
        .flat_map(|p| p.elements())
        .find(|e| e.ty == ty && e.name == name && e.id.starts_with(&prefix))
        .unwrap_or_else(|| panic!("no imported {ty} {name} from {source}"))
}

/// Pick `id` in the first open slot that offers it.
fn pick(e: &Engine, s: &CharacterState, id: &str) -> CharacterState {
    let snap = e.derive(s);
    let slot = snap
        .slots
        .iter()
        .find(|x| x.chosen.is_none() && x.options.iter().any(|o| o.id == id))
        .unwrap_or_else(|| panic!("{id} is not offered"));
    commands::select(e, s, &slot.key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("{id} refused: {err}"))
        .state
}

fn warlock_block(snap: &CharacterSnapshot) -> &borealis_engine::snapshot::SpellcastingSnapshot {
    snap.spellcasting
        .iter()
        .find(|c| c.name == "Warlock")
        .unwrap_or_else(|| panic!("no Warlock block: {:?}", snap.spellcasting))
}

#[test]
fn a_2024_warlock_with_pact_of_the_chain_has_find_familiar_ready_and_uncounted() {
    borealis_engine::invariants::enable_for_thread();
    let tmp = tempfile::tempdir().unwrap();
    let result = import(tmp.path());
    let class = element(&result, "Class", "Warlock", "XPHB").id.clone();
    let pact = result
        .packs
        .iter()
        .flat_map(|p| p.elements())
        .find(|e| e.name == "Pact of the Chain" && e.id.starts_with("ID_5ET_XPHB_"))
        .expect("2024 Pact of the Chain imported")
        .id
        .clone();
    let familiar = element(&result, "Spell", "Find Familiar", "XPHB")
        .id
        .clone();

    let mut builder = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        builder = builder.add_pack_dir(&content().join(pack)).unwrap();
    }
    for pack in &result.packs {
        builder = builder
            .add_pack_dir(&pack.write_to(&tmp.path().join("packs")).unwrap())
            .expect("imported pack loads");
    }
    let store = builder.build();
    let e = Engine::new(&store);

    let s = commands::new_character(&e, Ruleset::Dnd2024);
    let s = commands::set_base_scores(&e, &s, SCORES)
        .expect("scores")
        .state;
    let key = slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    let s = commands::select(&e, &s, &key, Choice::Element(class.clone()))
        .expect("class")
        .state;
    let before = e.derive(&s);
    assert!(
        !warlock_block(&before)
            .spells
            .iter()
            .any(|x| x.id == familiar),
        "Find Familiar before the invocation"
    );

    let s = pick(&e, &s, &pact);
    let snap = e.derive(&s);
    let block = warlock_block(&snap);
    let ff = block
        .spells
        .iter()
        .find(|x| x.id == familiar)
        .unwrap_or_else(|| {
            panic!(
                "Find Familiar not among the Warlock spells: {:?}",
                block.spells
            )
        });
    assert!(block.prepare, "the 2024 Warlock prepares its spells");
    assert!(
        ff.prepared && ff.always_prepared,
        "Find Familiar is ready without being prepared: {ff:?}"
    );
    assert!(ff.no_slot, "cast without expending a spell slot: {ff:?}");
    assert_eq!(
        block.prepared_count, 0,
        "Find Familiar counts against the prepared spells"
    );

    // level 2 brings two more invocations: Armor of Shadows casts Mage Armor at will
    let armor = result
        .packs
        .iter()
        .flat_map(|p| p.elements())
        .find(|e| e.name == "Armor of Shadows" && e.id.starts_with("ID_5ET_XPHB_"))
        .expect("2024 Armor of Shadows imported")
        .id
        .clone();
    let mage_armor = element(&result, "Spell", "Mage Armor", "XPHB").id.clone();
    let s = commands::level_up_main(&e, &s, None)
        .expect("level 2")
        .state;
    let s = pick(&e, &s, &armor);
    let snap = e.derive(&s);
    let block = warlock_block(&snap);
    let ma = block
        .spells
        .iter()
        .find(|x| x.id == mage_armor)
        .unwrap_or_else(|| {
            panic!(
                "Mage Armor not among the Warlock spells: {:?}",
                block.spells
            )
        });
    assert!(
        ma.always_prepared && ma.no_slot,
        "Mage Armor at will: {ma:?}"
    );
    assert_eq!(
        block.prepared_count, 0,
        "invocation spells count as prepared"
    );
}
