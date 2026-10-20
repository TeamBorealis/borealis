//! IM9a through a save and reload (native `borealis-character` JSON): a 2014 character with a
//! 2024 species and a 2024 background keeps only the background's ability increase, and a
//! stored IM9 pick left in the file stays there, unused and without error.
//! Bundled core + SRD 5.1 + SRD 5.2.1 packs.

use borealis_character::Character;
use borealis_content::{ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::SlotSnapshot;
use borealis_engine::state::CharacterState;
use borealis_engine::{Choice, Engine, FLEXIBLE_INCREASE_NAME, INCREASE_NOT_APPLIED};
use std::path::Path;
use std::sync::OnceLock;

const FIGHTER_14: &str = "ID_WOTC_PHB_CLASS_FIGHTER";
const ELF_24: &str = "ID_2024_SPECIES_ELF";
const SOLDIER_24: &str = "ID_2024_BACKGROUND_SOLDIER";
const BASE: [i32; 6] = [15, 14, 13, 12, 10, 8];
const STR2_DEX1: [i32; 6] = [17, 15, 13, 12, 10, 8];

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let store = ContentStoreBuilder::new()
            .add_pack_dir(&root.join("core"))
            .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
            .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1")))
            .expect("bundled packs")
            .build();
        Engine::new(&store)
    })
}

fn pick(s: &CharacterState, name: &str, id: &str) -> CharacterState {
    let e = engine();
    let key = slot_key(&e.derive(s), "ID_LEVEL_1", name, 1).unwrap_or_else(|| panic!("{name}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("{id}: {err}"))
        .state
}

fn list_pick(s: &CharacterState, slot: &SlotSnapshot, text: &str) -> CharacterState {
    let (item, _) = slot
        .list_items
        .iter()
        .find(|(_, t)| t == text)
        .unwrap_or_else(|| panic!("{text}"))
        .clone();
    commands::select(engine(), s, &slot.key, Choice::ListItem(item))
        .expect("list pick")
        .state
}

fn named_slot(s: &CharacterState, owner: &str, name: &str) -> Option<SlotSnapshot> {
    engine()
        .derive(s)
        .slots
        .into_iter()
        .find(|k| k.owner.id == owner && k.name == name)
}

fn scores(s: &CharacterState) -> Vec<i32> {
    engine()
        .derive(s)
        .abilities
        .iter()
        .map(|a| a.score)
        .collect()
}

fn reload(build: &CharacterState) -> CharacterState {
    let c = Character {
        build: build.clone(),
        ..Character::with_ruleset("Ilsa", Ruleset::Dnd2014)
    };
    let text = c.to_json_string().expect("save");
    let back = Character::from_json_str(&text).expect("load");
    assert_eq!(back, c, "the file round-trips");
    back.build
}

/// A 2014 fighter (setting on) with the 2024 Elf, the IM9 pick +1 Con/Wis/Cha, then the 2024
/// Soldier with its own Strength +2, Dexterity +1.
fn elf_soldier() -> (CharacterState, CharacterState) {
    let e = engine();
    let mut s = commands::new_character(e, Ruleset::Dnd2014);
    s.allow_cross_edition = Some(true);
    s = commands::set_base_scores(e, &s, BASE).unwrap().state;
    s = pick(&s, "Class", FIGHTER_14);
    s = pick(&s, "Race", ELF_24);
    let im9 = named_slot(&s, ELF_24, FLEXIBLE_INCREASE_NAME).expect("IM9 slot");
    let with_im9 = list_pick(&s, &im9, "Constitution +1, Wisdom +1, Charisma +1");
    let s = pick(&with_im9, "Background", SOLDIER_24);
    let dist = named_slot(&s, SOLDIER_24, "Ability Scores (Soldier)").expect("background slot");
    (with_im9, list_pick(&s, &dist, "Strength +2, Dexterity +1"))
}

#[test]
fn only_the_background_increase_survives_a_reload() {
    let (_, s) = elf_soldier();
    let back = reload(&s);
    let snap = engine().derive(&back);
    assert!(named_slot(&back, ELF_24, FLEXIBLE_INCREASE_NAME).is_none());
    assert_eq!(scores(&back), STR2_DEX1.to_vec());
    let notes = &snap
        .compatibility
        .entries
        .iter()
        .find(|c| c.element.id == ELF_24)
        .expect("species in the report")
        .notes;
    assert!(notes.iter().any(|n| n == INCREASE_NOT_APPLIED), "{notes:?}");
}

#[test]
fn a_stored_im9_pick_under_a_2024_background_survives_a_reload_unused() {
    let (with_im9, s) = elf_soldier();
    let im9 = with_im9
        .selections
        .iter()
        .find(|x| x.slot.owner == ELF_24)
        .expect("stored IM9 pick")
        .clone();
    let mut both = s.clone();
    both.selections.push(im9.clone());
    let back = reload(&both);
    assert!(back.selections.contains(&im9), "kept in the file");
    assert_eq!(scores(&back), STR2_DEX1.to_vec(), "not applied");
    let snap = engine().derive(&back);
    assert!(snap.slots.iter().all(|k| k.rejected.is_none()));
    assert!(snap
        .diagnostics
        .iter()
        .all(|d| !d.code.starts_with("selection.")));
}
