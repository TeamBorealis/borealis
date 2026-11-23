//! Infusions on the parts of an item a feature splits into parts (`infusion_items`).
//!
//! Made-up content: a rig grants "Tinkering" (`item infusions:max` +2), "Plating Parts" (the
//! plating "counts as separate items for your infusions: armor, boots, helmet, mantle, and the
//! plating's special weapon", +2 more) and "Spark Cannon" (a feature that counts as a simple ranged
//! weapon), and offers infusion picks: "Keen Edge" (+1 weapon), "Hardened Plating" (+1 armor)
//! and "Striding Boots" (a worn infusion item for the feet). "Plating" is a nonmagical magic
//! item that turns armor into the special armor.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::diag::code;
use borealis_engine::feature_weapon;
use borealis_engine::infusion_items::{self, reconcile, Mode};
use borealis_engine::state::{CharacterState, Choice, SlotKey};
use borealis_engine::{Engine, EngineSettings};
use serde_json::json;
use std::path::Path;

const RIG: &str = "ID_T_MAGIC_ITEM_TINKER_RIG";
const CANNON: &str = "ID_T_FEATURE_SPARK_CANNON";
const KEEN: &str = "ID_T_FEATURE_KEEN_EDGE";
const HARDENED: &str = "ID_T_FEATURE_HARDENED_PLATING";
const BOOTS: &str = "ID_T_FEATURE_STRIDING_BOOTS";
const MANTLE: &str = "ID_T_FEATURE_DAWN_MANTLE";
const MANTLE_ITEM: &str = "ID_T_MAGIC_ITEM_DAWN_MANTLE";
const BOOTS_ITEM: &str = "ID_T_MAGIC_ITEM_STRIDING_BOOTS";
const PLATING: &str = "ID_T_MAGIC_ITEM_PLATING";
const WEAPON_1: &str = "ID_WOTC_DMG_MAGIC_ITEM_WEAPON_1";
const ARMOR_1: &str = "ID_WOTC_DMG_MAGIC_ITEM_ARMOR_1";
const CHAIN_MAIL: &str = "ID_WOTC_ARMOR_HEAVY_CHAIN_MAIL";

fn pick_feature(id: &str, name: &str, text: &str) -> serde_json::Value {
    json!({"id": id, "name": name, "type": "Class Feature", "source": "Test",
        "supports": ["Tinker Infusion"], "sheet": {"descriptions": [{"text": text}]}})
}

fn engine() -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let file: ContentFile = serde_json::from_value(json!({
    "format": "borealis-content/1", "ruleset": "dnd2014", "elements": [
        {"id": RIG, "name": "Tinker Rig", "type": "Magic Item", "source": "Test",
         "setters": [{"name": "category", "value": "Wondrous Items"}, {"name": "slot", "value": "neck"}],
         "rules": [
            {"kind": "grant", "type": "Class Feature", "id": "ID_T_FEATURE_TINKERING"},
            {"kind": "grant", "type": "Class Feature", "id": "ID_T_FEATURE_PLATING_PARTS"},
            {"kind": "grant", "type": "Class Feature", "id": CANNON},
            {"kind": "select", "type": "Class Feature", "name": "Tinker Infusion",
             "supports": "Tinker Infusion", "number": 4}]},
        {"id": "ID_T_FEATURE_TINKERING", "name": "Tinkering", "type": "Class Feature", "source": "Test",
         "rules": [{"kind": "stat", "name": "item infusions:max", "value": "2"}]},
        {"id": "ID_T_FEATURE_PLATING_PARTS", "name": "Plating Parts", "type": "Class Feature", "source": "Test",
         "sheet": {"descriptions": [{"text": "Your plating now counts as separate items for your infusions: \
            armor, boots, helmet, mantle, and the plating's special weapon."}]},
         "rules": [{"kind": "stat", "name": "item infusions:max", "value": "2"}]},
        {"id": CANNON, "name": "Spark Cannon", "type": "Class Feature", "source": "Test",
         "sheet": {"descriptions": [{"text": "A node counts as a simple ranged weapon, with a normal range \
            of 60 feet and a long range of 240 feet, and it deals 1d6 lightning damage on a hit."}]}},
        pick_feature(KEEN, "Keen Edge", "You can infuse a simple or martial weapon into a +1 weapon."),
        pick_feature(HARDENED, "Hardened Plating", "You can infuse a suit of armor into a +1 armor."),
        pick_feature(BOOTS, "Striding Boots", "A pair of boots becomes Striding Boots."),
        pick_feature(MANTLE, "Dawn Mantle", "A cloak becomes a Dawn Mantle."),
        {"id": MANTLE_ITEM, "name": "Dawn Mantle", "type": "Magic Item", "source": "Test",
         "setters": [{"name": "category", "value": "Wondrous Items"}, {"name": "slot", "value": "misc"},
            {"name": "keywords", "value": "tinker infusion"}]},
        {"id": "ID_T_MAGIC_ITEM_MANTLE_OF_EASE", "name": "Mantle of Ease", "type": "Magic Item", "source": "Test",
         "setters": [{"name": "category", "value": "Wondrous Items"}, {"name": "slot", "value": "shoulders"}]},
        {"id": BOOTS_ITEM, "name": "Striding Boots", "type": "Magic Item", "source": "Test",
         "setters": [{"name": "category", "value": "Wondrous Items"}, {"name": "slot", "value": "misc"},
            {"name": "keywords", "value": "tinker infusion"}]},
        {"id": PLATING, "name": "Plating", "type": "Magic Item", "source": "Test",
         "setters": [{"name": "category", "value": "Magic Armor"}, {"name": "slot", "value": "body"},
            {"name": "type", "value": "Armor"},
            {"name": "armor", "value": "ID_INTERNAL_ARMOR_GROUP_MEDIUM|ID_INTERNAL_ARMOR_GROUP_HEAVY"},
            {"name": "name-format", "value": "Plated {{parent}}"}]}
    ]}))
    .expect("made-up content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .expect("bundled packs")
        .add_file("tinker.json", PackOrigin::Custom, file)
        .build();
    Engine::with_settings(
        &store,
        EngineSettings {
            feature_weapons: true,
            infusion_items: true,
            ..EngineSettings::default()
        },
    )
}

/// A level 1 fighter wearing the rig and plated chain mail.
fn tinker(e: &Engine) -> (CharacterState, String) {
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
    let s = commands::equip(e, &s, &uid, None).unwrap().state;
    let s = commands::add_adorned_item(e, &s, CHAIN_MAIL, PLATING, 1)
        .unwrap()
        .state;
    let armor = s.inventory.items.last().unwrap().uid.clone();
    (commands::equip(e, &s, &armor, None).unwrap().state, armor)
}

fn slot(e: &Engine, s: &CharacterState, n: usize) -> SlotKey {
    e.derive(s)
        .slots
        .into_iter()
        .filter(|sl| sl.name == "Tinker Infusion")
        .nth(n)
        .expect("infusion slot")
        .key
}

fn pick(e: &Engine, s: &CharacterState, n: usize, id: &str) -> CharacterState {
    let key = slot(e, s, n);
    let s = commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap()
        .state;
    reconcile(e, &s, Mode::Change).unwrap_or(s)
}

fn cannon() -> String {
    format!(
        "{}{}",
        feature_weapon::ID_PREFIX,
        CANNON.strip_prefix("ID_").unwrap()
    )
}

fn line<'a>(s: &'a CharacterState, uid: &str) -> &'a borealis_engine::state::InventoryItem {
    s.inventory.items.iter().find(|i| i.uid == uid).unwrap()
}

#[test]
fn parts_each_take_one_legal_infusion_and_the_extra_slots_go_on_parts() {
    let e = engine();
    let (s, armor) = tinker(&e);
    let ac = e.derive(&s).armor_class;
    let inf = e.derive(&s).inventory.infusions.expect("summary");
    assert_eq!((inf.used, inf.max, inf.parts_bonus), (0, 4, 2));
    assert_eq!(
        inf.parts,
        ["armor", "boots", "helmet", "mantle", "special weapon"]
    );

    // an armor infusion is layered on the special armor, as its armor part
    let s = pick(&e, &s, 0, HARDENED);
    let snap = e.derive(&s);
    let p = snap
        .inventory
        .infusion_prompts
        .iter()
        .find(|p| p.infusion == "Hardened Plating")
        .unwrap();
    assert_eq!(p.targets.len(), 1);
    assert_eq!(
        (p.targets[0].uid.as_str(), p.targets[0].part.as_str()),
        (armor.as_str(), "armor")
    );
    let s = infusion_items::layer(&e, &s, &p.link, ARMOR_1, &armor).expect("layered");
    let snap = e.derive(&s);
    assert_eq!(
        snap.armor_class,
        ac + 1,
        "the +1 applies to the special armor"
    );
    assert_eq!(line(&s, &armor).part.as_deref(), Some("armor"));
    assert!(reconcile(&e, &s, Mode::Change).is_none());

    // a weapon infusion on the feature's weapon takes the special weapon part
    let s = pick(&e, &s, 1, KEEN);
    let link = e.derive(&s).inventory.infusion_prompts[0].link.clone();
    let mut s = commands::add_adorned_item(&e, &s, &cannon(), WEAPON_1, 1)
        .unwrap()
        .state;
    let gun = s.inventory.items.last().unwrap().uid.clone();
    s.inventory.items.last_mut().unwrap().infusion_link = Some(link);
    let s = infusion_items::place(&e, &s, &gun);
    assert_eq!(line(&s, &gun).part.as_deref(), Some("special weapon"));
    let row = e
        .derive(&s)
        .attacks
        .into_iter()
        .find(|a| a.uid == gun)
        .unwrap();
    assert_eq!(row.name, "Spark Cannon +1");

    // a worn infusion item takes the part its slot fits
    let s = pick(&e, &s, 2, BOOTS);
    let boots = s
        .inventory
        .items
        .iter()
        .find(|i| i.item == BOOTS_ITEM)
        .unwrap()
        .uid
        .clone();
    assert_eq!(line(&s, &boots).part.as_deref(), Some("boots"));

    // one per part, and only a part the item fits
    assert!(
        infusion_items::set_part(&e, &s, &gun, Some("boots")).is_err(),
        "taken"
    );
    let freed = infusion_items::set_part(&e, &s, &boots, None).unwrap();
    assert!(
        infusion_items::set_part(&e, &freed, &gun, Some("boots")).is_err(),
        "a weapon is not boots"
    );
    assert!(
        infusion_items::set_part(&e, &freed, &boots, Some("helmet")).is_err(),
        "not a helmet by its name or slot"
    );
    assert!(
        infusion_items::set_part(&e, &freed, &boots, Some("boots")).is_ok(),
        "boots by its name when its slot names no body part"
    );

    let snap = e.derive(&s);
    let inf = snap.inventory.infusions.unwrap();
    assert_eq!(inf.used, 3);
    let codes: Vec<&str> = snap.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert!(!codes.contains(&code::INFUSIONS_OVER_LIMIT), "{codes:?}");

    // three off their parts exceed the two the base maximum allows
    let mut off = s.clone();
    for it in &mut off.inventory.items {
        it.part = None;
    }
    let codes: Vec<String> = e
        .derive(&off)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect();
    assert!(
        codes.iter().any(|c| c == code::INFUSIONS_OVER_LIMIT),
        "{codes:?}"
    );

    // unpicking the armor infusion takes the layer off; the armor stays
    let key = slot(&e, &s, 0);
    let s = commands::clear(&e, &s, &key).unwrap().state;
    let s = reconcile(&e, &s, Mode::Change).expect("layer removed");
    let plated = line(&s, &armor);
    assert!(
        plated.infusion.is_none()
            && plated.part.is_none()
            && plated.adorner.as_deref() == Some(PLATING)
    );
    assert_eq!(e.derive(&s).armor_class, ac);
}

#[test]
fn on_load_a_hand_layered_armor_infusion_is_adopted() {
    let e = engine();
    let (s, armor) = tinker(&e);
    let key = slot(&e, &s, 0);
    let mut s = commands::select(&e, &s, &key, Choice::Element(HARDENED.into()))
        .unwrap()
        .state;
    s.inventory
        .items
        .iter_mut()
        .find(|i| i.uid == armor)
        .unwrap()
        .infusion = Some(ARMOR_1.into());
    let s = reconcile(&e, &s, Mode::Load).expect("adopts the layer");
    assert!(line(&s, &armor).infusion_layer_link.is_some());
    assert!(reconcile(&e, &s, Mode::Load).is_none());
    assert!(e.derive(&s).inventory.infusion_prompts.is_empty());
}

#[test]
fn a_part_takes_the_items_the_content_names_for_it() {
    let e = engine();
    let (s, _) = tinker(&e);
    // no code knows "mantle": the content's "Mantle of Ease" (shoulders) names the part's slot,
    // so a worn item with no body slot fits it by its own name
    let s = pick(&e, &s, 0, MANTLE);
    let s = pick(&e, &s, 1, BOOTS);
    let mantle = s
        .inventory
        .items
        .iter()
        .find(|i| i.item == MANTLE_ITEM)
        .unwrap();
    assert_eq!(mantle.part.as_deref(), Some("mantle"));
    let boots = s
        .inventory
        .items
        .iter()
        .find(|i| i.item == BOOTS_ITEM)
        .unwrap();
    assert_eq!(boots.part.as_deref(), Some("boots"));
    let boots = boots.uid.clone();
    let freed = infusion_items::set_part(&e, &s, &boots, None).unwrap();
    assert!(
        infusion_items::set_part(&e, &freed, &boots, Some("mantle")).is_err(),
        "taken, and boots are not a mantle"
    );
    let mantle = mantle.uid.clone();
    let freed = infusion_items::set_part(&e, &s, &mantle, None).unwrap();
    assert!(infusion_items::set_part(&e, &freed, &boots, Some("mantle")).is_err());
    assert!(infusion_items::set_part(&e, &freed, &mantle, Some("boots")).is_err());
}
