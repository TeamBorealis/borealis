//! Spell list extensions that apply to a class (10.4), always-prepared `<extend prepared>`
//! entries and free casts bound to a Spell select (`spell_free_cast`), on synthetic content.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::SpellcastingSnapshot;
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::Engine;
use serde_json::{json, Value};
use std::path::Path;

fn engine_with(extra: Vec<Value>) -> Engine {
    let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/core");
    let file: ContentFile = serde_json::from_value(
        json!({"format": "borealis-content/1", "ruleset": "dnd2014", "elements": extra}),
    )
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&core)
        .expect("core pack")
        .add_file("synthetic.json", PackOrigin::Core, file)
        .build();
    Engine::new(&store)
}

fn spell(id: &str, name: &str, level: u32, tags: &[&str]) -> Value {
    let mut supports: Vec<String> = tags.iter().map(|t| (*t).to_string()).collect();
    supports.push(level.to_string());
    json!({"id": id, "name": name, "type": "Spell", "source": "Test", "supports": supports,
           "setters": [{"name": "level", "value": level.to_string()}]})
}

/// A prepared caster ("Seer") whose whole list is preparable, slots of level 1 at class level 1
/// and level 2 at class level 3, plus a feat-like extension granted by the class.
fn content(extension_all: bool) -> Vec<Value> {
    let ext = if extension_all {
        json!({"extend": true, "all": true,
               "extends": [{"supports": "ID_T_SPELL_BETA"}, {"supports": "ID_T_SPELL_GAMMA", "prepared": true}]})
    } else {
        json!({"name": "Seer", "extend": true,
               "extends": [{"supports": "ID_T_SPELL_BETA"}, {"supports": "ID_T_SPELL_GAMMA", "prepared": true}]})
    };
    vec![
        json!({"id": "ID_T_SEER", "name": "Seer", "type": "Class", "source": "Test",
        "setters": [{"name": "hd", "value": "d8"}],
        "rules": [
            {"kind": "grant", "id": "ID_T_SEER_CASTING", "type": "Class Feature"},
            {"kind": "grant", "id": "ID_T_MARK", "type": "Grants"}
        ]}),
        json!({"id": "ID_T_SEER_CASTING", "name": "Seer Spellcasting", "type": "Class Feature", "source": "Test",
        "spellcasting": {"name": "Seer", "ability": "Wisdom", "prepare": true, "list": {"supports": "Seer", "known": true}},
        "rules": [
            {"kind": "stat", "name": "seer:spellcasting:prepare", "value": "3"},
            {"kind": "stat", "name": "seer:spellcasting:slots:1", "value": "2"},
            {"kind": "stat", "name": "seer:spellcasting:slots:2", "value": "2", "level": 3},
            {"kind": "select", "type": "Spell", "name": "Gift Spell", "supports": "1", "spellcasting": "Seer"},
            {"kind": "spell_free_cast", "select": "Gift Spell", "uses": 1, "recharge": "long_rest"},
            {"kind": "grant_spell", "spell": "ID_T_SPELL_EPSILON", "mode": "always_prepared", "spell_list": "Seer",
             "free_cast": {"uses": "wisdom", "recharge": "long_rest"}}
        ]}),
        json!({"id": "ID_T_MARK", "name": "Mark of Seeing", "type": "Grants", "source": "Test",
               "spellcasting": ext}),
        spell("ID_T_SPELL_ALPHA", "Alpha", 1, &["Seer"]),
        spell("ID_T_SPELL_BETA", "Beta", 1, &["Other"]),
        spell("ID_T_SPELL_GAMMA", "Gamma", 2, &["Other"]),
        spell("ID_T_SPELL_DELTA", "Delta", 2, &["Other"]),
        spell("ID_T_SPELL_EPSILON", "Epsilon", 1, &["Other"]),
    ]
}

fn seer(e: &Engine, levels: u32) -> CharacterState {
    let mut s = commands::new_character(e, Ruleset::Dnd2014);
    s.abilities.base = [10, 10, 10, 10, 14, 10];
    let snap = e.derive(&s);
    let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).expect("class slot");
    s = commands::select(e, &s, &key, Choice::Element("ID_T_SEER".into()))
        .expect("class")
        .state;
    for _ in 1..levels {
        s = commands::level_up_main(e, &s, None)
            .expect("level up")
            .state;
    }
    s
}

fn caster(e: &Engine, s: &CharacterState) -> SpellcastingSnapshot {
    e.derive(s).caster("Seer").expect("Seer caster").clone()
}

fn entry<'a>(
    c: &'a SpellcastingSnapshot,
    id: &str,
) -> Option<&'a borealis_engine::snapshot::SpellEntry> {
    c.spells.iter().find(|s| s.id == id)
}

#[test]
fn extension_lists_feed_the_preparable_spells_of_a_list_caster() {
    for all in [true, false] {
        let e = engine_with(content(all));
        let c = caster(&e, &seer(&e, 1));
        assert!(c.prepare_from_list);
        let alpha = entry(&c, "ID_T_SPELL_ALPHA").expect("class list spell");
        assert!(!alpha.known);
        let beta = entry(&c, "ID_T_SPELL_BETA").expect("extension spell is preparable");
        assert!(!beta.known && !beta.always_prepared);
        // level 2 spells wait for level 2 slots, on and off the list
        assert!(entry(&c, "ID_T_SPELL_GAMMA").is_none());
        assert!(entry(&c, "ID_T_SPELL_DELTA").is_none());
    }
}

#[test]
fn prepared_extension_entries_join_once_the_class_can_cast_them() {
    let e = engine_with(content(true));
    let c = caster(&e, &seer(&e, 3));
    let gamma = entry(&c, "ID_T_SPELL_GAMMA").expect("always prepared at level 3");
    assert!(gamma.known && gamma.prepared && gamma.always_prepared);
    assert_eq!(gamma.level, 2);
    assert_eq!(
        gamma.granted_by.as_ref().map(|g| g.name.as_str()),
        Some("Seer"),
        "a Grants carrier is reported as its granter"
    );
    assert_eq!(c.prepared_count, 0, "always prepared spells do not count");
    // a spell outside every list stays out
    assert!(entry(&c, "ID_T_SPELL_DELTA").is_none());
}

#[test]
fn spell_free_cast_binds_to_the_spell_picked_in_its_select() {
    let e = engine_with(content(true));
    let mut s = seer(&e, 1);
    let granted = e.derive(&s).free_casts;
    assert_eq!(granted.len(), 1);
    assert_eq!(
        (granted[0].uses, granted[0].uses_rule.as_str()),
        (Some(2), "wisdom"),
        "uses of an ability are its modifier (Wisdom 14)"
    );
    let snap = e.derive(&s);
    let key = slot_key(&snap, "ID_T_SEER_CASTING", "Gift Spell", 1).expect("gift slot");
    s = commands::select(&e, &s, &key, Choice::Element("ID_T_SPELL_ALPHA".into()))
        .expect("pick")
        .state;
    let snap = e.derive(&s);
    let f = snap
        .free_casts
        .iter()
        .find(|f| f.spell == "ID_T_SPELL_ALPHA")
        .expect("free cast of the picked spell");
    assert_eq!((f.uses, f.recharge.as_str()), (Some(1), "long_rest"));
    assert!(!f.also_with_slots);
    assert_eq!(f.source, "ID_T_SEER_CASTING");
}
