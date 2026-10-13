//! Regression: with every bundled pack loaded (both editions), class tables resolve to the
//! character's own edition. A 2024 Barbarian once read the 2014 class by name (Rage max 0).
use borealis_content::{ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::CharacterState;
use borealis_engine::{Choice, Engine};
use std::path::Path;
use std::sync::OnceLock;

const PACKS: [&str; 5] = [
    "core",
    "srd-5.1",
    "srd-5.2.1",
    "srd-5.1-monsters",
    "srd-5.2.1-monsters",
];

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let mut b = ContentStoreBuilder::new();
        for p in PACKS.iter() {
            b = b.add_pack_dir(&root.join(p)).expect("pack loads");
        }
        Engine::new(&b.build())
    })
}

fn pick(s: &CharacterState, owner: &str, name: &str, id: &str) -> CharacterState {
    let snap = engine().derive(s);
    let key = slot_key(&snap, owner, name, 1).unwrap_or_else(|| panic!("slot {name}"));
    commands::select(engine(), s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
        .state
}

fn hero(rules: Ruleset, race: &str, class: &str, level: u32) -> CharacterSnapshot {
    let mut s = commands::new_character(engine(), rules);
    s = pick(&s, "ID_LEVEL_1", "Race", race);
    s = pick(&s, "ID_LEVEL_1", "Class", class);
    while s.level() < level {
        s = commands::level_up_main(engine(), &s, None).unwrap().state;
    }
    engine().derive(&s)
}

fn max_of(snap: &CharacterSnapshot, id: &str) -> i32 {
    snap.resources
        .iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("no resource {id}"))
        .max
}

#[test]
fn every_class_table_follows_the_characters_edition_with_all_packs_loaded() {
    let b24 = hero(
        Ruleset::Dnd2024,
        "ID_2024_SPECIES_DWARF",
        "ID_2024_CLASS_BARBARIAN",
        5,
    );
    assert_eq!(max_of(&b24, "rage"), 3);
    let toggle = b24.toggles.iter().find(|t| t.id == "rage").expect("rage");
    assert!(toggle.effects.iter().any(|e| e.contains("+2")));

    let m24 = hero(
        Ruleset::Dnd2024,
        "ID_2024_SPECIES_DWARF",
        "ID_2024_CLASS_MONK",
        5,
    );
    assert_eq!(max_of(&m24, "focus points"), 5);
    assert_eq!(m24.martial_arts.as_ref().expect("martial arts").die, "1d8");

    let b14 = hero(
        Ruleset::Dnd2014,
        "ID_RACE_HUMAN",
        "ID_WOTC_PHB_CLASS_BARBARIAN",
        5,
    );
    let stat = |k: &str| b14.stats.get(k).map(|s| s.value);
    assert_eq!(
        stat("barbarian rage:count"),
        Some(3),
        "{:?}",
        b14.stats.keys().collect::<Vec<_>>()
    );
    assert_eq!(stat("barbarian rage:damage"), Some(2));
}
