//! Granted spells cast without a slot (`grant_spell`): limited free casts at a raised slot level
//! (`cast_level`), casts paid with resource points (`resource_cost`), and the at-will and
//! ritual-only modes, on synthetic content.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::Engine;
use serde_json::{json, Value};
use std::path::Path;

fn engine_with(elements: Vec<Value>) -> Engine {
    let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/core");
    let file: ContentFile = serde_json::from_value(
        json!({"format": "borealis-content/1", "ruleset": "dnd2024", "elements": elements}),
    )
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&core)
        .expect("core pack")
        .add_file("synthetic.json", PackOrigin::Core, file)
        .build();
    Engine::new(&store)
}

fn spell(id: &str, name: &str, level: u32) -> Value {
    json!({"id": id, "name": name, "type": "Spell", "source": "Test", "supports": ["Other", level.to_string()],
           "setters": [{"name": "level", "value": level.to_string()}]})
}

/// A class whose level 1 feature grants four spells, each cast a different slotless way.
fn content() -> Vec<Value> {
    vec![
        json!({"id": "ID_T_ADEPT", "name": "Adept", "type": "Class", "source": "Test",
        "setters": [{"name": "hd", "value": "d8"}],
        "rules": [{"kind": "grant", "id": "ID_T_ADEPT_GIFTS", "type": "Class Feature"}]}),
        json!({"id": "ID_T_ADEPT_GIFTS", "name": "Adept Gifts", "type": "Class Feature", "source": "Test",
        "rules": [
            {"kind": "resource", "id": "glimmer", "name": "Glimmer", "max": {"type": "fixed", "value": 4},
             "recharge": ["long_rest"]},
            {"kind": "grant_spell", "spell": "ID_T_SPELL_ZETA", "mode": "always_prepared",
             "free_cast": {"uses": 1, "recharge": "long_rest"}, "cast_level": 3},
            {"kind": "grant_spell", "spell": "ID_T_SPELL_ETA", "mode": "always_prepared",
             "resource_cost": {"resource": "glimmer", "cost": 2}},
            {"kind": "grant_spell", "spell": "ID_T_SPELL_THETA", "mode": "at_will_no_slot"},
            {"kind": "grant_spell", "spell": "ID_T_SPELL_IOTA", "mode": "ritual_only"},
            {"kind": "grant_spell", "spell": "ID_T_SPELL_KAPPA", "mode": "always_prepared",
             "resource_cost": {"resource": "glimmer", "cost": 1}, "level": 5}
        ]}),
        spell("ID_T_SPELL_ZETA", "Zeta", 1),
        spell("ID_T_SPELL_ETA", "Eta", 2),
        spell("ID_T_SPELL_THETA", "Theta", 1),
        spell("ID_T_SPELL_IOTA", "Iota", 1),
        spell("ID_T_SPELL_KAPPA", "Kappa", 3),
    ]
}

fn adept(e: &Engine) -> CharacterState {
    let s = commands::new_character(e, Ruleset::Dnd2024);
    let snap = e.derive(&s);
    let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).expect("class slot");
    commands::select(e, &s, &key, Choice::Element("ID_T_ADEPT".into()))
        .expect("class")
        .state
}

#[test]
fn slotless_grant_fields_are_read_without_diagnostics() {
    let e = engine_with(content());
    let snap = e.derive(&adept(&e));
    let loud: Vec<_> = snap
        .diagnostics
        .iter()
        .filter(|d| d.code.starts_with("content.unknown_rule"))
        .map(|d| d.message.clone())
        .collect();
    assert!(loud.is_empty(), "{loud:?}");
}

#[test]
fn a_free_cast_reports_its_raised_slot_level() {
    let e = engine_with(content());
    let snap = e.derive(&adept(&e));
    let zeta = snap
        .free_casts
        .iter()
        .find(|f| f.spell == "ID_T_SPELL_ZETA")
        .expect("free cast");
    assert_eq!(
        (zeta.uses, zeta.recharge.as_str(), zeta.cast_level),
        (Some(1), "long_rest", Some(3))
    );
    assert_eq!(
        snap.free_casts.len(),
        1,
        "only the limited grant has free casts"
    );
}

#[test]
fn a_spell_paid_with_resource_points_is_a_resource_cost_named_as_the_spell() {
    let e = engine_with(content());
    let snap = e.derive(&adept(&e));
    let costs: Vec<_> = snap
        .resource_costs
        .iter()
        .map(|c| (c.option.as_str(), c.resource.as_str(), c.cost))
        .collect();
    assert_eq!(
        costs,
        vec![("Eta", "glimmer", 2)],
        "a cost of a grant that is not live yet (level 5) is not listed"
    );
}
