//! Weapon Mastery (2024, SRD 5.2.1 class features): the character chooses kinds of weapons, the
//! base Simple/Martial weapons with a mastery property, within each class's limits: Fighter any
//! Simple or Martial weapon; Barbarian Simple or Martial Melee weapons; Paladin, Ranger and Rogue
//! weapons they are proficient with. The engine's Weapon Mastery slots offer exactly those (the
//! Weapon Mastery list is built from them), and `choose_weapon_masteries` puts each weapon in a
//! slot that accepts it. Bundled SRD content only.

use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;

const LONGSWORD: &str = "ID_2024_WEAPON_LONGSWORD";
const BATTLEAXE: &str = "ID_2024_WEAPON_BATTLEAXE";
const GREATAXE: &str = "ID_2024_WEAPON_GREATAXE";
const LONGBOW: &str = "ID_2024_WEAPON_LONGBOW";
const RAPIER: &str = "ID_2024_WEAPON_RAPIER";
const SHORTBOW: &str = "ID_2024_WEAPON_SHORTBOW";
const MAGIC: [&str; 3] = [
    "ID_2024_MAGIC_ITEM_BERSERKER_AXE",
    "ID_2024_MAGIC_ITEM_AMMUNITION_OF_SLAYING",
    "ID_2024_MAGIC_ITEM_AMMUNITION_1_2_OR_3",
];

fn app() -> (tempfile::TempDir, AppState) {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    (dir, AppState::new(paths).expect("state"))
}

fn slot_of(snap: &CharacterSnapshot, title: &str) -> String {
    snap.slots
        .iter()
        .find(|s| s.title == title)
        .unwrap_or_else(|| panic!("no slot {title}"))
        .id
        .clone()
}

fn hero(app: &AppState, class: &str) -> CharacterSnapshot {
    let request = NewCharacterRequest {
        name: "Mira".into(),
        ruleset: Ruleset::Dnd2024,
        starting_level: 1,
        gender: "f".into(),
        ability_method: AbilityMethodDto::Manual,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    let snap = app
        .create_character(Ruleset::Dnd2024, &request)
        .expect("create");
    app.apply_selection(&snap.id, &slot_of(&snap, "Class"), class)
        .expect("class")
}

fn mastery_slots(snap: &CharacterSnapshot) -> Vec<String> {
    snap.slots
        .iter()
        .filter(|s| s.element_type == "weapon" && s.title.to_lowercase().contains("mastery"))
        .map(|s| s.id.clone())
        .collect()
}

/// Every element id a Weapon Mastery slot of the character offers (all rows, all versions).
fn offered(app: &AppState, snap: &CharacterSnapshot) -> BTreeSet<String> {
    let slots = mastery_slots(snap);
    assert!(!slots.is_empty(), "no Weapon Mastery slot");
    let mut out = BTreeSet::new();
    for slot in slots {
        let page = app
            .get_slot_options(&snap.id, &slot, None, None, None)
            .expect("options");
        let page: Value = serde_json::to_value(page).expect("json");
        for row in page["items"].as_array().expect("items") {
            out.insert(row["id"].as_str().expect("id").to_string());
            for v in row["versions"].as_array().into_iter().flatten() {
                out.insert(v["id"].as_str().expect("id").to_string());
            }
        }
    }
    out
}

#[test]
fn a_fighter_chooses_simple_or_martial_weapons_never_magic_items_or_ammunition() {
    let (_d, app) = app();
    let snap = hero(&app, "ID_2024_CLASS_FIGHTER");
    let ids = offered(&app, &snap);
    for w in [LONGSWORD, BATTLEAXE, LONGBOW, SHORTBOW] {
        assert!(ids.contains(w), "{w} missing: {ids:?}");
    }
    for m in MAGIC {
        assert!(!ids.contains(m), "{m} offered");
    }
    assert!(
        ids.iter().all(|i| i.starts_with("ID_2024_WEAPON_")),
        "only base weapons with a mastery property: {ids:?}"
    );
}

#[test]
fn a_barbarian_chooses_melee_weapons_only() {
    let (_d, app) = app();
    let snap = hero(&app, "ID_2024_CLASS_BARBARIAN");
    let ids = offered(&app, &snap);
    assert!(ids.contains(GREATAXE) && ids.contains(LONGSWORD), "{ids:?}");
    assert!(!ids.contains(LONGBOW) && !ids.contains(SHORTBOW), "{ids:?}");
}

#[test]
fn a_rogue_chooses_weapons_it_is_proficient_with() {
    let (_d, app) = app();
    let snap = hero(&app, "ID_2024_CLASS_ROGUE");
    let ids = offered(&app, &snap);
    // Simple weapons, and Martial weapons with the Finesse or Light property.
    assert!(ids.contains(RAPIER) && ids.contains(SHORTBOW), "{ids:?}");
    assert!(
        !ids.contains(LONGSWORD) && !ids.contains(LONGBOW),
        "{ids:?}"
    );
}

#[test]
fn a_weapon_goes_to_a_mastery_slot_that_accepts_it() {
    // Barbarian 1 / Fighter 1: the Barbarian's slots come first and take melee weapons only, so a
    // Longbow must go to a Fighter slot.
    let (_d, app) = app();
    let snap = hero(&app, "ID_2024_CLASS_BARBARIAN");
    let id = snap.id.clone();
    app.set_character_options(
        &id,
        &OptionsPatch {
            multiclassing: Some(true),
            ..OptionsPatch::default()
        },
    )
    .expect("options");
    app.set_base_score(&id, "str", 15).expect("str");
    let s = app.add_multiclass(&id).expect("new multiclass");
    let slot = slot_of(&s, "Multiclass (Level 2)");
    let page: Value = serde_json::to_value(
        app.get_slot_options(&id, &slot, None, None, None)
            .expect("multiclass options"),
    )
    .expect("json");
    let fighter = page["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|r| r["name"] == "Fighter")
        .and_then(|r| r["id"].as_str())
        .expect("Fighter multiclass")
        .to_string();
    let s = app.apply_selection(&id, &slot, &fighter).expect("fighter");
    assert!(
        mastery_slots(&s).len() > 2,
        "the Fighter adds Weapon Mastery slots"
    );
    let out = app
        .choose_weapon_masteries(&id, &[LONGBOW.to_string(), GREATAXE.to_string()])
        .expect("a Fighter slot takes the Longbow");
    let chosen: Vec<&str> = out
        .play
        .masteries
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["weapon"].as_str())
        .collect();
    assert!(
        chosen.contains(&LONGBOW) && chosen.contains(&GREATAXE),
        "{chosen:?}"
    );
}
