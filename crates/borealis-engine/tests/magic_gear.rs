//! Magic shields add their enhancement on top of the shield's own +2, and magic items with no
//! equip slot (rods, many wondrous items) can be put to use and attuned so their rules apply.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::Engine;
use serde_json::json;
use std::path::Path;

fn engine() -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let file: ContentFile = serde_json::from_value(json!({
    "format": "borealis-content/1", "ruleset": "dnd2014", "elements": [
        {"id": "ID_T_MAGIC_ITEM_SPIDER_ROD", "name": "Spider Rod", "type": "Magic Item",
         "source": "Test", "setters": [{"name": "category", "value": "Rods"},
            {"name": "type", "value": "Rod"}, {"name": "attunement", "value": "true"},
            {"name": "rarity", "value": "Rare"}],
         "rules": [{"kind": "stat", "name": "ac:misc", "value": "1"}]},
        {"id": "ID_T_MAGIC_ITEM_LUCKY_STONE", "name": "Lucky Stone", "type": "Magic Item",
         "source": "Test", "setters": [{"name": "category", "value": "Wondrous Items"},
            {"name": "type", "value": "Wondrous item"}, {"name": "rarity", "value": "Common"}],
         "rules": [{"kind": "stat", "name": "ac:misc", "value": "1"}]}
    ]}))
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .expect("bundled packs")
        .add_file("gear.json", PackOrigin::Custom, file)
        .build();
    Engine::new(&store)
}

fn fighter(e: &Engine) -> CharacterState {
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
    commands::set_base_scores(e, &s, [16, 10, 14, 10, 12, 8])
        .unwrap()
        .state
}

fn wear(e: &Engine, s: &CharacterState, item: &str, adorner: Option<&str>) -> CharacterState {
    let mut s = commands::add_item(e, s, item, 1).unwrap().state;
    let uid = s.inventory.items.last().unwrap().uid.clone();
    if adorner.is_some() {
        s = commands::set_adorner(e, &s, &uid, adorner).unwrap().state;
    }
    commands::equip(e, &s, &uid, None).expect("equips").state
}

#[test]
fn a_magic_shield_adds_its_enhancement_to_the_shield() {
    let e = engine();
    let s = wear(&e, &fighter(&e), "ID_WOTC_ARMOR_HEAVY_CHAIN_MAIL", None);
    assert_eq!(e.derive(&s).armor_class, 16);
    let plain = wear(&e, &s, "ID_WOTC_GEAR_SHIELD", None);
    assert_eq!(e.derive(&plain).armor_class, 18);
    let magic = wear(
        &e,
        &s,
        "ID_WOTC_GEAR_SHIELD",
        Some("ID_WOTC_DMG_MAGIC_ITEM_SHIELD_2"),
    );
    assert_eq!(e.derive(&magic).armor_class, 20);
}

#[test]
fn slotless_magic_items_can_be_used_and_attuned() {
    let e = engine();
    let s = fighter(&e);
    let base = e.derive(&s).armor_class;
    let rod = wear(&e, &s, "ID_T_MAGIC_ITEM_SPIDER_ROD", None);
    let snap = e.derive(&rod);
    assert_eq!(snap.inventory.attuned, 1, "using it attunes");
    assert_eq!(snap.armor_class, base + 1);
    let stone = wear(&e, &s, "ID_T_MAGIC_ITEM_LUCKY_STONE", None);
    assert_eq!(e.derive(&stone).armor_class, base + 1);
}

#[test]
fn a_magic_shield_adds_its_enhancement_without_body_armor() {
    let e = engine();
    let s = fighter(&e);
    let plain = e
        .derive(&wear(&e, &s, "ID_WOTC_GEAR_SHIELD", None))
        .armor_class;
    let magic = wear(
        &e,
        &s,
        "ID_WOTC_GEAR_SHIELD",
        Some("ID_WOTC_DMG_MAGIC_ITEM_SHIELD_1"),
    );
    assert_eq!(e.derive(&magic).armor_class, plain + 1);
}

/// User report (1.0.0): Attune said "equip the item first" on an Animated Shield whose details offered no Equip.
/// The snapshot now tells the UI which lines can be equipped.
#[test]
fn the_snapshot_says_which_lines_can_be_equipped() {
    let e = engine();
    let mut s = fighter(&e);
    for item in [
        "ID_WOTC_DMG_MAGIC_ITEM_ANIMATED_SHIELD",
        "ID_WOTC_PHB_ITEM_ROPE_HEMPEN_50FEET",
    ] {
        s = commands::add_item(&e, &s, item, 1).unwrap().state;
    }
    let snap = e.derive(&s);
    let line = |id: &str| {
        snap.inventory
            .items
            .iter()
            .find(|l| l.item.id == id)
            .unwrap()
    };
    assert!(line("ID_WOTC_DMG_MAGIC_ITEM_ANIMATED_SHIELD").equippable);
    assert!(!line("ID_WOTC_PHB_ITEM_ROPE_HEMPEN_50FEET").equippable);
    let uid = line("ID_WOTC_DMG_MAGIC_ITEM_ANIMATED_SHIELD").uid.clone();
    let worn = commands::equip(&e, &s, &uid, None).expect("equips").state;
    let snap = e.derive(&worn);
    assert_eq!(snap.inventory.attuned, 1, "equipping attunes it");
    assert_eq!(snap.armor_class, 12, "10 + Dex 0 + shield 2");
}
