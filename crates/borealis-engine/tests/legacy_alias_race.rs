//! A 2024 species that was a race (`legacy_alias: "Race"`, set by the 5etools importer on the
//! races IM8 renamed to species) still fills a 2014 character's Race slot: a character saved
//! with the former race keeps a race that applies in full (speed, traits, its own spellcasting
//! choice), and a new 2014 character is offered it only with "allow the other edition" on.
//! A species without the alias (Aurora 2024 content) is never used as a 2014 race. Synthetic
//! content on the bundled core + SRD 5.1 packs.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::state::{CharacterState, Choice, Selection, SlotKey};
use borealis_engine::{CharacterSnapshot, Engine};
use serde_json::{json, Value};
use std::path::Path;

const SPECIES: &str = "ID_T_SPECIES_MOTHFOLK";
const PLAIN_SPECIES: &str = "ID_T_SPECIES_AURORA_STYLE";
const OLD_RACE: &str = "ID_SRD_RACE_DWARF";
const TRAIT: &str = "ID_T_SPECIES_MOTHFOLK_WINGS";
const CASTING_WIS: &str = "ID_T_SPECIES_MOTHFOLK_SPELLCASTING_WIS";
const CASTING_CHA: &str = "ID_T_SPECIES_MOTHFOLK_SPELLCASTING_CHA";
const LIGHT: &str = "ID_PHB_SPELL_LIGHT";
const FIGHTER: &str = "ID_WOTC_PHB_CLASS_FIGHTER";
const SPEED: i32 = 35;

fn casting(id: &str, ability: &str) -> Value {
    json!({"id": id, "name": format!("Mothfolk Spellcasting ({ability})"), "type": "Grants",
           "source": "Test", "ruleset": "dnd2024", "supports": ["SPELLCASTING_OF_MOTHFOLK"],
           "rules": [{"kind": "grant", "type": "Spell", "id": LIGHT}]})
}

fn engine() -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let elements = vec![
        json!({"id": SPECIES, "name": "Mothfolk", "type": "Species", "source": "Test",
               "ruleset": "dnd2024", "legacy_alias": "Race",
               "renamed_from": "ID_T_RACE_MOTHFOLK",
               "rules": [
                   {"kind": "stat", "name": "innate speed", "value": SPEED.to_string(),
                    "bonus": "base"},
                   {"kind": "grant", "type": "Size", "id": "ID_SIZE_MEDIUM"},
                   {"kind": "grant", "type": "Racial Trait", "id": TRAIT},
                   {"kind": "select", "type": "Grants", "name": "Spellcasting Ability",
                    "supports": "SPELLCASTING_OF_MOTHFOLK"}]}),
        json!({"id": TRAIT, "name": "Soft Wings", "type": "Racial Trait", "source": "Test",
               "ruleset": "dnd2024",
               "rules": [{"kind": "stat", "name": "innate speed:fly", "value": "20",
                          "bonus": "base"}]}),
        casting(CASTING_WIS, "Wisdom"),
        casting(CASTING_CHA, "Charisma"),
        json!({"id": PLAIN_SPECIES, "name": "Plain Species", "type": "Species", "source": "Test",
               "ruleset": "dnd2024",
               "rules": [{"kind": "stat", "name": "innate speed", "value": "40",
                          "bonus": "base"}]}),
    ];
    let file: ContentFile = serde_json::from_value(json!({
        "format": "borealis-content/1", "ruleset": "dnd2024", "elements": elements}))
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .expect("bundled packs")
        .add_file("species.json", PackOrigin::Custom, file)
        .build();
    Engine::new(&store)
}

fn hero(e: &Engine, allow: Option<bool>) -> CharacterState {
    let mut s = commands::new_character(e, Ruleset::Dnd2014);
    s.allow_cross_edition = allow;
    commands::set_base_scores(e, &s, [15, 14, 13, 12, 10, 8])
        .unwrap()
        .state
}

fn pick(e: &Engine, s: &CharacterState, owner: &str, name: &str, id: &str) -> CharacterState {
    let key = slot_key(&e.derive(s), owner, name, 1).expect("slot");
    commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("{id}: {err}"))
        .state
}

/// A character saved while the species was a 2014 race: the stored picks name it directly, as
/// the app's id migration leaves them, without going through the option filter.
fn saved_with_species(e: &Engine) -> CharacterState {
    let s = pick(e, &hero(e, None), "ID_LEVEL_1", "Class", FIGHTER);
    let mut s = pick(e, &s, "ID_LEVEL_1", "Race", OLD_RACE);
    for x in &mut s.selections {
        if matches!(&x.choice, Choice::Element(id) if id == OLD_RACE) {
            x.choice = Choice::Element(SPECIES.into());
        }
    }
    // the species' "Spellcasting Ability" select is its rule 3
    s.selections.push(Selection {
        slot: SlotKey::new(SPECIES, 3, 1),
        choice: Choice::Element(CASTING_WIS.into()),
        replace_level: None,
    });
    s
}

fn race_options(snap: &CharacterSnapshot) -> Vec<String> {
    snap.slot("ID_LEVEL_1", "Race")
        .expect("race slot")
        .options
        .iter()
        .map(|o| o.id.clone())
        .collect()
}

fn has(snap: &CharacterSnapshot, id: &str) -> bool {
    serde_json::to_string(snap).unwrap().contains(id)
}

#[test]
fn a_character_saved_with_a_former_race_applies_its_species_in_full() {
    let e = engine();
    let s = saved_with_species(&e);
    let snap = e.derive(&s);
    let race = snap.slot("ID_LEVEL_1", "Race").expect("race slot");
    assert_eq!(race.chosen.as_ref().map(|c| c.id.as_str()), Some(SPECIES));
    assert!(race.rejected.is_none(), "{:?}", race.rejected);
    assert!(
        snap.invalid_selections.is_empty(),
        "{:?}",
        snap.invalid_selections
    );
    // speed and the granted trait apply
    assert_eq!(snap.speed.walk, SPEED);
    assert_eq!(snap.speed.fly, 20, "the trait's fly speed");
    // the species' own (2024) spellcasting choice is kept and grants its spell
    let ability = snap
        .slot(SPECIES, "Spellcasting Ability")
        .expect("species select");
    assert_eq!(
        ability.chosen.as_ref().map(|c| c.id.as_str()),
        Some(CASTING_WIS)
    );
    assert!(has(&snap, LIGHT), "Light granted");
    // the species is reported as foreign content used as a race
    let entry = snap
        .compatibility
        .entries
        .iter()
        .find(|c| c.element.id == SPECIES)
        .expect("compatibility entry");
    assert!(entry.notes.iter().any(|n| n.contains("used as a Race")));
}

#[test]
fn the_species_own_choice_offers_its_options_once_it_is_the_race() {
    let e = engine();
    let s = pick(&e, &hero(&e, Some(true)), "ID_LEVEL_1", "Race", SPECIES);
    let snap = e.derive(&s);
    let ability = snap
        .slot(SPECIES, "Spellcasting Ability")
        .expect("species select");
    let offered: Vec<&str> = ability.options.iter().map(|o| o.id.as_str()).collect();
    assert!(
        offered.contains(&CASTING_WIS) && offered.contains(&CASTING_CHA),
        "{offered:?}"
    );
    let s = pick(&e, &s, SPECIES, "Spellcasting Ability", CASTING_CHA);
    let snap = e.derive(&s);
    assert!(has(&snap, LIGHT));
    // the option is never offered to a slot outside the species
    let plain = e.derive(&hero(&e, Some(true)));
    assert!(!has(&plain, CASTING_CHA));
}

#[test]
fn the_2014_race_slot_offers_the_species_only_with_the_other_edition_allowed() {
    let e = engine();
    for (allow, offered) in [(Some(true), true), (Some(false), false), (None, false)] {
        let options = race_options(&e.derive(&hero(&e, allow)));
        assert_eq!(options.contains(&SPECIES.to_string()), offered, "{allow:?}");
        // a species without the alias (Aurora 2024 content) never fills a 2014 race slot
        assert!(!options.contains(&PLAIN_SPECIES.to_string()), "{allow:?}");
        // 2014 races are offered as before
        assert!(options.contains(&OLD_RACE.to_string()), "{allow:?}");
    }
}

#[test]
fn a_species_without_the_alias_stays_invalid_in_a_2014_race_slot() {
    let e = engine();
    let mut s = pick(&e, &hero(&e, Some(true)), "ID_LEVEL_1", "Race", OLD_RACE);
    for x in &mut s.selections {
        if matches!(&x.choice, Choice::Element(id) if id == OLD_RACE) {
            x.choice = Choice::Element(PLAIN_SPECIES.into());
        }
    }
    let snap = e.derive(&s);
    let race = snap.slot("ID_LEVEL_1", "Race").expect("race slot");
    assert!(race.chosen.is_none());
    assert_eq!(
        race.rejected,
        Some(Choice::Element(PLAIN_SPECIES.into())),
        "a 2024 species without legacy_alias is not a 2014 race"
    );
    assert_ne!(snap.speed.walk, 40);
}

#[test]
fn a_2014_race_is_unaffected() {
    let e = engine();
    let s = pick(&e, &hero(&e, None), "ID_LEVEL_1", "Race", OLD_RACE);
    let snap = e.derive(&s);
    assert_eq!(snap.speed.walk, 25);
    assert!(snap.compatibility.entries.is_empty());
    assert!(!has(&snap, SPECIES));
}
