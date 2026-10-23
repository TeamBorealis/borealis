//! An adorned item (a magic item layered on a base armor or weapon) is named by the magic item's
//! `name-format` ("Armor, +1" on Chain Mail is "Chain Mail +1", as in Aurora) and weighs what
//! its base weighs; generic magic armor and weapons need a base of the kind they name.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::item_base::{can_adorn, needs_base};
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::Engine;
use serde_json::json;
use std::path::Path;

const CHAIN_MAIL: &str = "ID_WOTC_ARMOR_HEAVY_CHAIN_MAIL";
const LEATHER: &str = "ID_WOTC_ARMOR_LIGHT_LEATHER";
const LONGSWORD: &str = "ID_WOTC_PHB_WEAPON_LONGSWORD";
const ARMOR_1: &str = "ID_WOTC_DMG_MAGIC_ITEM_ARMOR_1";
const WEAPON_1: &str = "ID_WOTC_DMG_MAGIC_ITEM_WEAPON_1";
const ADAMANTINE: &str = "ID_WOTC_DMG_MAGIC_ITEM_ADAMANTINE_ARMOR";
const KEEN_BLADE: &str = "ID_T_MAGIC_ITEM_KEEN_LONGSWORD";

fn engine() -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let file: ContentFile = serde_json::from_value(json!({
    "format": "borealis-content/1", "ruleset": "dnd2014", "elements": [
        {"id": KEEN_BLADE, "name": "Keen Longsword", "type": "Magic Item", "source": "Test",
         "setters": [{"name": "category", "value": "Magic Weapons"},
            {"name": "type", "value": "Weapon", "attributes": {"addition": "longsword"}},
            {"name": "rarity", "value": "Rare"}, {"name": "weapon", "value": "Longsword"}]}
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
    commands::set_base_scores(e, &s, [16, 12, 14, 10, 12, 8])
        .unwrap()
        .state
}

/// The fighter with `base` adorned by `adorner`, equipped.
fn adorned(e: &Engine, base: &str, adorner: &str) -> CharacterState {
    let s = commands::add_adorned_item(e, &fighter(e), base, adorner, 1)
        .unwrap()
        .state;
    let uid = s.inventory.items.last().unwrap().uid.clone();
    commands::equip(e, &s, &uid, None).expect("equips").state
}

#[test]
fn plus_one_armor_on_chain_mail_is_chain_mail_plus_one_and_weighs_55_lb() {
    let e = engine();
    let snap = e.derive(&adorned(&e, CHAIN_MAIL, ARMOR_1));
    let line = snap.inventory.items.last().unwrap();
    assert_eq!(line.name, "Chain Mail +1");
    assert!((line.weight - 55.0).abs() < 1e-9, "{}", line.weight);
}

#[test]
fn plus_one_weapon_is_named_by_its_format_in_the_inventory_and_the_attacks() {
    let e = engine();
    let snap = e.derive(&adorned(&e, LONGSWORD, WEAPON_1));
    let line = snap.inventory.items.last().unwrap();
    assert_eq!(line.name, "Longsword +1");
    assert!((line.weight - 3.0).abs() < 1e-9, "{}", line.weight);
    let attack = snap
        .attacks
        .iter()
        .find(|a| a.uid == line.uid)
        .expect("attack row");
    assert_eq!(attack.name, "Longsword +1");
}

#[test]
fn an_adorner_without_a_name_format_keeps_both_names() {
    let e = engine();
    let snap = e.derive(&adorned(&e, CHAIN_MAIL, ADAMANTINE));
    assert_eq!(
        snap.inventory.items.last().unwrap().name,
        "Chain Mail (Adamantine Armor)"
    );
}

#[test]
fn a_magic_item_that_names_its_base_weighs_what_the_base_weighs() {
    let e = engine();
    let s = commands::add_item(&e, &fighter(&e), KEEN_BLADE, 1)
        .unwrap()
        .state;
    let snap = e.derive(&s);
    let line = snap.inventory.items.last().unwrap();
    assert_eq!(line.name, "Keen Longsword");
    assert!((line.weight - 3.0).abs() < 1e-9, "{}", line.weight);
}

#[test]
fn generic_magic_armor_needs_a_base_of_the_kind_it_names() {
    let e = engine();
    let uni = e.universe(Ruleset::Dnd2014);
    let get = |id: &str| uni.get(id).unwrap_or_else(|| panic!("{id}"));
    assert!(needs_base(uni, get(ARMOR_1)));
    assert!(needs_base(uni, get(WEAPON_1)));
    assert!(!needs_base(uni, get(KEEN_BLADE)), "it names its one base");
    assert!(!needs_base(uni, get(CHAIN_MAIL)), "a plain item");
    assert!(can_adorn(get(ARMOR_1), get(CHAIN_MAIL)));
    assert!(can_adorn(get(ARMOR_1), get(LEATHER)));
    assert!(
        !can_adorn(get(ARMOR_1), get(LONGSWORD)),
        "armor on a weapon"
    );
    assert!(can_adorn(get(WEAPON_1), get(LONGSWORD)));
    assert!(can_adorn(get(ADAMANTINE), get(CHAIN_MAIL)), "heavy");
    assert!(!can_adorn(get(ADAMANTINE), get(LEATHER)), "light");
    let hide = uni
        .by_type("Armor")
        .find(|b| b.el.name.starts_with("Hide"))
        .expect("hide armor");
    assert!(!can_adorn(get(ADAMANTINE), hide), "medium, but not hide");
}
