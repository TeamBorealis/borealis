//! B-55 (`specs/version-rows-and-filters.md` 3): switching a chosen row to another version of
//! the same name replaces the pick in place and keeps the sub-choices that still fit. Synthetic
//! imported species on the bundled core + SRD 5.1 + SRD 5.2.1 packs.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, CommandError};
use borealis_engine::snapshot::{CharacterSnapshot, SlotSnapshot};
use borealis_engine::state::{CharacterState, Choice, SlotKey};
use borealis_engine::Engine;
use serde_json::{json, Value};
use std::path::Path;

const SRD24: &str = "ID_2024_SPECIES_DRAGONBORN";
const SRD14: &str = "ID_RACE_DRAGONBORN";
const RED: &str = "ID_2024_OPTION_DRACONIC_ANCESTRY_RED";
/// Imported Dragonborn whose ancestry select offers the SRD 5.2.1 options.
const SAME_OPTIONS: &str = "ID_T_SPECIES_DRAGONBORN_SAME";
/// Imported Dragonborn whose ancestry select offers its own "Red Dragon" (another id).
const OWN_OPTIONS: &str = "ID_T_SPECIES_DRAGONBORN_OWN";
const OWN_RED: &str = "ID_T_OPTION_RED_DRAGON";
/// Imported copy that declares itself equivalent to the SRD 5.2.1 Dragonborn.
const TWIN: &str = "ID_T_SPECIES_DRAGONBORN_TWIN";
const ELF: &str = "ID_2024_SPECIES_ELF";

fn species(id: &str, trait_id: &str) -> Value {
    json!({"id": id, "name": "Dragonborn", "type": "Species", "source": "Test Imports",
        "rules": [{"kind": "grant", "id": trait_id, "type": "Racial Trait"}]})
}

fn ancestry_trait(id: &str, supports: &str) -> Value {
    json!({"id": id, "name": "Draconic Ancestry", "type": "Racial Trait", "source": "Test Imports",
        "rules": [{"kind": "select", "type": "Species Trait Option", "name": "Draconic Ancestry",
            "supports": supports}]})
}

fn engine() -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut twin = species(TWIN, "ID_2024_TRAIT_DRAGONBORN_DRACONIC_ANCESTRY");
    twin["maps_to"] = json!({"dnd2024": SRD24, "kind": "equivalent"});
    let elements = vec![
        species(SAME_OPTIONS, "ID_T_TRAIT_SAME"),
        ancestry_trait("ID_T_TRAIT_SAME", "Draconic Ancestry"),
        species(OWN_OPTIONS, "ID_T_TRAIT_OWN"),
        ancestry_trait("ID_T_TRAIT_OWN", "Test Ancestry"),
        json!({"id": OWN_RED, "name": "Red  Dragon", "type": "Species Trait Option",
            "source": "Test Imports", "supports": ["Test Ancestry"]}),
        twin,
    ];
    let file: ContentFile = serde_json::from_value(json!({
        "format": "borealis-content/1", "ruleset": "dnd2024", "elements": elements}))
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1")))
        .expect("bundled packs")
        .add_file("imports.json", PackOrigin::Custom, file)
        .build();
    Engine::new(&store)
}

fn slot<'s>(snap: &'s CharacterSnapshot, name: &str) -> &'s SlotSnapshot {
    snap.slots
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no slot {name}"))
}

fn species_key(e: &Engine, s: &CharacterState) -> SlotKey {
    let snap = e.derive(s);
    snap.slots
        .iter()
        .find(|x| x.ty == "Race" || x.ty == "Species")
        .expect("species slot")
        .key
        .clone()
}

/// A 2024 character with the SRD 5.2.1 Dragonborn and the red ancestry.
fn red_dragonborn(e: &Engine) -> CharacterState {
    let s = commands::new_character(e, Ruleset::Dnd2024);
    let key = species_key(e, &s);
    let s = commands::select(e, &s, &key, Choice::Element(SRD24.into()))
        .expect("dragonborn")
        .state;
    let ancestry = slot(&e.derive(&s), "Draconic Ancestry").key.clone();
    commands::select(e, &s, &ancestry, Choice::Element(RED.into()))
        .expect("red")
        .state
}

fn chosen(snap: &CharacterSnapshot, name: &str) -> Option<String> {
    slot(snap, name).chosen.as_ref().map(|c| c.id.clone())
}

#[test]
fn a_version_offering_the_same_option_keeps_the_sub_choice() {
    let e = engine();
    let s = red_dragonborn(&e);
    let key = species_key(&e, &s);
    let out = commands::switch_version(&e, &s, &key, SAME_OPTIONS).expect("switch");
    assert!(out.dropped.is_empty(), "{:?}", out.dropped);
    let snap = e.derive(&out.outcome.state);
    assert_eq!(chosen(&snap, "Draconic Ancestry").as_deref(), Some(RED));
    let species = snap
        .slots
        .iter()
        .find(|x| x.key == key)
        .expect("same slot key");
    assert_eq!(
        species.chosen.as_ref().map(|c| c.id.as_str()),
        Some(SAME_OPTIONS)
    );
}

#[test]
fn an_option_of_the_same_name_counts_as_the_same_sub_choice() {
    let e = engine();
    let s = red_dragonborn(&e);
    let key = species_key(&e, &s);
    let out = commands::switch_version(&e, &s, &key, OWN_OPTIONS).expect("switch");
    assert!(out.dropped.is_empty(), "{:?}", out.dropped);
    let snap = e.derive(&out.outcome.state);
    assert_eq!(chosen(&snap, "Draconic Ancestry").as_deref(), Some(OWN_RED));
}

#[test]
fn a_sub_choice_the_new_version_cannot_take_is_dropped_and_reported() {
    let e = engine();
    let s = red_dragonborn(&e);
    let key = species_key(&e, &s);
    let out = commands::switch_version(&e, &s, &key, SRD14).expect("switch to 2014");
    assert_eq!(out.dropped, vec!["Draconic Ancestry".to_string()]);
    let snap = e.derive(&out.outcome.state);
    assert_eq!(chosen(&snap, "Draconic Ancestry"), None);
    assert!(
        !out.outcome
            .state
            .selections
            .iter()
            .any(|x| x.choice == Choice::Element(RED.into())),
        "the 2024 option is gone"
    );
}

#[test]
fn switching_back_and_forth_keeps_the_choice_where_it_fits() {
    let e = engine();
    let s = red_dragonborn(&e);
    let key = species_key(&e, &s);
    let there = commands::switch_version(&e, &s, &key, SAME_OPTIONS).expect("there");
    let back = commands::switch_version(&e, &there.outcome.state, &key, SRD24).expect("back");
    assert!(back.dropped.is_empty(), "{:?}", back.dropped);
    assert_eq!(
        chosen(&e.derive(&back.outcome.state), "Draconic Ancestry").as_deref(),
        Some(RED)
    );
}

#[test]
fn an_element_of_another_name_is_refused_with_a_clear_message() {
    let e = engine();
    let s = red_dragonborn(&e);
    let key = species_key(&e, &s);
    let err = commands::switch_version(&e, &s, &key, ELF).expect_err("elf is no version");
    let CommandError::Version(message) = &err else {
        panic!("expected a version error, got {err:?}");
    };
    assert!(
        message.contains("Elf") && message.contains("Dragonborn"),
        "{message}"
    );
    assert!(matches!(
        commands::switch_version(&e, &s, &key, "ID_T_NO_SUCH_ELEMENT"),
        Err(CommandError::UnknownElement(_))
    ));
}

#[test]
fn an_empty_slot_has_no_version_to_switch() {
    let e = engine();
    let s = commands::new_character(&e, Ruleset::Dnd2024);
    let key = species_key(&e, &s);
    assert!(matches!(
        commands::switch_version(&e, &s, &key, SRD24),
        Err(CommandError::Version(_))
    ));
}

#[test]
fn an_equivalent_twin_is_a_version_while_the_pick_is_held() {
    let e = engine();
    let s = red_dragonborn(&e);
    let key = species_key(&e, &s);
    let snap = e.derive(&s);
    let listed = snap.slots.iter().find(|x| x.key == key).expect("slot");
    assert!(
        !listed.options.iter().any(|o| o.id == TWIN),
        "the duplicate check hides the twin from the plain option list"
    );
    let versions: Vec<String> = commands::pick_versions(&e, &s, &key)
        .expect("versions")
        .into_iter()
        .map(|o| o.id)
        .collect();
    for id in [SRD24, SRD14, SAME_OPTIONS, OWN_OPTIONS, TWIN] {
        assert!(versions.contains(&id.to_string()), "{id} in {versions:?}");
    }
    assert!(!versions.contains(&ELF.to_string()), "{versions:?}");
    let out = commands::switch_version(&e, &s, &key, TWIN).expect("twin accepted");
    assert!(out.dropped.is_empty(), "{:?}", out.dropped);
    let after = e.derive(&out.outcome.state);
    assert_eq!(chosen(&after, "Draconic Ancestry").as_deref(), Some(RED));
}

#[test]
fn switching_to_the_current_pick_changes_nothing() {
    let e = engine();
    let s = red_dragonborn(&e);
    let key = species_key(&e, &s);
    let out = commands::switch_version(&e, &s, &key, SRD24).expect("same");
    assert_eq!(out.outcome.state, s);
    assert!(out.dropped.is_empty());
}
