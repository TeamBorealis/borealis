//! Rule shapes imported content (5etools, Aurora XML) needs beyond the item mechanics of
//! `imported_item_rules.rs`: dice-valued charges rolled once per item, a choice whose every option
//! is already known, "proficient with this item", a bonus to every speed the character has, and
//! a choice mixing single elements and a category. Synthetic content on the bundled core +
//! SRD 5.1 packs.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::invariants;
use borealis_engine::snapshot::{ResourceSnapshot, SlotSnapshot};
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::{CharacterSnapshot, Engine};
use serde_json::{json, Value};
use std::path::Path;

const FIGHTER: &str = "ID_WOTC_PHB_CLASS_FIGHTER";
const WIZARD: &str = "ID_WOTC_PHB_CLASS_WIZARD";
const WAND: &str = "ID_T_WAND_LIVES";
const CHARGES: &str = "ID_T_WAND_LIVES_CHARGES";
const NAVIGATOR: &str = "ID_PROFICIENCY_TOOL_PROFICIENCY_NAVIGATORS_TOOLS";

fn wondrous(id: &str, name: &str, setters: Value, rules: Value) -> Value {
    let mut all = vec![json!({"name": "category", "value": "Wondrous Items"})];
    all.extend(setters.as_array().cloned().unwrap_or_default());
    json!({"id": id, "name": name, "type": "Magic Item", "source": "Test", "setters": all,
           "rules": rules})
}

fn sword(id: &str, name: &str, extra: Value) -> Value {
    let mut setters = vec![
        json!({"name": "category", "value": "Weapons"}),
        json!({"name": "slot", "value": "onehand"}),
        json!({"name": "damage", "value": "1d8", "attributes": {"type": "slashing"}}),
        json!({"name": "proficiency", "value": "ID_PROFICIENCY_WEAPON_PROFICIENCY_LONGSWORD"}),
    ];
    setters.extend(extra.as_array().cloned().unwrap_or_default());
    json!({"id": id, "name": name, "type": "Weapon", "source": "Test", "setters": setters,
           "supports": ["ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE"]})
}

fn race(id: &str, supports: &str) -> Value {
    json!({"id": id, "name": id, "type": "Race", "source": "Test", "rules": [
        {"kind": "select", "type": "Proficiency", "name": "Kin Training", "supports": supports,
         "number": 2}]})
}

fn engine() -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let elements = vec![
        wondrous(
            WAND,
            "Wand of Lives",
            json!([{"name": "slot", "value": "wondrous"}]),
            json!([{"kind": "resource", "id": CHARGES, "name": "Wand of Lives charges",
                    "max": {"type": "rolled", "dice": "1d6 + 1"}, "recharge": ["none"]}]),
        ),
        wondrous(
            "ID_T_BOOTS_ALL",
            "Boots of Every Stride",
            json!([{"name": "slot", "value": "feet"}]),
            json!([{"kind": "stat", "name": "speed:all", "value": "10"}]),
        ),
        wondrous(
            "ID_T_WINGS",
            "Wings",
            json!([{"name": "slot", "value": "shoulders"}]),
            json!([{"kind": "stat", "name": "innate speed:fly", "value": "30", "bonus": "base"}]),
        ),
        sword(
            "ID_T_PASSKEY",
            "Passkey Sword",
            json!([{"name": "proficient", "value": "true"}]),
        ),
        sword(
            "ID_T_CLAW",
            "Bound Claw",
            json!([{"name": "proficient", "value": "true"},
                   {"name": "attunement", "value": "true"}]),
        ),
        sword("ID_T_PLAIN", "Plain Sword", json!([])),
        race("ID_T_RACE_MARTIAL_KIN", "Weapon&&Martial"),
        race("ID_T_RACE_NOTHING", "No Such Tag At All"),
        race("ID_T_RACE_GUILD", &format!("Artisan tools||{NAVIGATOR}")),
    ];
    let file: ContentFile = serde_json::from_value(json!({
        "format": "borealis-content/1", "ruleset": "dnd2014", "elements": elements}))
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .expect("bundled packs")
        .add_file("items.json", PackOrigin::Custom, file)
        .build();
    Engine::new(&store)
}

fn pick(e: &Engine, s: &CharacterState, name: &str, id: &str) -> CharacterState {
    let key = slot_key(&e.derive(s), "ID_LEVEL_1", name, 1).expect("slot");
    commands::select(e, s, &key, Choice::Element(id.into()))
        .expect("select")
        .state
}

fn hero(e: &Engine, class: &str, race: Option<&str>) -> CharacterState {
    let s = commands::new_character(e, Ruleset::Dnd2014);
    let s = match race {
        Some(r) => pick(e, &s, "Race", r),
        None => s,
    };
    let s = pick(e, &s, "Class", class);
    commands::set_base_scores(e, &s, [16, 14, 14, 10, 12, 8])
        .unwrap()
        .state
}

fn wear(e: &Engine, s: &CharacterState, item: &str) -> (CharacterState, String) {
    let s = commands::add_item(e, s, item, 1).unwrap().state;
    let uid = s.inventory.items.last().unwrap().uid.clone();
    let s = commands::equip(e, &s, &uid, None).expect("equips").state;
    (s, uid)
}

fn charges(snap: &CharacterSnapshot) -> &ResourceSnapshot {
    snap.resources
        .iter()
        .find(|r| r.id == CHARGES)
        .expect("the wand's charges")
}

fn kin_slots(snap: &CharacterSnapshot) -> Vec<&SlotSnapshot> {
    snap.slots
        .iter()
        .filter(|s| s.name == "Kin Training")
        .collect()
}

#[test]
fn dice_charges_are_rolled_once_when_the_item_is_gained() {
    let e = engine();
    let (s, uid) = wear(&e, &hero(&e, WIZARD, None), WAND);
    let rolled = s.inventory.items[0].rolled_max[CHARGES];
    assert!((2..=7).contains(&rolled), "1d6 + 1 rolled {rolled}");
    let snap = e.derive(&s);
    let r = charges(&snap);
    assert_eq!(r.max, i32::try_from(rolled).unwrap());
    let info = r.rolled.as_ref().expect("rolled info");
    assert!(info.rolled && info.dice == "1d6 + 1" && (info.min, info.max) == (2, 7));
    assert_eq!(info.item_uid.as_deref(), Some(uid.as_str()));
    // the roll is stable: deriving and equipping again do not roll it again
    let again = commands::unequip(&e, &s, &uid).unwrap().state;
    let again = commands::equip(&e, &again, &uid, None).unwrap().state;
    assert_eq!(charges(&e.derive(&again)).max, r.max);
    // it survives a save
    let json = serde_json::to_string(&s).unwrap();
    let back: CharacterState = serde_json::from_str(&json).unwrap();
    assert_eq!(back.inventory.items[0].rolled_max[CHARGES], rolled);
}

#[test]
fn dice_charges_can_be_entered_or_rolled_again() {
    let e = engine();
    let (s, uid) = wear(&e, &hero(&e, WIZARD, None), WAND);
    let s = commands::set_rolled_max(&e, &s, &uid, CHARGES, 6)
        .unwrap()
        .state;
    assert_eq!(charges(&e.derive(&s)).max, 6);
    for bad in [1, 8] {
        assert!(
            commands::set_rolled_max(&e, &s, &uid, CHARGES, bad).is_err(),
            "{bad} is outside 1d6 + 1"
        );
    }
    assert!(commands::set_rolled_max(&e, &s, &uid, "ID_T_OTHER", 3).is_err());
    let s = commands::reroll_max(&e, &s, &uid, CHARGES).unwrap().state;
    assert!((2..=7).contains(&charges(&e.derive(&s)).max));
    // spent charges count against the stored maximum
    let s = commands::set_rolled_max(&e, &s, &uid, CHARGES, 5)
        .unwrap()
        .state;
    let s = commands::spend_resource(&e, &s, CHARGES, 2).unwrap().state;
    let r = e.derive(&s);
    assert_eq!((charges(&r).max, charges(&r).current), (5, 3));
}

#[test]
fn a_line_without_a_stored_roll_shows_the_average() {
    let e = engine();
    let (mut s, _) = wear(&e, &hero(&e, WIZARD, None), WAND);
    s.inventory.items[0].rolled_max.clear();
    let snap = e.derive(&s);
    let r = charges(&snap);
    assert_eq!(r.max, 4, "average of 1d6 + 1 rounded down");
    assert!(!r.rolled.as_ref().unwrap().rolled);
}

#[test]
fn a_choice_whose_every_option_is_known_is_done() {
    let e = engine();
    // a wizard knows no martial weapon: the two picks are open
    let wiz = e.derive(&hero(&e, WIZARD, Some("ID_T_RACE_MARTIAL_KIN")));
    let open = kin_slots(&wiz);
    assert_eq!(open.len(), 2);
    assert!(open
        .iter()
        .all(|s| s.requires_selection && !s.all_known && !s.options.is_empty()));
    // a fighter already has every martial weapon: nothing is left to choose
    let fighter = hero(&e, FIGHTER, Some("ID_T_RACE_MARTIAL_KIN"));
    let ftr = e.derive(&fighter);
    let done = kin_slots(&ftr);
    assert_eq!(done.len(), 2);
    for s in &done {
        assert!(s.options.is_empty() && s.all_known && !s.requires_selection);
    }
    assert!(
        !invariants::check(&e, &fighter, &ftr)
            .iter()
            .any(|v| v.kind == invariants::kind::EMPTY_SELECT),
        "not an invalid choice"
    );
    // without options computed (the play-time derive) the slot is done as well
    let quick = e.derive_with(&fighter, false);
    assert!(kin_slots(&quick).iter().all(|s| !s.requires_selection));
}

#[test]
fn a_choice_that_matches_nothing_stays_open() {
    let e = engine();
    let snap = e.derive(&hero(&e, FIGHTER, Some("ID_T_RACE_NOTHING")));
    for s in kin_slots(&snap) {
        assert!(s.options.is_empty() && !s.all_known && s.requires_selection);
    }
}

#[test]
fn a_choice_mixes_a_single_element_and_a_category() {
    let e = engine();
    let snap = e.derive(&hero(&e, WIZARD, Some("ID_T_RACE_GUILD")));
    let s = kin_slots(&snap)[0];
    let ids: Vec<&str> = s.options.iter().map(|o| o.id.as_str()).collect();
    assert!(ids.contains(&NAVIGATOR), "the single tool: {ids:?}");
    assert!(
        ids.iter().any(|i| i.contains("SMITHS_TOOLS")),
        "the category: {ids:?}"
    );
    assert!(
        !ids.iter().any(|i| i.contains("THIEVES")),
        "nothing else: {ids:?}"
    );
}

fn attack_proficient(e: &Engine, s: &CharacterState, name: &str) -> bool {
    e.derive(s)
        .attacks
        .iter()
        .find(|a| a.name == name)
        .unwrap_or_else(|| panic!("attack {name}"))
        .proficient
}

#[test]
fn an_item_can_make_the_character_proficient_with_itself() {
    let e = engine();
    let wiz = hero(&e, WIZARD, None);
    let (s, _) = wear(&e, &wiz, "ID_T_PLAIN");
    assert!(
        !attack_proficient(&e, &s, "Plain Sword"),
        "no longsword proficiency"
    );
    let (s, uid) = wear(&e, &wiz, "ID_T_PASSKEY");
    assert!(attack_proficient(&e, &s, "Passkey Sword"));
    let snap = e.derive(&s);
    let line = snap.inventory.items.iter().find(|i| i.uid == uid).unwrap();
    assert!(line.proficient_by_item);
    // an item that requires attunement makes the character proficient only while attuned
    let (s, uid) = wear(&e, &wiz, "ID_T_CLAW");
    assert!(
        attack_proficient(&e, &s, "Bound Claw"),
        "equipping attunes it"
    );
    let s = commands::unattune(&e, &s, &uid).unwrap().state;
    assert!(!attack_proficient(&e, &s, "Bound Claw"));
}

#[test]
fn a_bonus_to_every_speed_creates_no_new_speed() {
    let e = engine();
    let base = e.derive(&hero(&e, WIZARD, None)).speed;
    let (s, _) = wear(&e, &hero(&e, WIZARD, None), "ID_T_BOOTS_ALL");
    let sp = e.derive(&s).speed;
    assert_eq!(sp.walk, base.walk + 10);
    assert_eq!(
        (sp.fly, sp.swim, sp.climb, sp.burrow),
        (0, 0, 0, 0),
        "no new speeds"
    );
    let (s, _) = wear(&e, &s, "ID_T_WINGS");
    let sp = e.derive(&s).speed;
    assert_eq!(
        (sp.walk, sp.fly),
        (base.walk + 10, 40),
        "the flying speed it has"
    );
}
