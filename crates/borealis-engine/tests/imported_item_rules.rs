//! Item mechanics that imported content (5etools, Aurora XML) carries: attunement restrictions,
//! `+N` ammunition, a weapon's own critical range, ability-check and concentration bonuses and
//! toggled speed multipliers. Synthetic items on the bundled core + SRD 5.1 packs.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::attunement;
use borealis_engine::commands::{self, slot_key};
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::{CharacterSnapshot, Engine};
use serde_json::{json, Value};
use std::path::Path;

fn magic(id: &str, name: &str, attune: Option<Value>, rules: Value) -> Value {
    let mut setters = vec![json!({"name": "category", "value": "Wondrous Items"})];
    if let Some(a) = attune {
        setters.push(a);
    }
    json!({"id": id, "name": name, "type": "Magic Item", "source": "Test", "setters": setters,
           "rules": rules})
}

fn by(text: &str) -> Value {
    json!({"name": "attunement", "value": "true", "attributes": {"addition": text}})
}

fn ammo(id: &str, name: &str, bonus: i32) -> Value {
    json!({"id": id, "name": name, "type": "Magic Item", "source": "Test", "setters": [
        {"name": "category", "value": "Ammunition"}, {"name": "enhancement", "value": bonus.to_string()},
        {"name": "stackable", "value": "true"}]})
}

fn engine() -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let ac = json!([{"kind": "stat", "name": "ac:misc", "value": "1"}]);
    let elements = vec![
        magic(
            "ID_T_ROD_PRIESTS",
            "Priest Rod",
            Some(by("by a cleric, druid, or warlock")),
            ac.clone(),
        ),
        magic(
            "ID_T_ROD_CASTERS",
            "Caster Rod",
            Some(by("by a spellcaster")),
            ac.clone(),
        ),
        magic(
            "ID_T_ROD_FIGHTERS",
            "Fighter Rod",
            Some(by("by a Fighter")),
            ac.clone(),
        ),
        magic(
            "ID_T_ROD_GOOD",
            "Good Rod",
            Some(by("by a creature of good alignment")),
            ac.clone(),
        ),
        magic(
            "ID_T_ROD_ODD",
            "Odd Rod",
            Some(by("by a creature missing a limb")),
            ac.clone(),
        ),
        magic(
            "ID_T_ROD_EXPLICIT",
            "Explicit Rod",
            Some(json!({"name": "attunement", "value": "true",
                "attributes": {"addition": "by a fighter", "requirements": "[wizard:1]"}})),
            ac.clone(),
        ),
        magic(
            "ID_T_STONE_LUCK",
            "Luck Stone",
            None,
            json!([{"kind": "stat", "name": "ability checks", "value": "1"},
                     {"kind": "stat", "name": "concentration:save", "value": "2"}]),
        ),
        magic(
            "ID_T_BOOTS_SPEED",
            "Speed Boots",
            None,
            json!([{"kind": "toggle",
            "id": "ID_T_BOOTS_SPEED_ON", "name": "Speed Boots", "activation": "bonus action",
            "effects": [{"kind": "speed_multiply", "speed": "walk", "factor": 2}]}]),
        ),
        magic(
            "ID_T_WHISTLE",
            "Eagle Pipe",
            None,
            json!([{"kind": "toggle",
            "id": "ID_T_WHISTLE_ON", "name": "Eagle Pipe",
            "effects": [{"kind": "speed_multiply", "speed": "fly", "factor": 2, "of": "walk"}]}]),
        ),
        ammo("ID_T_ARROW_1", "Arrow +1", 1),
        ammo("ID_T_ARROW_2", "Arrow +2", 2),
        ammo("ID_T_BOLT_3", "Crossbow Bolt +3", 3),
        json!({"id": "ID_T_QUIVER_1", "name": "Quiver +1 (20)", "type": "Magic Item",
            "source": "Test", "setters": [{"name": "category", "value": "Equipment Packs"},
                {"name": "ammunition", "value": "arrow"}, {"name": "enhancement", "value": "1"}]}),
        json!({"id": "ID_T_KEEN_SWORD", "name": "Keen Sword", "type": "Weapon", "source": "Test",
            "setters": [{"name": "category", "value": "Weapons"}, {"name": "slot", "value": "onehand"},
                {"name": "damage", "value": "1d8", "attributes": {"type": "slashing"}},
                {"name": "proficiency", "value": "ID_PROFICIENCY_WEAPON_PROFICIENCY_LONGSWORD"}],
            "supports": ["ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE"],
            "rules": [{"kind": "critical_range", "min": 19, "item": "self"}]}),
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

fn hero(e: &Engine, class: &str) -> CharacterState {
    let s = commands::new_character(e, Ruleset::Dnd2014);
    let key = slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    let s = commands::select(e, &s, &key, Choice::Element(class.into()))
        .unwrap()
        .state;
    commands::set_base_scores(e, &s, [16, 14, 14, 10, 12, 8])
        .unwrap()
        .state
}

const FIGHTER: &str = "ID_WOTC_PHB_CLASS_FIGHTER";
const CLERIC: &str = "ID_WOTC_PHB_CLASS_CLERIC";

fn add(e: &Engine, s: &CharacterState, item: &str) -> (CharacterState, String) {
    let s = commands::add_item(e, s, item, 1).unwrap().state;
    let uid = s.inventory.items.last().unwrap().uid.clone();
    (s, uid)
}

/// Equip `item`; returns the state and the equip warnings.
fn wear(e: &Engine, s: &CharacterState, item: &str) -> (CharacterState, Vec<String>) {
    let (s, uid) = add(e, s, item);
    let o = commands::equip(e, &s, &uid, None).expect("equips");
    (o.state, o.warnings)
}

fn line<'a>(
    snap: &'a CharacterSnapshot,
    item: &str,
) -> &'a borealis_engine::snapshot::ItemSnapshot {
    snap.inventory
        .items
        .iter()
        .find(|l| l.item.id == item)
        .expect("inventory line")
}

#[test]
fn a_class_restriction_bars_attunement_for_other_classes() {
    let e = engine();
    let (s, warnings) = wear(&e, &hero(&e, FIGHTER), "ID_T_ROD_PRIESTS");
    let snap = e.derive(&s);
    let l = line(&snap, "ID_T_ROD_PRIESTS");
    assert!(!l.attuned && l.attunement_barred, "{l:?}");
    assert_eq!(
        l.attunement_restriction.as_deref(),
        Some("by a cleric, druid, or warlock")
    );
    assert!(
        warnings[0].contains("requires attunement by a cleric"),
        "{warnings:?}"
    );
    let err = commands::attune(&e, &s, &l.uid).expect_err("refused");
    assert!(err.to_string().contains("by a cleric"), "{err}");

    let (s, warnings) = wear(&e, &hero(&e, CLERIC), "ID_T_ROD_PRIESTS");
    let l = line(&e.derive(&s), "ID_T_ROD_PRIESTS").clone();
    assert!(l.attuned && !l.attunement_barred && warnings.is_empty());
}

#[test]
fn spellcaster_alignment_and_explicit_restrictions() {
    let e = engine();
    let fighter = hero(&e, FIGHTER);
    let cleric = hero(&e, CLERIC);
    let attuned = |s: &CharacterState, item: &str| {
        let (s, _) = wear(&e, s, item);
        line(&e.derive(&s), item).attuned
    };
    assert!(!attuned(&fighter, "ID_T_ROD_CASTERS"));
    assert!(attuned(&cleric, "ID_T_ROD_CASTERS"));
    assert!(
        attuned(&fighter, "ID_T_ROD_FIGHTERS"),
        "class names ignore case"
    );
    assert!(
        attuned(&fighter, "ID_T_ROD_ODD"),
        "unknown words: shown, not enforced"
    );
    assert!(
        !attuned(&fighter, "ID_T_ROD_EXPLICIT"),
        "the requirements attribute wins over the text"
    );
    assert!(
        attuned(&fighter, "ID_T_ROD_GOOD"),
        "no alignment chosen yet: not enforced"
    );
    let snap = e.derive(&fighter);
    let slot = snap
        .slots
        .iter()
        .find(|s| s.ty == "Alignment")
        .expect("alignment slot");
    let good = commands::select(
        &e,
        &fighter,
        &slot.key,
        Choice::Element("ID_ALIGNMENT_NEUTRAL_GOOD".into()),
    )
    .unwrap()
    .state;
    assert!(attuned(&good, "ID_T_ROD_GOOD"));
    let evil = commands::select(
        &e,
        &fighter,
        &slot.key,
        Choice::Element("ID_ALIGNMENT_NEUTRAL_EVIL".into()),
    )
    .unwrap()
    .state;
    assert!(!attuned(&evil, "ID_T_ROD_GOOD"));
}

#[test]
fn restriction_text_becomes_a_requirement() {
    let e = engine();
    let uni = e.universe(Ruleset::Dnd2014);
    let req = |t: &str| attunement::requirement_from_text(uni, t);
    assert_eq!(
        req("by a sorcerer, warlock, or wizard").as_deref(),
        Some("[sorcerer:1]||[warlock:1]||[wizard:1]")
    );
    assert_eq!(
        req("by a spellcaster").as_deref(),
        Some("[feature:spellcasting]")
    );
    assert_eq!(req("by a dwarf").as_deref(), Some("[feature:Dwarf]"));
    let evil = req("by an evil cleric or paladin").expect("parsed");
    assert!(
        evil.starts_with("([cleric:1]||[paladin:1])&&(")
            && evil.contains("ID_ALIGNMENT_LAWFUL_EVIL"),
        "{evil}"
    );
    let non_good = req("by a creature of non-good alignment").expect("parsed");
    let allowed = non_good.split("||!(").next().unwrap_or_default();
    assert!(
        !allowed.contains("GOOD") && allowed.contains("ID_ALIGNMENT_NEUTRAL"),
        "{non_good}"
    );
    assert_eq!(req("by a warlock with the Pact of the Blade feature"), None);
    assert_eq!(req("by a humanoid"), None);
}

#[test]
fn an_attuned_barred_item_from_a_save_still_applies_with_a_warning() {
    let e = engine();
    let s = hero(&e, FIGHTER);
    let base_ac = e.derive(&s).armor_class;
    let (mut s, _) = wear(&e, &s, "ID_T_ROD_PRIESTS");
    s.inventory.items.last_mut().unwrap().attuned = true;
    let snap = e.derive(&s);
    assert!(line(&snap, "ID_T_ROD_PRIESTS").attuned);
    assert_eq!(snap.armor_class, base_ac + 1);
    assert!(snap
        .diagnostics
        .iter()
        .any(|d| d.code == "attunement.restricted"));
}

fn bow_attack(e: &Engine, s: &CharacterState) -> (i32, String, Option<String>) {
    let a = &e.derive(s).attacks[0];
    (a.attack, a.damage.clone(), a.ammunition.clone())
}

#[test]
fn plus_n_ammunition_adds_to_the_weapon_that_fires_it() {
    let e = engine();
    let (bow, _) = wear(&e, &hero(&e, FIGHTER), "ID_WOTC_PHB_WEAPON_LONGBOW");
    let (a0, d0, none) = bow_attack(&e, &bow);
    assert!(none.is_none());
    let (with1, _) = add(&e, &bow, "ID_T_ARROW_1");
    let (a1, d1, name) = bow_attack(&e, &with1);
    assert_eq!((a1, name.as_deref()), (a0 + 1, Some("Arrow +1")));
    assert_eq!(
        (d0.as_str(), d1.as_str()),
        ("1d8+2 piercing", "1d8+3 piercing")
    );
    let (bundle, _) = add(&e, &bow, "ID_T_QUIVER_1");
    assert_eq!(
        bow_attack(&e, &bundle).0,
        a0 + 1,
        "a bundle with an ammunition kind"
    );
    let (with2, _) = add(&e, &with1, "ID_T_ARROW_2");
    assert_eq!(bow_attack(&e, &with2).0, a0 + 2, "the best carried");
    let (bolts, _) = add(&e, &bow, "ID_T_BOLT_3");
    assert_eq!(bow_attack(&e, &bolts).0, a0, "a bow does not fire bolts");
    let stored = {
        let mut s = with1.clone();
        s.inventory.storages.push("Chest".into());
        let uid = s.inventory.items.last().unwrap().uid.clone();
        s = commands::move_to_storage(&s, &uid, Some("Chest"))
            .unwrap()
            .state;
        s
    };
    assert_eq!(
        bow_attack(&e, &stored).0,
        a0,
        "stored ammunition is not at hand"
    );
    let (xbow, _) = wear(&e, &hero(&e, FIGHTER), "ID_WOTC_PHB_WEAPON_CROSSBOW_LIGHT");
    let (x0, _, _) = bow_attack(&e, &xbow);
    let (xb, _) = add(&e, &xbow, "ID_T_BOLT_3");
    assert_eq!(bow_attack(&e, &xb).0, x0 + 3);
}

#[test]
fn a_weapon_critical_range_applies_to_that_weapon_only() {
    let e = engine();
    let (s, _) = wear(&e, &hero(&e, FIGHTER), "ID_T_KEEN_SWORD");
    let (s, _) = wear(&e, &s, "ID_WOTC_PHB_WEAPON_LONGSWORD");
    let snap = e.derive(&s);
    let crit = |name: &str| {
        snap.attacks
            .iter()
            .find(|a| a.name == name)
            .unwrap()
            .crit_range
    };
    assert_eq!((crit("Keen Sword"), crit("Longsword")), (19, 20));
}

#[test]
fn ability_check_and_concentration_bonuses() {
    let e = engine();
    let s = hero(&e, FIGHTER);
    let before = e.derive(&s);
    let (s, _) = wear(&e, &s, "ID_T_STONE_LUCK");
    let after = e.derive(&s);
    for (b, a) in before.skills.iter().zip(&after.skills) {
        assert_eq!(
            (a.total, a.passive),
            (b.total + 1, b.passive + 1),
            "{}",
            a.name
        );
    }
    assert_eq!(after.initiative, before.initiative + 1);
    assert_eq!(
        after.abilities[2].save, before.abilities[2].save,
        "not a save bonus"
    );
    assert_eq!(before.concentration_save, before.abilities[2].save);
    assert_eq!(after.concentration_save, before.concentration_save + 2);
}

#[test]
fn toggled_speed_multipliers() {
    let e = engine();
    let s = hero(&e, FIGHTER);
    let walk = e.derive(&s).speed.walk;
    let (boots, _) = wear(&e, &s, "ID_T_BOOTS_SPEED");
    assert_eq!(e.derive(&boots).speed.walk, walk, "off until switched on");
    let on = commands::set_toggle(&e, &boots, "ID_T_BOOTS_SPEED_ON", true)
        .unwrap()
        .state;
    let snap = e.derive(&on);
    assert_eq!(snap.speed.walk, walk * 2);
    let t = snap
        .toggles
        .iter()
        .find(|t| t.id == "ID_T_BOOTS_SPEED_ON")
        .unwrap();
    assert_eq!(t.effects, vec!["Walk speed x2".to_string()]);
    let (pipe, _) = wear(&e, &on, "ID_T_WHISTLE");
    let both = commands::set_toggle(&e, &pipe, "ID_T_WHISTLE_ON", true)
        .unwrap()
        .state;
    assert_eq!(
        e.derive(&both).speed.fly,
        walk * 4,
        "twice the doubled walk"
    );
}
