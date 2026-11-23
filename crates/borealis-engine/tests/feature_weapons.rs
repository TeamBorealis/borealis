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

const SLING: &str = "ID_T_FEATURE_TIDE_SLING";
const MAUL: &str = "ID_T_FEATURE_TIDE_MAUL";
const HARNESS: &str = "ID_T_MAGIC_ITEM_TIDE_HARNESS";
const WEAPON_2: &str = "ID_WOTC_DMG_MAGIC_ITEM_WEAPON_2";
const GLOWING: &str = "ID_T_MAGIC_ITEM_GLOWING_WEAPON";
const ANY_WEAPON: &str = "ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_MELEE||ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_RANGED||\
                          ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE||ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_RANGED";

fn engine(feature_weapons: bool) -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let file: ContentFile = serde_json::from_value(json!({
    "format": "borealis-content/1", "ruleset": "dnd2014", "elements": [
        {"id": SLING, "name": "Tide Sling", "type": "Class Feature", "source": "Test",
         "sheet": {"descriptions": [{"text": "A node on your harness counts as a simple ranged \
            weapon, with a normal range of 40 feet and a long range of 160 feet, and it deals \
            1d4 cold damage on a hit."}]}},
        {"id": MAUL, "name": "Tide Maul", "type": "Class Feature", "source": "Test",
         "sheet": {"descriptions": [{"text": "Each harness counts as a martial melee weapon while \
            you hold nothing in it, and it deals 1d10 force damage on a hit."}]}},
        {"id": HARNESS, "name": "Tide Harness", "type": "Magic Item", "source": "Test",
         "setters": [{"name": "category", "value": "Wondrous Items"}, {"name": "slot", "value": "neck"}],
         "rules": [{"kind": "grant", "type": "Class Feature", "id": SLING},
                   {"kind": "grant", "type": "Class Feature", "id": MAUL}]},
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
        .add_file("tide.json", PackOrigin::Custom, file)
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
fn tidal(e: &Engine) -> CharacterState {
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
    let s = commands::add_item(e, &s, HARNESS, 1).unwrap().state;
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
        .get(&weapon_id(SLING))
        .is_none());
    let e = engine(true);
    let w = e
        .universe(Ruleset::Dnd2014)
        .get(&weapon_id(SLING))
        .expect("derived weapon");
    assert_eq!(
        (w.el.ty.as_str(), w.el.name.as_str()),
        ("Weapon", "Tide Sling")
    );
    assert!(w.derived);
}

#[test]
fn the_feature_weapon_attacks_with_its_text_damage_and_range() {
    let e = engine(true);
    let s = tidal(&e);
    let cannon = attack(&e, &s, &weapon_id(SLING), None);
    assert_eq!(
        (cannon.name.as_str(), cannon.range.as_str()),
        ("Tide Sling", "40/160")
    );
    // Dex +2, proficiency +2
    assert_eq!((cannon.attack, cannon.damage.as_str()), (4, "1d4+2 cold"));
    let fist = attack(&e, &s, &weapon_id(MAUL), None);
    assert_eq!(fist.damage, "1d10+3 force", "a melee weapon uses Strength");
}

#[test]
fn a_feature_weapon_is_proficient_through_its_weapon_category() {
    let e = engine(true);
    let s = tidal(&e);
    assert!(
        attack(&e, &s, &weapon_id(SLING), None).proficient,
        "a fighter is proficient with simple weapons"
    );
    let w = e.universe(Ruleset::Dnd2014).get(&weapon_id(MAUL)).unwrap();
    assert_eq!(
        w.el.setter_value("proficiency"),
        Some("ID_PROFICIENCY_WEAPON_PROFICIENCY_MARTIAL_MELEE_WEAPONS"),
        "the feature alone grants no proficiency; its category does"
    );
}

#[test]
fn a_plus_two_and_a_named_magic_weapon_apply_to_the_feature_weapons_attack() {
    let e = engine(true);
    let s = tidal(&e);
    let plus_two = attack(&e, &s, &weapon_id(SLING), Some(WEAPON_2));
    assert_eq!(plus_two.name, "Tide Sling +2");
    assert_eq!(
        (plus_two.attack, plus_two.damage.as_str()),
        (6, "1d4+4 cold")
    );
    let glowing = attack(&e, &s, &weapon_id(SLING), Some(GLOWING));
    assert_eq!(glowing.name, "Glowing Tide Sling +1");
    assert_eq!(glowing.attack, 5);
}

#[test]
fn turning_the_setting_off_removes_the_derived_weapons() {
    let derived = |e: &Engine| {
        [Ruleset::Dnd2014, Ruleset::Dnd2024]
            .into_iter()
            .map(|r| {
                e.universe(r)
                    .iter()
                    .filter(|u| u.el.id.starts_with(feature_weapon::ID_PREFIX))
                    .count()
            })
            .sum::<usize>()
    };
    assert!(derived(&engine(true)) >= 2, "one weapon per such feature");
    assert_eq!(derived(&engine(false)), 0);
}

#[test]
fn a_saved_feature_weapon_stays_in_the_inventory_when_the_setting_is_turned_off() {
    let on = engine(true);
    let s = commands::add_item(&on, &tidal(&on), &weapon_id(SLING), 1)
        .unwrap()
        .state;
    let saved = serde_json::to_string(&s).expect("save");
    let off = engine(false);
    let loaded: CharacterState = serde_json::from_str(&saved).expect("load");
    assert!(
        loaded
            .inventory
            .items
            .iter()
            .any(|i| i.item == weapon_id(SLING)),
        "the line is kept, so turning the setting on again brings the weapon back"
    );
    let snap = off.derive(&loaded);
    assert!(
        snap.diagnostics.iter().any(|d| {
            d.code == "content.missing_element" && d.message.contains(&weapon_id(SLING))
        }),
        "a clear note says the item is not installed: {:?}",
        snap.diagnostics
    );
}
