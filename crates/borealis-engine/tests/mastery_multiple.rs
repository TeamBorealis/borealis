//! Weapons with more than one Mastery property (RL10, reported by RJBPrime). A weapon lists every
//! property it names, in each content form: Borealis JSON (`data.mastery_ids`), Aurora XML
//! (`supports` of Mastery Property ids, or a `mastery` setter with a comma list). The Weapon
//! Mastery pick, the attack line and the pick list show each property with its text, and a
//! weapon with one property is read exactly as before.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::CharacterState;
use borealis_engine::{Choice, Engine};
use borealis_migrate::{convert_xml, Document};
use serde_json::json;
use std::path::Path;

const FEATURE: &str = "ID_2024_CLASS_FEATURE_FIGHTER_WEAPON_MASTERY";
const THORNGLAIVE: &str = "ID_HB_WEAPON_THORNGLAIVE";
const BRIARSTAFF: &str = "ID_HB_WEAPON_BRIARSTAFF";
const GALECUTTER: &str = "ID_HB_WEAPON_GALECUTTER";

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

/// A homebrew weapon in Borealis's JSON form with two Mastery property ids.
fn json_pack() -> ContentFile {
    serde_json::from_value(json!({
        "format": "borealis-content/1", "ruleset": "dnd2024",
        "elements": [{
            "id": THORNGLAIVE, "name": "Thornglaive", "type": "Weapon", "source": "My Homebrew",
            "ruleset": "dnd2024",
            "supports": ["ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE", "ID_INTERNAL_DAMAGE_TYPE_SLASHING",
                "ID_INTERNAL_WEAPON_PROPERTY_HEAVY", "ID_INTERNAL_WEAPON_PROPERTY_TWOHANDED"],
            "setters": [
                {"name": "category", "value": "Weapons"},
                {"name": "slot", "value": "twohand"},
                {"name": "damage", "value": "1d10", "attributes": {"type": "slashing"}},
                {"name": "mastery", "value": "Topple, Push"}
            ],
            "data": {
                "weapon_category": "Martial Melee", "melee": true,
                "mastery_id": "ID_2024_MASTERY_TOPPLE",
                "mastery_ids": ["ID_2024_MASTERY_TOPPLE", "ID_2024_MASTERY_PUSH"]
            }
        }]
    }))
    .expect("homebrew JSON")
}

/// Homebrew weapons in Aurora's XML form: one names two Mastery properties by `supports` (the
/// Aurora 2024 core ids, `ID_WOTC_PHB24_*`, community content uses), the other by a `mastery` setter.
fn aurora_pack() -> ContentFile {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<elements>
  <element name="Briarstaff" type="Weapon" source="My Homebrew" id="ID_HB_WEAPON_BRIARSTAFF">
    <supports>ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_MELEE, ID_INTERNAL_DAMAGE_TYPE_BLUDGEONING, ID_WOTC_PHB24_WEAPON_PROPERTY_SAP, ID_WOTC_PHB24_WEAPON_PROPERTY_SLOW</supports>
    <description><p>A staff grown from thorned briar.</p></description>
    <setters>
      <set name="category">Weapons</set>
      <set name="damage" type="bludgeoning">1d6</set>
      <set name="slot">onehand</set>
    </setters>
  </element>
  <element name="Galecutter" type="Weapon" source="My Homebrew" id="ID_HB_WEAPON_GALECUTTER">
    <supports>ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE, ID_INTERNAL_DAMAGE_TYPE_SLASHING</supports>
    <description><p>A blade that rides the wind.</p></description>
    <setters>
      <set name="category">Weapons</set>
      <set name="damage" type="slashing">1d8</set>
      <set name="slot">onehand</set>
      <set name="mastery">Push, Vex</set>
    </setters>
  </element>
</elements>"#;
    let conv = convert_xml(xml, "dual-mastery.xml");
    let Some(Document::Content(mut file)) = conv.document else {
        panic!("converted: {:?}", conv.issues)
    };
    file.ruleset = Some(Ruleset::Dnd2024);
    file
}

fn engine() -> Engine {
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root().join("core"))
        .and_then(|b| b.add_pack_dir(&root().join("srd-5.2.1")))
        .expect("packs")
        .add_file("homebrew.json", PackOrigin::Custom, json_pack())
        .add_file("dual-mastery.xml", PackOrigin::Custom, aurora_pack())
        .build();
    Engine::new(&store)
}

fn pick(
    e: &Engine,
    s: &CharacterState,
    owner: &str,
    name: &str,
    n: u32,
    id: &str,
) -> CharacterState {
    let snap = e.derive(s);
    let key = slot_key(&snap, owner, name, n).unwrap_or_else(|| panic!("slot {name} {n}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("select {id}: {err:?}"))
        .state
}

fn equip(e: &Engine, s: &CharacterState, id: &str) -> CharacterState {
    let s = commands::add_item(e, s, id, 1).expect("add").state;
    let uid = s.inventory.items.last().expect("item").uid.clone();
    commands::equip(e, &s, &uid, None).expect("equip").state
}

fn fighter(e: &Engine) -> CharacterState {
    let s = commands::new_character(e, Ruleset::Dnd2024);
    let s = pick(e, &s, "ID_LEVEL_1", "Race", 1, "ID_2024_SPECIES_DWARF");
    pick(e, &s, "ID_LEVEL_1", "Class", 1, "ID_2024_CLASS_FIGHTER")
}

/// Property names of the pick of `weapon`, each with whether it has text.
fn pick_properties(snap: &CharacterSnapshot, weapon: &str) -> Vec<(String, bool)> {
    snap.masteries
        .iter()
        .find(|p| p.weapon == weapon)
        .unwrap_or_else(|| panic!("no pick of {weapon}: {:?}", snap.masteries))
        .properties
        .iter()
        .map(|p| (p.name.clone(), !p.text.is_empty()))
        .collect()
}

fn owned(names: &[&str]) -> Vec<(String, bool)> {
    names.iter().map(|n| ((*n).to_string(), true)).collect()
}

#[test]
fn a_homebrew_weapon_with_two_masteries_lists_both_on_the_pick_and_the_attack() {
    let e = engine();
    let mut s = fighter(&e);
    s = pick(&e, &s, FEATURE, "Weapon Mastery", 1, THORNGLAIVE);
    s = equip(&e, &s, THORNGLAIVE);
    let snap = e.derive(&s);
    assert_eq!(
        pick_properties(&snap, THORNGLAIVE),
        owned(&["Topple", "Push"])
    );
    let topple = &snap.masteries[0].properties[0];
    assert!(topple.text.contains("Prone"), "{topple:?}");
    let attack = snap
        .attacks
        .iter()
        .find(|a| a.name == "Thornglaive")
        .expect("attack");
    let lines: Vec<(&str, bool)> = attack
        .masteries
        .iter()
        .map(|m| (m.name.as_str(), m.mastered))
        .collect();
    assert_eq!(lines, [("Topple", true), ("Push", true)]);
    assert!(attack.masteries[1].text.contains("10 feet"), "{attack:?}");
}

#[test]
fn aurora_xml_names_several_masteries_by_supports_or_a_setter_list() {
    let e = engine();
    let mut s = fighter(&e);
    s = pick(&e, &s, FEATURE, "Weapon Mastery", 1, BRIARSTAFF);
    s = pick(&e, &s, FEATURE, "Weapon Mastery", 2, GALECUTTER);
    let snap = e.derive(&s);
    assert_eq!(pick_properties(&snap, BRIARSTAFF), owned(&["Sap", "Slow"]));
    assert_eq!(pick_properties(&snap, GALECUTTER), owned(&["Push", "Vex"]));
    s = equip(&e, &s, GALECUTTER);
    let snap = e.derive(&s);
    let attack = snap
        .attacks
        .iter()
        .find(|a| a.name == "Galecutter")
        .expect("attack");
    let names: Vec<&str> = attack.masteries.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["Push", "Vex"]);
    assert!(attack.masteries.iter().all(|m| m.mastered));
}

#[test]
fn a_weapon_with_one_mastery_reads_as_before() {
    let e = engine();
    let mut s = fighter(&e);
    s = pick(
        &e,
        &s,
        FEATURE,
        "Weapon Mastery",
        1,
        "ID_2024_WEAPON_GREATSWORD",
    );
    s = equip(&e, &s, "ID_2024_WEAPON_QUARTERSTAFF");
    let snap = e.derive(&s);
    assert_eq!(
        pick_properties(&snap, "ID_2024_WEAPON_GREATSWORD"),
        owned(&["Graze"])
    );
    let staff = snap
        .attacks
        .iter()
        .find(|a| a.name == "Quarterstaff")
        .expect("attack");
    let lines: Vec<(&str, bool)> = staff
        .masteries
        .iter()
        .map(|m| (m.name.as_str(), m.mastered))
        .collect();
    assert_eq!(lines, [("Topple", false)], "one property, not mastered");
}
