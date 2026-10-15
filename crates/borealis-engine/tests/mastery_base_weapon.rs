//! Weapon Mastery covers magic weapons of a mastered kind: a weapon whose `data.base_weapon` is
//! the picked weapon, or a weapon of the same name from another pack, is mastered too.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::state::CharacterState;
use borealis_engine::{Choice, Engine};
use serde_json::{json, Value};
use std::path::Path;

const GREATSWORD: &str = "ID_2024_WEAPON_GREATSWORD";

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

/// A copy of the bundled Greatsword under another id and name, based on `base`.
fn variant(bundled: &Value, id: &str, name: &str, base: &str) -> Value {
    let mut v = bundled.clone();
    v["id"] = json!(id);
    v["name"] = json!(name);
    v["data"]["base_weapon"] = json!(base);
    v
}

fn engine() -> Engine {
    let bundled = ContentStoreBuilder::new()
        .add_pack_dir(&root().join("srd-5.2.1"))
        .expect("pack")
        .build();
    let sword =
        serde_json::to_value(&bundled.get(GREATSWORD).expect("greatsword").element).unwrap();
    let mut other = sword.clone();
    other["id"] = json!("ID_T_WEAPON_GREATSWORD");
    let file: ContentFile = serde_json::from_value(json!({
    "format": "borealis-content/1", "ruleset": "dnd2024",
    "elements": [
        other,
        variant(&sword, "ID_T_PLUS1_GREATSWORD", "Glimmer Greatsword", GREATSWORD),
        variant(&sword, "ID_T_PLUS2_GREATSWORD", "Ember Greatsword", "ID_T_WEAPON_GREATSWORD"),
    ]}))
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root().join("core"))
        .and_then(|b| b.add_pack_dir(&root().join("srd-5.2.1")))
        .expect("packs")
        .add_file("synthetic.json", PackOrigin::Custom, file)
        .build();
    Engine::new(&store)
}

fn pick(e: &Engine, s: &CharacterState, owner: &str, name: &str, id: &str) -> CharacterState {
    let snap = e.derive(s);
    let key = slot_key(&snap, owner, name, 1).unwrap_or_else(|| panic!("slot {name}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .expect("select")
        .state
}

fn equip(e: &Engine, s: &CharacterState, id: &str) -> CharacterState {
    let s = commands::add_item(e, s, id, 1).expect("add").state;
    let uid = s.inventory.items.last().expect("item").uid.clone();
    commands::equip(e, &s, &uid, None).expect("equip").state
}

#[test]
fn a_magic_weapon_of_the_mastered_kind_is_mastered() {
    let e = engine();
    let mut s = commands::new_character(&e, Ruleset::Dnd2024);
    s = pick(&e, &s, "ID_LEVEL_1", "Race", "ID_2024_SPECIES_DWARF");
    s = pick(&e, &s, "ID_LEVEL_1", "Class", "ID_2024_CLASS_FIGHTER");
    let mastered = |s: &CharacterState, name: &str| {
        e.derive(s)
            .attacks
            .iter()
            .find(|a| a.name == name)
            .and_then(|a| a.mastery.as_ref())
            .map(|m| m.mastered)
    };
    // two-handed: each equip replaces the weapon in hand
    s = equip(&e, &s, "ID_T_PLUS1_GREATSWORD");
    assert_eq!(mastered(&s, "Glimmer Greatsword"), Some(false));
    s = pick(
        &e,
        &s,
        "ID_2024_CLASS_FEATURE_FIGHTER_WEAPON_MASTERY",
        "Weapon Mastery",
        GREATSWORD,
    );
    assert_eq!(mastered(&s, "Glimmer Greatsword"), Some(true), "base id");
    s = equip(&e, &s, "ID_T_PLUS2_GREATSWORD");
    assert_eq!(
        mastered(&s, "Ember Greatsword"),
        Some(true),
        "a base weapon of the same name from another pack"
    );
}
