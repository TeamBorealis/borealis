//! Regression: bundled 2024 armor once added Dex (and the medium cap) on top of the core
//! armored-AC rules, so a Dex 16 character in Scale Mail had AC 20 instead of 16.

use borealis_content::{ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{Choice, Engine};
use std::path::Path;

fn store() -> borealis_content::ContentStore {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut b = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        b = b.add_pack_dir(&root.join(pack)).expect("pack loads");
    }
    b.build()
}

fn ac_in(e: &Engine, ruleset: Ruleset, class: &str, armor: Option<&str>) -> i32 {
    let s = commands::new_character(e, ruleset);
    let key = slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    let mut s = commands::select(e, &s, &key, Choice::Element(class.into()))
        .unwrap()
        .state;
    s = commands::set_base_scores(e, &s, [10, 16, 14, 10, 12, 8])
        .unwrap()
        .state;
    if let Some(armor) = armor {
        s = commands::add_item(e, &s, armor, 1).unwrap().state;
        let uid = s.inventory.items.last().unwrap().uid.clone();
        s = commands::equip(e, &s, &uid, None).unwrap().state;
    }
    e.derive(&s).armor_class
}

#[test]
fn dex_counts_once_and_medium_armor_caps_it_in_both_editions() {
    let store = store();
    let e = Engine::new(&store);
    let f24 = |armor| ac_in(&e, Ruleset::Dnd2024, "ID_2024_CLASS_FIGHTER", armor);
    assert_eq!(f24(None), 13);
    assert_eq!(f24(Some("ID_2024_ARMOR_LEATHER_ARMOR")), 11 + 3);
    assert_eq!(f24(Some("ID_2024_ARMOR_SCALE_MAIL")), 14 + 2);
    assert_eq!(f24(Some("ID_2024_ARMOR_PLATE_ARMOR")), 18);
    let f14 = |armor| ac_in(&e, Ruleset::Dnd2014, "ID_WOTC_PHB_CLASS_FIGHTER", armor);
    assert_eq!(f14(Some("ID_WOTC_ARMOR_MEDIUM_SCALE_MAIL")), 14 + 2);
    assert_eq!(f14(Some("ID_WOTC_ARMOR_LIGHT_LEATHER")), 11 + 3);
}
