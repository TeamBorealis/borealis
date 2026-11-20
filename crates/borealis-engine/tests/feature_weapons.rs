//! Weapons a feature counts as (`EngineSettings::feature_weapons`, off by default): a feature whose
//! text says a part of the character "counts as a simple ranged weapon" gets a derived Weapon
//! element that can be wielded, takes a magic weapon's bonus and shows in the attacks table.
//! Made-up content only, on the bundled SRD packs.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::feature_weapon;
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::{Engine, EngineSettings};
use serde_json::json;
use std::path::Path;

const CANNON: &str = "ID_T_FEATURE_SPARK_CANNON";
const FIST: &str = "ID_T_FEATURE_SPARK_FIST";
const RIG: &str = "ID_T_MAGIC_ITEM_SPARK_RIG";
const WEAPON_2: &str = "ID_WOTC_DMG_MAGIC_ITEM_WEAPON_2";
const GLOWING: &str = "ID_T_MAGIC_ITEM_GLOWING_WEAPON";
const ANY_WEAPON: &str = "ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_MELEE||ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_RANGED||\
                          ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE||ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_RANGED";

fn engine(feature_weapons: bool) -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let file: ContentFile = serde_json::from_value(json!({
    "format": "borealis-content/1", "ruleset": "dnd2014", "elements": [
        {"id": CANNON, "name": "Spark Cannon", "type": "Class Feature", "source": "Test",
         "sheet": {"descriptions": [{"text": "A node on your gauntlet counts as a simple ranged \
            weapon, with a normal range of 60 feet and a long range of 240 feet, and it deals \
            1d6 lightning damage on a hit."}]}},
        {"id": FIST, "name": "Spark Fist", "type": "Class Feature", "source": "Test",
         "sheet": {"descriptions": [{"text": "Each gauntlet counts as a martial melee weapon while \
            you hold nothing in it, and it deals 1d8 thunder damage on a hit."}]}},
        {"id": RIG, "name": "Spark Rig", "type": "Magic Item", "source": "Test",
         "setters": [{"name": "category", "value": "Wondrous Items"}, {"name": "slot", "value": "neck"}],
         "rules": [{"kind": "grant", "type": "Class Feature", "id": CANNON},
                   {"kind": "grant", "type": "Class Feature", "id": FIST}]},
        {"id": GLOWING, "name": "Glowing Weapon", "type": "Magic Item", "source": "Test",
         "setters": [{"name": "category", "value": "Magic Weapons"},
            {"name": "type", "value": "Weapon", "attributes": {"addition": "simple or martial weapon"}},
            {"name": "rarity", "value": "Uncommon"}, {"name": "enhancement", "value": "1"},
            {"name": "attunement", "value": "true"}, {"name": "weapon", "value": ANY_WEAPON},
            {"name": "name-format", "value": "Glowing {{parent}} +{{enhancement}}"}]}
    ]}))
    .expect("made-up content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .expect("bundled packs")
        .add_file("spark.json", PackOrigin::Custom, file)
        .build();
    Engine::with_settings(
        &store,
        EngineSettings {
            feature_weapons,
            ..EngineSettings::default()
        },
    )
}

/// A level 1 fighter (Dex 14) wearing the rig, which grants the features.
fn sparky(e: &Engine) -> CharacterState {
    let s = commands::new_character(e, Ruleset::Dnd2014);
    let key = slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    let s = commands::select(
        e,
        &s,
        &key,
        Choice::Element("ID_WOTC_PHB_CLASS_FIGHTER".into()),
    )
    .unwrap()
    .state;
    let s = commands::set_base_scores(e, &s, [16, 14, 14, 10, 12, 8])
        .unwrap()
        .state;
    let s = commands::add_item(e, &s, RIG, 1).unwrap().state;
    let uid = s.inventory.items.last().unwrap().uid.clone();
    commands::equip(e, &s, &uid, None).unwrap().state
}

fn weapon_id(feature: &str) -> String {
    format!(
        "{}{}",
        feature_weapon::ID_PREFIX,
        feature.strip_prefix("ID_").unwrap()
    )
}

/// The attack row of the equipped `weapon`, adorned by `adorner` when given.
fn attack(
    e: &Engine,
    s: &CharacterState,
    weapon: &str,
    adorner: Option<&str>,
) -> borealis_engine::snapshot::AttackSnapshot {
    let s = match adorner {
        Some(a) => commands::add_adorned_item(e, s, weapon, a, 1),
        None => commands::add_item(e, s, weapon, 1),
    }
    .unwrap()
    .state;
    let uid = s.inventory.items.last().unwrap().uid.clone();
    let s = commands::equip(e, &s, &uid, None).unwrap().state;
    e.derive(&s)
        .attacks
        .into_iter()
        .find(|a| a.uid == uid)
        .expect("attack row")
}

#[test]
fn a_feature_that_counts_as_a_weapon_gets_a_weapon_only_with_the_setting() {
    assert!(engine(false)
        .universe(Ruleset::Dnd2014)
        .get(&weapon_id(CANNON))
        .is_none());
    let e = engine(true);
    let w = e
        .universe(Ruleset::Dnd2014)
        .get(&weapon_id(CANNON))
        .expect("derived weapon");
    assert_eq!(
        (w.el.ty.as_str(), w.el.name.as_str()),
        ("Weapon", "Spark Cannon")
    );
    assert!(w.derived);
}

#[test]
fn the_feature_weapon_attacks_with_its_text_damage_and_range() {
    let e = engine(true);
    let s = sparky(&e);
    let cannon = attack(&e, &s, &weapon_id(CANNON), None);
    assert_eq!(
        (cannon.name.as_str(), cannon.range.as_str()),
        ("Spark Cannon", "60/240")
    );
    // Dex +2, proficiency +2
    assert_eq!(
        (cannon.attack, cannon.damage.as_str()),
        (4, "1d6+2 lightning")
    );
    let fist = attack(&e, &s, &weapon_id(FIST), None);
    assert_eq!(fist.damage, "1d8+3 thunder", "a melee weapon uses Strength");
}

#[test]
fn a_feature_weapon_is_proficient_through_its_weapon_category() {
    let e = engine(true);
    let s = sparky(&e);
    assert!(
        attack(&e, &s, &weapon_id(CANNON), None).proficient,
        "a fighter is proficient with simple weapons"
    );
    let w = e.universe(Ruleset::Dnd2014).get(&weapon_id(FIST)).unwrap();
    assert_eq!(
        w.el.setter_value("proficiency"),
        Some("ID_PROFICIENCY_WEAPON_PROFICIENCY_MARTIAL_MELEE_WEAPONS"),
        "the feature alone grants no proficiency; its category does"
    );
}

#[test]
fn a_plus_two_and_a_named_magic_weapon_apply_to_the_feature_weapons_attack() {
    let e = engine(true);
    let s = sparky(&e);
    let plus_two = attack(&e, &s, &weapon_id(CANNON), Some(WEAPON_2));
    assert_eq!(plus_two.name, "Spark Cannon +2");
    assert_eq!(
        (plus_two.attack, plus_two.damage.as_str()),
        (6, "1d6+4 lightning")
    );
    let glowing = attack(&e, &s, &weapon_id(CANNON), Some(GLOWING));
    assert_eq!(glowing.name, "Glowing Spark Cannon +1");
    assert_eq!(glowing.attack, 5);
}
