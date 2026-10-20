//! IM9 (user, 2026-10-19): a 2014 character (other-edition setting on) that picks a 2024
//! species, which has no ability increase, is offered a flexible +2/+1 or +1/+1/+1 to any
//! abilities, as with Tasha's custom origin. The adapter adds it when the universe is prepared
//! (stored data untouched, R4) and the compatibility report says so. Not for 2024 characters, not
//! for 2014 races, and not for a 2024 species whose own rules or parts already raise a score.
//! IM9a (user, 2026-10-20): not applied while the character has a background that gives its own
//! increase (a 2024 background), in either pick order; the report says so.
//! Bundled core + SRD 5.1 + SRD 5.2.1 packs, plus two synthetic 2024 species with increases.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::{CharacterSnapshot, SlotSnapshot};
use borealis_engine::state::CharacterState;
use borealis_engine::{Choice, Engine, FLEXIBLE_INCREASE_NAME, INCREASE_NOT_APPLIED};
use serde_json::json;
use std::path::Path;
use std::sync::OnceLock;

const FIGHTER_14: &str = "ID_WOTC_PHB_CLASS_FIGHTER";
const FIGHTER_24: &str = "ID_2024_CLASS_FIGHTER";
const ELF_24: &str = "ID_2024_SPECIES_ELF";
const DWARF_24: &str = "ID_2024_SPECIES_DWARF";
const DWARF_14: &str = "ID_SRD_RACE_DWARF";
const OWN_ASI_24: &str = "ID_T_2024_SPECIES_OWN_ASI";
const LINEAGE_ASI_24: &str = "ID_T_2024_SPECIES_LINEAGE_ASI";
const BASE: [i32; 6] = [15, 14, 13, 12, 10, 8];

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let file: ContentFile = serde_json::from_value(json!({
        "format": "borealis-content/1", "ruleset": "dnd2024",
        "elements": [
            {"id": OWN_ASI_24, "name": "Test Species With Increase", "type": "Species",
             "source": "Test", "ruleset": "dnd2024",
             "rules": [{"kind": "stat", "name": "strength", "value": "1"}]},
            {"id": LINEAGE_ASI_24, "name": "Test Species With Lineages", "type": "Species",
             "source": "Test", "ruleset": "dnd2024",
             "rules": [{"kind": "grant", "type": "Racial Trait", "id": "ID_T_2024_TRAIT_LINEAGE"}]},
            {"id": "ID_T_2024_TRAIT_LINEAGE", "name": "Test Lineage", "type": "Racial Trait",
             "source": "Test", "ruleset": "dnd2024",
             "rules": [{"kind": "select", "type": "Species Trait Option",
                        "name": "Test Lineage", "supports": "Test Lineage"}]},
            {"id": "ID_T_2024_OPTION_LINEAGE_A", "name": "Lineage A",
             "type": "Species Trait Option", "source": "Test", "ruleset": "dnd2024",
             "supports": ["Test Lineage"],
             "rules": [{"kind": "ability_increase", "options": [{"abilities": 1, "amount": 1}]}]}
        ]}))
        .expect("synthetic content");
        let store = ContentStoreBuilder::new()
            .add_pack_dir(&root.join("core"))
            .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
            .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1")))
            .expect("bundled packs")
            .add_file("species.json", PackOrigin::Custom, file)
            .build();
        Engine::new(&store)
    })
}

fn pick(s: &CharacterState, owner: &str, name: &str, id: &str) -> CharacterState {
    let e = engine();
    let key = slot_key(&e.derive(s), owner, name, 1).unwrap_or_else(|| panic!("slot {name}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("{id}: {err}"))
        .state
}

/// A level 1 fighter of `ruleset` with the given race and the setting on.
fn fighter_with(ruleset: Ruleset, base: [i32; 6], race: &str) -> CharacterState {
    let e = engine();
    let class = if ruleset == Ruleset::Dnd2014 {
        FIGHTER_14
    } else {
        FIGHTER_24
    };
    let mut s = commands::new_character(e, ruleset);
    s.allow_cross_edition = Some(true);
    s = commands::set_base_scores(e, &s, base).unwrap().state;
    s = pick(&s, "ID_LEVEL_1", "Class", class);
    pick(&s, "ID_LEVEL_1", "Race", race)
}

fn increase_slot<'a>(snap: &'a CharacterSnapshot, owner: &str) -> Option<&'a SlotSnapshot> {
    snap.slots
        .iter()
        .find(|k| k.owner.id == owner && k.name == FLEXIBLE_INCREASE_NAME)
}

fn choose(s: &CharacterState, owner: &str, text: &str) -> CharacterState {
    let snap = engine().derive(s);
    let slot = increase_slot(&snap, owner).expect("increase slot");
    let (item, _) = slot
        .list_items
        .iter()
        .find(|(_, t)| t == text)
        .unwrap_or_else(|| panic!("{text} in {:?}", slot.list_items))
        .clone();
    commands::select(engine(), s, &slot.key, Choice::ListItem(item))
        .expect("increase pick")
        .state
}

fn scores(s: &CharacterState) -> Vec<i32> {
    engine()
        .derive(s)
        .abilities
        .iter()
        .map(|a| a.score)
        .collect()
}

#[test]
fn a_2014_fighter_with_a_2024_species_is_offered_the_flexible_increase() {
    let s = fighter_with(Ruleset::Dnd2014, BASE, ELF_24);
    let snap = engine().derive(&s);
    let slot = increase_slot(&snap, ELF_24).expect("the IM9 choice is offered");
    assert!(slot.chosen.is_none() && slot.rejected.is_none());
    // 30 ordered +2/+1 pairs of different abilities and 20 +1 triples
    assert_eq!(slot.list_items.len(), 50, "{:?}", slot.list_items);
    assert_eq!(scores(&s), BASE.to_vec(), "nothing applied until chosen");
}

#[test]
fn both_shapes_apply_to_the_scores() {
    let s = fighter_with(Ruleset::Dnd2014, BASE, ELF_24);
    let two_one = choose(&s, ELF_24, "Strength +2, Dexterity +1");
    assert_eq!(scores(&two_one), vec![17, 15, 13, 12, 10, 8]);
    let three = choose(&s, ELF_24, "Constitution +1, Wisdom +1, Charisma +1");
    assert_eq!(scores(&three), vec![15, 14, 14, 12, 11, 9]);
    // another 2024 species, same choice
    let dwarf = fighter_with(Ruleset::Dnd2014, BASE, DWARF_24);
    let dwarf = choose(&dwarf, DWARF_24, "Dexterity +1, Constitution +2");
    assert_eq!(scores(&dwarf), vec![15, 15, 15, 12, 10, 8]);
}

#[test]
fn the_increase_respects_the_maximum_of_20() {
    let s = fighter_with(Ruleset::Dnd2014, [19, 14, 13, 12, 10, 8], ELF_24);
    let s = choose(&s, ELF_24, "Strength +2, Dexterity +1");
    assert_eq!(scores(&s), vec![20, 15, 13, 12, 10, 8]);
}

#[test]
fn the_compatibility_report_lists_the_increase() {
    let s = fighter_with(Ruleset::Dnd2014, BASE, ELF_24);
    let snap = engine().derive(&choose(&s, ELF_24, "Strength +2, Dexterity +1"));
    let entry = snap
        .compatibility
        .entries
        .iter()
        .find(|c| c.element.id == ELF_24)
        .expect("species in the report");
    assert_eq!(entry.from, Ruleset::Dnd2024);
    assert!(entry.converted, "{entry:?}");
    assert!(
        entry
            .notes
            .iter()
            .any(|n| n.starts_with("Ability score increase added")),
        "{:?}",
        entry.notes
    );
}

#[test]
fn a_2024_character_with_the_same_species_gets_no_choice() {
    let s = fighter_with(Ruleset::Dnd2024, BASE, ELF_24);
    let snap = engine().derive(&s);
    assert!(snap.has_element(ELF_24));
    assert!(increase_slot(&snap, ELF_24).is_none());
    assert!(
        snap.compatibility
            .entries
            .iter()
            .all(|c| c.element.id != ELF_24),
        "native species, no report entry"
    );
}

#[test]
fn a_2014_race_gets_no_choice() {
    let s = fighter_with(Ruleset::Dnd2014, BASE, DWARF_14);
    let snap = engine().derive(&s);
    assert!(snap.has_element(DWARF_14));
    assert!(snap.slots.iter().all(|k| k.name != FLEXIBLE_INCREASE_NAME));
}

#[test]
fn a_2024_species_that_already_raises_a_score_gets_no_choice() {
    for species in [OWN_ASI_24, LINEAGE_ASI_24] {
        let s = fighter_with(Ruleset::Dnd2014, BASE, species);
        let snap = engine().derive(&s);
        assert!(snap.has_element(species), "{species}");
        assert!(increase_slot(&snap, species).is_none(), "{species}");
    }
}

#[test]
fn turning_the_setting_off_afterwards_breaks_nothing() {
    let s = fighter_with(Ruleset::Dnd2014, BASE, ELF_24);
    let mut s = choose(&s, ELF_24, "Strength +2, Dexterity +1");
    let picked = scores(&s);
    s.allow_cross_edition = Some(false);
    let snap = engine().derive(&s);
    // the stored species is replayed as it is (new 2024 picks are refused), with its increase
    assert!(snap.has_element(ELF_24));
    let slot = increase_slot(&snap, ELF_24).expect("stored increase slot");
    assert!(
        slot.list_choice.is_some() && slot.rejected.is_none(),
        "{slot:?}"
    );
    assert_eq!(scores(&s), picked);
    // and a fresh character with the setting off is not offered the species at all
    let mut fresh = commands::new_character(engine(), Ruleset::Dnd2014);
    fresh.allow_cross_edition = Some(false);
    fresh = commands::set_base_scores(engine(), &fresh, BASE)
        .unwrap()
        .state;
    fresh = pick(&fresh, "ID_LEVEL_1", "Class", FIGHTER_14);
    let snap = engine().derive(&fresh);
    assert!(snap
        .slots
        .iter()
        .filter(|k| k.ty == "Race")
        .all(|k| k.options.iter().all(|o| o.id != ELF_24)));
}

// IM9a (user, 2026-10-20): the IM9 increase is not applied when the character also has a
// background that gives its own increase (a 2024 background): only one origin increase applies,
// the background's. The choice requires that none of those backgrounds is on the character.

const ACOLYTE_14: &str = "ID_BACKGROUND_ACOLYTE";
const SOLDIER_24: &str = "ID_2024_BACKGROUND_SOLDIER";
const SOLDIER_SCORES: &str = "Ability Scores (Soldier)";
const STR2_DEX1: [i32; 6] = [17, 15, 13, 12, 10, 8];
const IM9_CON_WIS_CHA: [i32; 6] = [15, 14, 14, 12, 11, 9];

fn with_background(s: &CharacterState, id: &str) -> CharacterState {
    pick(s, "ID_LEVEL_1", "Background", id)
}

/// Pick the Soldier's own +2/+1 (Strength +2, Dexterity +1).
fn soldier_increase(s: &CharacterState) -> CharacterState {
    let snap = engine().derive(s);
    let slot = snap
        .slots
        .iter()
        .find(|k| k.owner.id == SOLDIER_24 && k.name == SOLDIER_SCORES)
        .expect("the 2024 background's increase");
    let (item, _) = slot
        .list_items
        .iter()
        .find(|(_, t)| t == "Strength +2, Dexterity +1")
        .expect("list entry")
        .clone();
    commands::select(engine(), s, &slot.key, Choice::ListItem(item))
        .expect("background increase")
        .state
}

fn species_notes(s: &CharacterState) -> Vec<String> {
    engine()
        .derive(s)
        .compatibility
        .entries
        .iter()
        .find(|c| c.element.id == ELF_24)
        .expect("species in the report")
        .notes
        .clone()
}

/// No rejected slot and no selection or evaluation diagnostic.
fn no_problems(s: &CharacterState) {
    let snap = engine().derive(s);
    assert!(
        snap.slots.iter().all(|k| k.rejected.is_none()),
        "no rejected slot"
    );
    assert!(
        snap.diagnostics
            .iter()
            .all(|d| !d.code.starts_with("selection.") && !d.code.starts_with("eval.")),
        "{:?}",
        snap.diagnostics
    );
}

#[test]
fn with_a_2014_background_the_increase_is_offered_and_applies() {
    let s = with_background(&fighter_with(Ruleset::Dnd2014, BASE, ELF_24), ACOLYTE_14);
    let s = choose(&s, ELF_24, "Strength +2, Dexterity +1");
    assert_eq!(scores(&s), STR2_DEX1.to_vec());
    assert!(!species_notes(&s).iter().any(|n| n == INCREASE_NOT_APPLIED));
    no_problems(&s);
}

#[test]
fn with_a_2024_background_only_the_background_increase_applies() {
    let s = with_background(&fighter_with(Ruleset::Dnd2014, BASE, ELF_24), SOLDIER_24);
    assert!(
        increase_slot(&engine().derive(&s), ELF_24).is_none(),
        "the IM9 choice is not offered"
    );
    assert_eq!(
        scores(&s),
        BASE.to_vec(),
        "nothing until the background's pick"
    );
    let s = soldier_increase(&s);
    assert_eq!(scores(&s), STR2_DEX1.to_vec(), "the background's, once");
    no_problems(&s);
}

#[test]
fn picking_a_2024_background_after_the_increase_switches_it_off() {
    let s = choose(
        &fighter_with(Ruleset::Dnd2014, BASE, ELF_24),
        ELF_24,
        "Constitution +1, Wisdom +1, Charisma +1",
    );
    assert_eq!(scores(&s), IM9_CON_WIS_CHA.to_vec());
    let s = soldier_increase(&with_background(&s, SOLDIER_24));
    assert!(increase_slot(&engine().derive(&s), ELF_24).is_none());
    assert_eq!(scores(&s), STR2_DEX1.to_vec(), "only the background's");
    no_problems(&s);
    // A command drops selections whose slot is gone (`prune`), so going back to a 2014
    // background offers the choice again, empty.
    let bg = slot_key(&engine().derive(&s), "ID_LEVEL_1", "Background", 1).expect("bg slot");
    let back = commands::select(engine(), &s, &bg, Choice::Element(ACOLYTE_14.into()))
        .expect("2014 background")
        .state;
    let snap = engine().derive(&back);
    let slot = increase_slot(&snap, ELF_24).expect("offered again");
    assert!(
        slot.list_choice.is_none() && slot.rejected.is_none(),
        "{slot:?}"
    );
    assert_eq!(scores(&back), BASE.to_vec());
    let back = choose(&back, ELF_24, "Strength +2, Dexterity +1");
    assert_eq!(scores(&back), STR2_DEX1.to_vec());
}

#[test]
fn a_stored_increase_is_kept_but_not_applied_under_a_2024_background() {
    // A state holding both (saved before IM9a, or edited by hand): the stored IM9 pick stays in
    // the state, unused and without error, and applies again once the background is 2014.
    let s = fighter_with(Ruleset::Dnd2014, BASE, ELF_24);
    let chosen = choose(&s, ELF_24, "Constitution +1, Wisdom +1, Charisma +1");
    let im9 = chosen
        .selections
        .iter()
        .find(|x| !s.selections.contains(x))
        .expect("the IM9 pick")
        .clone();
    let mut both = soldier_increase(&with_background(&s, SOLDIER_24));
    both.selections.push(im9.clone());
    assert_eq!(scores(&both), STR2_DEX1.to_vec());
    no_problems(&both);
    assert!(both.selections.contains(&im9));
    let swapped = CharacterState {
        selections: both
            .selections
            .iter()
            .map(|sel| match &sel.choice {
                Choice::Element(id) if id == SOLDIER_24 => borealis_engine::state::Selection {
                    choice: Choice::Element(ACOLYTE_14.into()),
                    ..sel.clone()
                },
                _ => sel.clone(),
            })
            .collect(),
        ..both
    };
    assert_eq!(scores(&swapped), IM9_CON_WIS_CHA.to_vec());
}

#[test]
fn the_compatibility_report_says_when_the_increase_is_not_applied() {
    let s = with_background(&fighter_with(Ruleset::Dnd2014, BASE, ELF_24), SOLDIER_24);
    let notes = species_notes(&s);
    assert!(notes.iter().any(|n| n == INCREASE_NOT_APPLIED), "{notes:?}");
    assert!(notes
        .iter()
        .any(|n| n.starts_with("Ability score increase added")));
    let s = with_background(&fighter_with(Ruleset::Dnd2014, BASE, ELF_24), ACOLYTE_14);
    assert!(!species_notes(&s).iter().any(|n| n == INCREASE_NOT_APPLIED));
}
