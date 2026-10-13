//! Magic page wiring of selection slots (regression): slot ids are opaque
//! `owner:rule:ordinal`, so a spell slot names its spellcasting class in `class_slot_id`, and a
//! list-rule slot is typed `list` (its options are list item numbers, not element ids).

use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::state::{AppPaths, AppState};
use std::path::PathBuf;

fn app() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    (dir, AppState::new(paths).expect("state"))
}

fn slot_id(snap: &CharacterSnapshot, title: &str) -> String {
    snap.slots
        .iter()
        .find(|s| s.title == title)
        .unwrap_or_else(|| panic!("no slot {title}"))
        .id
        .clone()
}

/// A 2024 Elf Wizard 1 with the Sage background (Seraphine Vale).
fn sage_wizard(app: &AppState) -> CharacterSnapshot {
    let request = NewCharacterRequest {
        name: "Seraphine".into(),
        ruleset: Ruleset::Dnd2024,
        starting_level: 1,
        gender: "f".into(),
        ability_method: AbilityMethodDto::PointBuy,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    let s = app
        .create_character(Ruleset::Dnd2024, &request)
        .expect("create");
    let id = s.id.clone();
    let s = app
        .apply_selection(&id, &slot_id(&s, "Race"), "ID_2024_SPECIES_ELF")
        .expect("species");
    let s = app
        .apply_selection(&id, &slot_id(&s, "Class"), "ID_2024_CLASS_WIZARD")
        .expect("class");
    app.apply_selection(&id, &slot_id(&s, "Background"), "ID_2024_BACKGROUND_SAGE")
        .expect("background")
}

#[test]
fn wizard_spell_slots_name_their_class_and_other_slots_do_not() {
    let (_dir, app) = app();
    let snap = sage_wizard(&app);
    let wizard = snap
        .spellcasting
        .classes
        .iter()
        .find(|c| c.class_name == "Wizard")
        .expect("wizard spellcasting");
    let cantrips: Vec<_> = snap
        .slots
        .iter()
        .filter(|s| s.title.starts_with("Cantrip (Wizard"))
        .collect();
    assert!(!cantrips.is_empty(), "wizard cantrip slots");
    for s in &cantrips {
        assert!(
            !s.id.starts_with(&format!("{}:", wizard.class_slot_id)),
            "slot ids are owner:rule:ordinal, not class-prefixed: {}",
            s.id
        );
        assert_eq!(
            s.class_slot_id.as_deref(),
            Some(wizard.class_slot_id.as_str()),
            "{}",
            s.id
        );
    }
    for s in snap.slots.iter().filter(|s| s.tab != "spells") {
        assert_eq!(s.class_slot_id, None, "non-spell slot {}", s.id);
    }
    // Magic Initiate (from Sage) feeds no listed spellcasting class: the UI falls back to the first class.
    for s in snap
        .slots
        .iter()
        .filter(|s| s.title.starts_with("Magic Initiate"))
    {
        assert_eq!(s.class_slot_id, None, "{}", s.id);
    }
}

#[test]
fn a_cantrip_picked_through_a_real_slot_id_joins_the_wizard_spell_list() {
    let (_dir, app) = app();
    let snap = sage_wizard(&app);
    let slot = snap
        .slots
        .iter()
        .find(|s| s.title.starts_with("Cantrip (Wizard") && s.chosen.is_none())
        .expect("open cantrip slot");
    let options = app
        .get_slot_options(&snap.id, &slot.id, None, Some(1), None)
        .expect("options");
    let pick = &options.items.first().expect("a cantrip option").summary;
    let after = app
        .apply_selection(&snap.id, &slot.id, &pick.id)
        .expect("pick cantrip");
    let wizard = after
        .spellcasting
        .classes
        .iter()
        .find(|c| c.class_name == "Wizard")
        .expect("wizard");
    let entry = wizard
        .spells
        .iter()
        .find(|s| s.id == pick.id)
        .expect("picked cantrip listed");
    assert_eq!(entry.slot_id.as_deref(), Some(slot.id.as_str()));
}

#[test]
fn sage_ability_scores_is_a_list_slot_whose_options_are_not_elements() {
    let (_dir, app) = app();
    let snap = sage_wizard(&app);
    let slot = snap
        .slots
        .iter()
        .find(|s| s.title == "Ability Scores (Sage)")
        .expect("sage ability slot");
    assert_eq!(slot.element_type, "list");
    let options = app
        .get_slot_options(&snap.id, &slot.id, None, None, None)
        .expect("options");
    let first = &options.items.first().expect("an entry").summary;
    assert_eq!(first.ty, "list");
    assert!(
        app.get_element(&first.id).is_err(),
        "list entry ids are not element ids"
    );
    let after = app
        .apply_selection(&snap.id, &slot.id, &first.id)
        .expect("pick entry");
    let chosen = after
        .slots
        .iter()
        .find(|s| s.id == slot.id)
        .and_then(|s| s.chosen.clone())
        .expect("chosen");
    assert_eq!(chosen.ty, "list");
}
