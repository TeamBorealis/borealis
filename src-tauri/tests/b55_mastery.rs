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

/// Every element id one slot offers (all rows, all versions).
fn offered_by(app: &AppState, snap: &CharacterSnapshot, slot: &str) -> BTreeSet<String> {
    let page = app
        .get_slot_options(&snap.id, slot, None, None, None)
        .expect("options");
    let page: Value = serde_json::to_value(page).expect("json");
    let mut out = BTreeSet::new();
    for row in page["items"].as_array().expect("items") {
        out.insert(row["id"].as_str().expect("id").to_string());
        for v in row["versions"].as_array().into_iter().flatten() {
            out.insert(v["id"].as_str().expect("id").to_string());
        }
    }
    out
}

#[test]
fn a_weapon_mastered_in_one_slot_is_not_offered_in_another() {
    // A8: two slots that master the same kind of weapon give one mastery, not two. A weapon the
    // character carries is no duplicate: mastery is of a kind of weapon, not of an item.
    let (_d, app) = app();
    let snap = hero(&app, "ID_2024_CLASS_BARBARIAN");
    let snap = app
        .add_item(&snap.id, BATTLEAXE, 1, false, None)
        .expect("a Battleaxe in the inventory");
    let slots = mastery_slots(&snap);
    assert_eq!(slots.len(), 2, "a level 1 Barbarian masters two weapons");
    for slot in &slots {
        assert!(
            offered_by(&app, &snap, slot).contains(BATTLEAXE),
            "a carried Battleaxe blocks no mastery slot"
        );
    }
    let snap = app
        .apply_selection(&snap.id, &slots[0], BATTLEAXE)
        .expect("Battleaxe in the first slot");
    let first = offered_by(&app, &snap, &slots[0]);
    let second = offered_by(&app, &snap, &slots[1]);
    assert!(
        first.contains(BATTLEAXE),
        "the slot's own choice stays listed"
    );
    assert!(
        !second.contains(BATTLEAXE),
        "the second slot offers the Battleaxe again"
    );
    assert!(second.contains(GREATAXE), "{second:?}");
    assert!(
        app.apply_selection(&snap.id, &slots[1], BATTLEAXE).is_err(),
        "the second slot accepts the weapon the first one holds"
    );
    let snap = app
        .apply_selection(&snap.id, &slots[1], GREATAXE)
        .expect("Greataxe in the second slot");
    let first = offered_by(&app, &snap, &slots[0]);
    assert!(
        first.contains(BATTLEAXE) && !first.contains(GREATAXE),
        "the first slot keeps its own choice and drops the other's: {first:?}"
    );
}

#[test]
fn barbarian_mastery_slots_are_required() {
    // A9: Weapon Mastery is a plain required choice (`<select ... name="Weapon Mastery"
    // number="2"/>` in Aurora-format content): each open slot counts as a remaining selection.
    let (_d, app) = app();
    let snap = hero(&app, "ID_2024_CLASS_BARBARIAN");
    let slots = mastery_slots(&snap);
    assert_eq!(slots.len(), 2, "a level 1 Barbarian masters two weapons");
    for slot in snap.slots.iter().filter(|s| slots.contains(&s.id)) {
        assert!(slot.required, "{} is optional", slot.title);
    }
    let before = snap.progress.remaining;
    let snap = app
        .apply_selection(&snap.id, &slots[0], BATTLEAXE)
        .expect("Battleaxe");
    assert_eq!(snap.progress.remaining + 1, before, "the first pick counts");
    let snap = app
        .apply_selection(&snap.id, &slots[1], GREATAXE)
        .expect("Greataxe");
    assert_eq!(
        snap.progress.remaining + 2,
        before,
        "the second pick counts"
    );
}

#[test]
fn weapon_description_names_its_mastery_property() {
    // A7 (tester video 2026-11-02): the weapon's card says which mastery it gives and what it does.
    let (_d, app) = app();
    let axe = app.get_element(BATTLEAXE).expect("Battleaxe");
    let mastery: Vec<&str> = axe
        .facts
        .iter()
        .filter(|f| f.label == "Mastery")
        .map(|f| f.value.as_str())
        .collect();
    assert_eq!(mastery, ["Topple"], "{:?}", axe.facts);
    let topple = app.get_element("ID_2024_MASTERY_TOPPLE").expect("Topple");
    let rule = topple
        .description_html
        .trim()
        .trim_start_matches("<p>")
        .trim_end_matches("</p>")
        .to_string();
    assert!(!rule.is_empty());
    assert!(
        axe.description_html.contains("Mastery: Topple") && axe.description_html.contains(&rule),
        "the card carries the Topple rule: {}",
        axe.description_html
    );
    let magic = app
        .get_element("ID_2024_MAGIC_ITEM_BERSERKER_AXE")
        .expect("magic axe");
    assert!(
        !magic.description_html.contains("Mastery:"),
        "an item with no mastery property names none"
    );
}

#[test]
fn mastery_slots_carry_a_numbered_hint() {
    // A7: the two Weapon Mastery pickers say what the pick is and which of the two each one is.
    let (_d, app) = app();
    let snap = hero(&app, "ID_2024_CLASS_BARBARIAN");
    let ids = mastery_slots(&snap);
    let hints: Vec<String> = snap
        .slots
        .iter()
        .filter(|s| ids.contains(&s.id))
        .map(|s| s.hint.clone().unwrap_or_default())
        .collect();
    assert_eq!(hints.len(), 2);
    for (hint, end) in hints.iter().zip(["(1 of 2)", "(2 of 2)"]) {
        assert!(
            hint.starts_with("Choose a kind of weapon whose mastery property you can use"),
            "{hint}"
        );
        assert!(hint.ends_with(end), "{hint} ends with {end}");
    }
    let others = snap
        .slots
        .iter()
        .filter(|s| !ids.contains(&s.id))
        .filter(|s| s.hint.as_deref().is_some_and(|h| h.contains("mastery")))
        .count();
    assert_eq!(others, 0, "only mastery slots carry the mastery hint");
}
