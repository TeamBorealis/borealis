//! An infusion pick keeps its magic item in the inventory (`infusion_items`).
//!
//! Made-up content: a rig whose select offers "Tinker Infusion" features. "Striding Boots" has a
//! Magic Item of the same name marked as an infusion (no base); "Glowing Weapon" has one that
//! needs a weapon; "Keen Edge" says it makes a weapon "into a +1 weapon" (the SRD `Weapon, +1`).

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::infusion_items::{reconcile, Mode};
use borealis_engine::state::{CharacterState, Choice, SlotKey};
use borealis_engine::Engine;
use serde_json::json;
use std::path::Path;

const RIG: &str = "ID_T_MAGIC_ITEM_TINKER_RIG";
const BOOTS: &str = "ID_T_FEATURE_STRIDING_BOOTS";
const BOOTS_ITEM: &str = "ID_T_MAGIC_ITEM_STRIDING_BOOTS";
/// A second infusion element of the same name (as when two books print it).
const BOOTS_TOO: &str = "ID_T2_FEATURE_STRIDING_BOOTS";
const GLOWING: &str = "ID_T_FEATURE_GLOWING_WEAPON";
const GLOWING_ITEM: &str = "ID_T_MAGIC_ITEM_GLOWING_WEAPON";
const KEEN: &str = "ID_T_FEATURE_KEEN_EDGE";
const WEAPON_1: &str = "ID_WOTC_DMG_MAGIC_ITEM_WEAPON_1";
const LONGSWORD: &str = "ID_WOTC_PHB_WEAPON_LONGSWORD";

fn feature(id: &str, name: &str, text: &str) -> serde_json::Value {
    json!({"id": id, "name": name, "type": "Class Feature", "source": "Test",
        "supports": ["Tinker Infusion"], "sheet": {"descriptions": [{"text": text}]}})
}

fn engine() -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let file: ContentFile = serde_json::from_value(json!({
    "format": "borealis-content/1", "ruleset": "dnd2014", "elements": [
        {"id": RIG, "name": "Tinker Rig", "type": "Magic Item", "source": "Test",
         "setters": [{"name": "category", "value": "Wondrous Items"}, {"name": "slot", "value": "neck"}],
         "rules": [{"kind": "select", "type": "Class Feature", "name": "Tinker Infusion",
            "supports": "Tinker Infusion", "number": 3}]},
        feature(BOOTS, "Striding Boots", "A pair of boots becomes Striding Boots."),
        feature(BOOTS_TOO, "Striding Boots", "A pair of boots becomes Striding Boots."),
        feature(GLOWING, "Glowing Weapon", "A weapon of your choice glows."),
        feature(KEEN, "Keen Edge", "You can infuse a simple or martial weapon into a +1 weapon."),
        {"id": BOOTS_ITEM, "name": "Striding Boots", "type": "Magic Item", "source": "Test",
         "setters": [{"name": "category", "value": "Wondrous Items"}, {"name": "slot", "value": "feet"},
            {"name": "keywords", "value": "artificer infusion"}]},
        {"id": GLOWING_ITEM, "name": "Glowing Weapon", "type": "Magic Item", "source": "Test",
         "setters": [{"name": "category", "value": "Magic Weapons"},
            {"name": "type", "value": "Weapon", "attributes": {"addition": "simple or martial weapon"}},
            {"name": "rarity", "value": "Artificer Infusion"}, {"name": "enhancement", "value": "1"},
            {"name": "weapon", "value": "ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_MELEE||ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE"},
            {"name": "name-format", "value": "Glowing {{parent}} +{{enhancement}}"}]}
    ]}))
    .expect("made-up content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .expect("bundled packs")
        .add_file("tinker.json", PackOrigin::Custom, file)
        .build();
    Engine::new(&store)
}

/// A level 1 fighter wearing the rig.
fn tinker(e: &Engine) -> CharacterState {
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

fn infusion_slot(e: &Engine, s: &CharacterState, n: u32) -> SlotKey {
    e.derive(s)
        .slots
        .into_iter()
        .filter(|sl| sl.name == "Tinker Infusion")
        .nth(n as usize)
        .expect("infusion slot")
        .key
}

/// Pick `id` in infusion slot `n`, then reconcile as after a change.
fn pick(e: &Engine, s: &CharacterState, n: u32, id: &str) -> CharacterState {
    let key = infusion_slot(e, s, n);
    let s = commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap()
        .state;
    reconcile(e, &s, Mode::Change).unwrap_or(s)
}

fn copies<'a>(s: &'a CharacterState, item: &str) -> Vec<&'a borealis_engine::state::InventoryItem> {
    s.inventory
        .items
        .iter()
        .filter(|i| i.item == item || i.adorner.as_deref() == Some(item))
        .collect()
}

#[test]
fn picking_an_infusion_adds_its_item_and_unpicking_removes_that_copy() {
    let e = engine();
    let s = pick(&e, &tinker(&e), 0, BOOTS);
    let boots = copies(&s, BOOTS_ITEM);
    assert_eq!(boots.len(), 1);
    assert!(boots[0].infusion_link.is_some());
    assert!(boots[0].equipped.is_some(), "worn where it fits");
    assert!(
        reconcile(&e, &s, Mode::Change).is_none(),
        "nothing more to do"
    );
    let key = infusion_slot(&e, &s, 0);
    let s = commands::clear(&e, &s, &key).unwrap().state;
    let s = reconcile(&e, &s, Mode::Change).expect("removes the copy");
    assert!(copies(&s, BOOTS_ITEM).is_empty());
}

#[test]
fn a_new_pick_adds_a_copy_even_when_one_is_owned_and_removal_takes_only_that_one() {
    let e = engine();
    let owned = commands::add_item(&e, &tinker(&e), BOOTS_ITEM, 1)
        .unwrap()
        .state;
    let mine = owned.inventory.items.last().unwrap().uid.clone();
    let s = pick(&e, &owned, 0, BOOTS);
    assert_eq!(copies(&s, BOOTS_ITEM).len(), 2);
    let s = pick(&e, &s, 1, BOOTS_TOO);
    assert_eq!(copies(&s, BOOTS_ITEM).len(), 3, "two picks, two copies");
    let key = infusion_slot(&e, &s, 0);
    let s = commands::clear(&e, &s, &key).unwrap().state;
    let s = reconcile(&e, &s, Mode::Change).unwrap();
    let left = copies(&s, BOOTS_ITEM);
    assert_eq!(left.len(), 2);
    assert!(
        left.iter()
            .any(|i| i.uid == mine && i.infusion_link.is_none()),
        "the owned one is untouched"
    );
}

#[test]
fn an_item_that_needs_a_base_is_a_prompt_until_a_legal_base_is_added_with_the_link() {
    let e = engine();
    let s = pick(&e, &tinker(&e), 0, KEEN);
    let snap = e.derive(&s);
    let p = snap
        .inventory
        .infusion_prompts
        .first()
        .expect("choose the item for Keen Edge");
    assert_eq!(p.infusion, "Keen Edge");
    assert!(p.items.contains(&WEAPON_1.to_string()), "{:?}", p.items);
    assert!(copies(&s, WEAPON_1).is_empty(), "never guessed");
    // the builder adds the item on the chosen base with the prompt's link
    let mut s = commands::add_adorned_item(&e, &s, LONGSWORD, WEAPON_1, 1)
        .unwrap()
        .state;
    let line = s.inventory.items.last_mut().unwrap();
    line.infusion_link = Some(p.link.clone());
    let uid = line.uid.clone();
    let s = borealis_engine::infusion_items::place(&e, &s, &uid);
    let snap = e.derive(&s);
    assert!(snap.inventory.infusion_prompts.is_empty());
    let row = snap.attacks.iter().find(|a| a.uid == uid).expect("wielded");
    assert_eq!(row.name, "Longsword +1");
    // Str +3, proficiency +2, +1
    assert_eq!(row.attack, 6);
    assert!(reconcile(&e, &s, Mode::Change).is_none());
}

#[test]
fn on_load_a_pick_adopts_a_hand_added_item_and_loading_twice_changes_nothing() {
    let e = engine();
    // a pre-update character: the pick and the item, with no link
    let key = infusion_slot(&e, &tinker(&e), 0);
    let s = commands::select(&e, &tinker(&e), &key, Choice::Element(BOOTS.into()))
        .unwrap()
        .state;
    let s = commands::add_item(&e, &s, BOOTS_ITEM, 1).unwrap().state;
    let s = reconcile(&e, &s, Mode::Load).expect("adopts");
    let boots = copies(&s, BOOTS_ITEM);
    assert_eq!(boots.len(), 1);
    assert!(boots[0].infusion_link.is_some());
    assert!(reconcile(&e, &s, Mode::Load).is_none(), "idempotent");

    // the pick without its item: one copy is added
    let bare = commands::select(&e, &tinker(&e), &key, Choice::Element(BOOTS.into()))
        .unwrap()
        .state;
    let s = reconcile(&e, &bare, Mode::Load).expect("adds");
    assert_eq!(copies(&s, BOOTS_ITEM).len(), 1);
    assert!(reconcile(&e, &s, Mode::Load).is_none());

    // a pick whose base is unknown: a prompt, no item
    let glow = commands::select(&e, &tinker(&e), &key, Choice::Element(GLOWING.into()))
        .unwrap()
        .state;
    assert!(reconcile(&e, &glow, Mode::Load).is_none());
    assert_eq!(e.derive(&glow).inventory.infusion_prompts.len(), 1);

    // a hand-added glowing longsword is adopted for it
    let glow = commands::add_adorned_item(&e, &glow, LONGSWORD, GLOWING_ITEM, 1)
        .unwrap()
        .state;
    let s = reconcile(&e, &glow, Mode::Load).expect("adopts the infused weapon");
    assert!(copies(&s, GLOWING_ITEM)[0].infusion_link.is_some());
    assert!(e.derive(&s).inventory.infusion_prompts.is_empty());
}

#[test]
fn a_link_survives_saving_and_an_older_file_without_it_loads() {
    let e = engine();
    let s = pick(&e, &tinker(&e), 0, BOOTS);
    let back: CharacterState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(back, s);
    let mut old = serde_json::to_value(&s).unwrap();
    for it in old["inventory"]["items"].as_array_mut().unwrap() {
        it.as_object_mut().unwrap().remove("infusion_link");
    }
    let old: CharacterState = serde_json::from_value(old).unwrap();
    let s2 = reconcile(&e, &old, Mode::Load).expect("re-links the copy");
    assert_eq!(
        copies(&s2, BOOTS_ITEM).len(),
        1,
        "an older device's save does not double-add"
    );
}

#[test]
fn a_file_saved_before_infusion_items_loads_unchanged_and_saves_without_the_new_fields() {
    let e = engine();
    let s = tinker(&e);
    let s = commands::add_item(&e, &s, "ID_WOTC_PHB_WEAPON_LONGSWORD", 1)
        .unwrap()
        .state;
    let text = serde_json::to_string(&s).unwrap();
    for key in [
        "infusion_link",
        "infusion_adopted",
        "infusion",
        "infusion_layer_link",
        "part",
    ] {
        assert!(
            !text.contains(&format!("\"{key}\"")),
            "an ordinary line saves no `{key}`: {text}"
        );
    }
    // A file of the older app has exactly those keys; it loads, and reconciling it changes nothing.
    let old: CharacterState = serde_json::from_str(&text).unwrap();
    assert_eq!(old, s);
    let again = reconcile(&e, &old, Mode::Load).unwrap_or_else(|| old.clone());
    assert_eq!(again.inventory, old.inventory);
}

#[test]
fn loot_with_a_generic_plus_one_is_never_adopted_deleted_or_upgraded() {
    let e = engine();
    // the character's own Longsword +1 (loot), then a pre-update pick of the +1 infusion
    let s = commands::add_adorned_item(&e, &tinker(&e), LONGSWORD, WEAPON_1, 1)
        .unwrap()
        .state;
    let loot = s.inventory.items.last().unwrap().clone();
    let key = infusion_slot(&e, &s, 0);
    let s = commands::select(&e, &s, &key, Choice::Element(KEEN.into()))
        .unwrap()
        .state;
    assert!(reconcile(&e, &s, Mode::Load).is_none(), "not taken on load");
    let snap = e.derive(&s);
    let p = &snap.inventory.infusion_prompts[0];
    assert_eq!(p.owned.len(), 1, "offered, for the user to choose");
    assert_eq!(p.owned[0].uid, loot.uid);
    // unpicking leaves the loot exactly as it was
    let s = commands::clear(&e, &s, &key).unwrap().state;
    assert!(reconcile(&e, &s, Mode::Change).is_none());
    assert_eq!(
        s.inventory.items.iter().find(|i| i.uid == loot.uid),
        Some(&loot)
    );
}

#[test]
fn the_user_can_choose_an_owned_copy_for_a_generic_infusion() {
    let e = engine();
    let s = commands::add_adorned_item(&e, &tinker(&e), LONGSWORD, WEAPON_1, 1)
        .unwrap()
        .state;
    let uid = s.inventory.items.last().unwrap().uid.clone();
    let s = pick(&e, &s, 0, KEEN);
    let link = e.derive(&s).inventory.infusion_prompts[0].link.clone();
    let s = borealis_engine::infusion_items::adopt(&e, &s, &link, &uid).expect("chosen");
    assert!(e.derive(&s).inventory.infusion_prompts.is_empty());
    assert_eq!(copies(&s, WEAPON_1).len(), 1);
}

#[test]
fn a_renamed_pick_keeps_its_line_and_a_swapped_one_loses_it() {
    let e = engine();
    let s = pick(&e, &tinker(&e), 0, BOOTS);
    let mut s = s;
    let line = s
        .inventory
        .items
        .iter_mut()
        .find(|i| i.item == BOOTS_ITEM)
        .unwrap();
    line.notes = Some("scuffed".into());
    let uid = line.uid.clone();
    let good = line.infusion_link.clone().unwrap();
    // the link still names the pick's old id (renamed in the content since)
    line.infusion_link = Some(good.replace(BOOTS, "ID_T_FEATURE_OLD_BOOTS"));
    let s = reconcile(&e, &s, Mode::Change).expect("re-linked");
    let boots = copies(&s, BOOTS_ITEM);
    assert_eq!(boots.len(), 1, "no copy removed and added");
    assert_eq!(boots[0].uid, uid);
    assert_eq!(boots[0].infusion_link.as_deref(), Some(good.as_str()));
    assert_eq!(boots[0].notes.as_deref(), Some("scuffed"));
    assert!(boots[0].equipped.is_some());

    // swapping the pick in the same slot for another infusion removes the old item
    let key = infusion_slot(&e, &s, 0);
    let swapped = commands::select(&e, &s, &key, Choice::Element(GLOWING.into()))
        .unwrap()
        .state;
    let swapped = reconcile(&e, &swapped, Mode::Change).unwrap_or(swapped);
    assert!(copies(&swapped, BOOTS_ITEM).is_empty());
}

#[test]
fn an_older_devices_save_then_an_edit_adds_no_duplicate() {
    let e = engine();
    let s = pick(&e, &tinker(&e), 0, BOOTS);
    // an older version saved it: the links are gone
    let mut old = s.clone();
    for it in &mut old.inventory.items {
        it.infusion_link = None;
    }
    // received like an opened file (Mode::Load first), then the user edits (Mode::Change)
    let loaded = reconcile(&e, &old, Mode::Load).expect("re-adopted");
    assert!(reconcile(&e, &loaded, Mode::Change).is_none());
    assert_eq!(copies(&loaded, BOOTS_ITEM).len(), 1);
}

#[test]
fn chosen_loot_is_unlinked_and_kept_unchanged_when_the_pick_goes() {
    for amount in [1, 3] {
        let e = engine();
        let s = commands::add_adorned_item(&e, &tinker(&e), LONGSWORD, WEAPON_1, amount)
            .unwrap()
            .state;
        let mut s = s;
        let loot = s.inventory.items.last_mut().unwrap();
        loot.notes = Some("from the dragon hoard".into());
        let uid = loot.uid.clone();
        let s = commands::equip(&e, &s, &uid, None).unwrap().state;
        let before = s
            .inventory
            .items
            .iter()
            .find(|i| i.uid == uid)
            .unwrap()
            .clone();
        let s = pick(&e, &s, 0, KEEN);
        let link = e.derive(&s).inventory.infusion_prompts[0].link.clone();
        let s = borealis_engine::infusion_items::adopt(&e, &s, &link, &uid).unwrap();
        assert!(
            s.inventory
                .items
                .iter()
                .find(|i| i.uid == uid)
                .unwrap()
                .infusion_adopted
        );
        let key = infusion_slot(&e, &s, 0);
        let s = commands::clear(&e, &s, &key).unwrap().state;
        let s = reconcile(&e, &s, Mode::Change).expect("unlinked");
        let after = s
            .inventory
            .items
            .iter()
            .find(|i| i.uid == uid)
            .expect("still owned");
        assert_eq!(
            after, &before,
            "amount {amount}: uid, adorner, notes, equip and count unchanged"
        );
    }
}
