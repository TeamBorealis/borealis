//! Charges of carried gear: an item that cannot be equipped (adventuring gear with no equip
//! slot, such as imported 5etools `G` items with charges) runs its resources while it is carried,
//! and nothing else of it applies. An item that can be equipped keeps the equipped rule, and an
//! item that requires attunement runs them only while attuned. Synthetic content on the bundled
//! core + SRD 5.1 packs.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands;
use borealis_engine::snapshot::ResourceSnapshot;
use borealis_engine::state::CharacterState;
use borealis_engine::{CharacterSnapshot, Engine};
use serde_json::{json, Value};
use std::path::Path;

const GEAR: &str = "ID_T_ROBOT_REMOTE";
const GEAR_CHARGES: &str = "ID_T_ROBOT_REMOTE_CHARGES";
const BOUND_GEAR: &str = "ID_T_BOUND_INCENSE";
const BOUND_CHARGES: &str = "ID_T_BOUND_INCENSE_CHARGES";
const ROLLED_GEAR: &str = "ID_T_NEEDLE_GUN";
const ROLLED_CHARGES: &str = "ID_T_NEEDLE_GUN_CHARGES";
const WAND: &str = "ID_T_WAND_WORN";
const WAND_CHARGES: &str = "ID_T_WAND_WORN_CHARGES";

fn charges(id: &str, max: Value) -> Value {
    json!({"kind": "resource", "id": id, "name": format!("{id} charges"), "max": max,
           "recharge": ["long_rest"]})
}

fn gear(id: &str, setters: Value, rules: Value) -> Value {
    let mut all = vec![json!({"name": "category", "value": "Adventuring Gear"})];
    all.extend(setters.as_array().cloned().unwrap_or_default());
    json!({"id": id, "name": id, "type": "Item", "source": "Test", "setters": all,
           "rules": rules})
}

fn engine() -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let elements = vec![
        gear(
            GEAR,
            json!([]),
            json!([charges(GEAR_CHARGES, json!({"type": "fixed", "value": 3})),
                   {"kind": "stat", "name": "strength", "value": "2"}]),
        ),
        gear(
            BOUND_GEAR,
            json!([{"name": "attunement", "value": "true"}]),
            json!([charges(BOUND_CHARGES, json!({"type": "fixed", "value": 5}))]),
        ),
        gear(
            ROLLED_GEAR,
            json!([]),
            json!([charges(
                ROLLED_CHARGES,
                json!({"type": "rolled", "dice": "1d6 + 3"})
            )]),
        ),
        json!({"id": WAND, "name": "Worn Wand", "type": "Magic Item", "source": "Test",
               "setters": [{"name": "category", "value": "Wondrous Items"},
                           {"name": "slot", "value": "wondrous"}],
               "rules": [charges(WAND_CHARGES, json!({"type": "fixed", "value": 7}))]}),
    ];
    let file: ContentFile = serde_json::from_value(json!({
        "format": "borealis-content/1", "ruleset": "dnd2014", "elements": elements}))
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .expect("bundled packs")
        .add_file("gear.json", PackOrigin::Custom, file)
        .build();
    Engine::new(&store)
}

fn hero(e: &Engine) -> CharacterState {
    let s = commands::new_character(e, Ruleset::Dnd2014);
    commands::set_base_scores(e, &s, [10, 14, 14, 10, 12, 8])
        .unwrap()
        .state
}

fn carry(e: &Engine, s: &CharacterState, item: &str) -> (CharacterState, String) {
    let s = commands::add_item(e, s, item, 1).unwrap().state;
    let uid = s.inventory.items.last().unwrap().uid.clone();
    (s, uid)
}

fn resource<'a>(snap: &'a CharacterSnapshot, id: &str) -> Option<&'a ResourceSnapshot> {
    snap.resources.iter().find(|r| r.id == id)
}

#[test]
fn gear_without_an_equip_slot_runs_its_charges_while_carried() {
    let e = engine();
    let (s, uid) = carry(&e, &hero(&e), GEAR);
    assert!(
        commands::equip(&e, &s, &uid, None).is_err(),
        "the gear has no equip slot"
    );
    let snap = e.derive(&s);
    let r = resource(&snap, GEAR_CHARGES).expect("charges while carried");
    assert_eq!((r.max, r.current), (3, 3));
    // only the resources run: the gear's other rules do not apply while it is carried
    assert_eq!(
        snap.abilities[0].score, 10,
        "no Strength bonus from carried gear"
    );
    // charges can be spent and come back on a long rest
    let s = commands::spend_resource(&e, &s, GEAR_CHARGES, 2)
        .unwrap()
        .state;
    assert_eq!(resource(&e.derive(&s), GEAR_CHARGES).unwrap().current, 1);
    let rested = commands::long_rest(&e, &s).unwrap().state;
    assert_eq!(
        resource(&e.derive(&rested), GEAR_CHARGES).unwrap().current,
        3
    );
    // dropped: gone
    let s = commands::remove_item(&e, &s, &uid).unwrap().state;
    assert!(resource(&e.derive(&s), GEAR_CHARGES).is_none());
}

#[test]
fn an_item_that_can_be_equipped_keeps_the_equipped_rule() {
    let e = engine();
    let (s, uid) = carry(&e, &hero(&e), WAND);
    assert!(
        resource(&e.derive(&s), WAND_CHARGES).is_none(),
        "only carried"
    );
    let s = commands::equip(&e, &s, &uid, None).unwrap().state;
    assert_eq!(resource(&e.derive(&s), WAND_CHARGES).unwrap().max, 7);
}

#[test]
fn carried_gear_that_requires_attunement_runs_only_while_attuned() {
    let e = engine();
    let (s, uid) = carry(&e, &hero(&e), BOUND_GEAR);
    let snap = e.derive(&s);
    assert!(resource(&snap, BOUND_CHARGES).is_none(), "not attuned");
    let line = snap.inventory.items.iter().find(|l| l.uid == uid).unwrap();
    assert!(line.requires_attunement && !line.attuned);
    // gear without an equip slot is attuned while carried
    let s = commands::attune(&e, &s, &uid)
        .expect("attunes while carried")
        .state;
    let snap = e.derive(&s);
    assert_eq!(resource(&snap, BOUND_CHARGES).unwrap().max, 5);
    let line = snap.inventory.items.iter().find(|l| l.uid == uid).unwrap();
    assert!(line.attuned);
    assert_eq!(snap.inventory.attuned, 1, "it counts against the limit");
    let s = commands::unattune(&e, &s, &uid).unwrap().state;
    assert!(resource(&e.derive(&s), BOUND_CHARGES).is_none());
    // an item that can be equipped is still equipped before it is attuned
    let (s, wand) = carry(&e, &s, WAND);
    assert!(commands::attune(&e, &s, &wand).is_err());
}

#[test]
fn rolled_charges_of_carried_gear_are_rolled_and_stored_on_the_line() {
    let e = engine();
    let (s, uid) = carry(&e, &hero(&e), ROLLED_GEAR);
    let rolled = s.inventory.items[0].rolled_max[ROLLED_CHARGES];
    assert!((4..=9).contains(&rolled), "1d6 + 3 rolled {rolled}");
    let snap = e.derive(&s);
    let r = resource(&snap, ROLLED_CHARGES).expect("rolled charges while carried");
    assert_eq!(r.max, i32::try_from(rolled).unwrap());
    let info = r.rolled.as_ref().expect("rolled info");
    assert!(info.rolled && info.dice == "1d6 + 3" && (info.min, info.max) == (4, 9));
    assert_eq!(info.item_uid.as_deref(), Some(uid.as_str()));
    let s = commands::set_rolled_max(&e, &s, &uid, ROLLED_CHARGES, 8)
        .unwrap()
        .state;
    assert_eq!(resource(&e.derive(&s), ROLLED_CHARGES).unwrap().max, 8);
    // a line without a stored roll shows the average
    let mut s = s;
    s.inventory.items[0].rolled_max.clear();
    let snap = e.derive(&s);
    let r = resource(&snap, ROLLED_CHARGES).unwrap();
    assert_eq!(r.max, 6, "average of 1d6 + 3 rounded down");
    assert!(!r.rolled.as_ref().unwrap().rolled);
}
