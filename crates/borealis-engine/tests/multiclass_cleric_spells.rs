//! A multiclass into a class whose spellcasting block sits on the class element itself casts like
//! that class taken first (Discord report, 1.5.0: a Sorcerer 1 who multiclassed into an imported
//! Cleric got three "Cantrip (Cleric 1)" slots that offered nothing, and no Cleric spell list).
//!
//! The bundled SRD classes carry their block on a `Spellcasting` class feature, which the
//! multiclass grants like the class does. A class imported from the 5etools format, or made in the
//! homebrew creator, carries it on the `Class` element, and the synthesized `Multiclass` element
//! must carry it too: without it no block serves the class's name, so `$(spellcasting:list)`
//! matches nothing and the Magic page has no list to pick or prepare from.
//!
//! Bundled SRD 5.1 content plus one synthetic class in the shape of an imported one (no text).

mod common;
use borealis_content::{ContentFile, ContentStore, ContentStoreBuilder, PackOrigin};
use borealis_engine::commands;
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::Engine;
use common::*;
use serde_json::json;
use std::sync::OnceLock;

const PRIEST: &str = "ID_T_MCS_CLASS_CLERIC";
const PRIEST_MC: &str = "ID_T_MCS_MULTICLASS_CLERIC";
const CANTRIPS: &str = "Cantrip (Cleric 1)";

/// A 2014 cleric as the 5etools-format importer writes it: the block, the slot and prepare
/// stats and the cantrip select all on the class element, the multiclass node only proficiencies.
fn imported_cleric() -> ContentFile {
    let stat = |name: &str, value: &str| json!({"kind": "stat", "name": name, "value": value});
    serde_json::from_value(json!({
        "format": "borealis-content/1",
        "ruleset": "dnd2014",
        "elements": [{
            "id": PRIEST, "name": "Cleric", "type": "Class", "source": "Test",
            "setters": [{"name": "hd", "value": "d8"}],
            "spellcasting": {"name": "Cleric", "ability": "Wisdom", "prepare": true,
                             "list": {"known": true, "supports": "Cleric"}},
            "rules": [
                stat("cleric:spellcasting:slots:1", "2"),
                stat("cleric:spellcasting:cantrips known", "3"),
                stat("cleric:spellcasting:prepare", "level:cleric"),
                stat("cleric:spellcasting:prepare", "wisdom:modifier"),
                {"kind": "select", "type": "Spell", "name": CANTRIPS, "number": 3,
                 "spellcasting": "Cleric", "supports": "$(spellcasting:list), 0"}
            ],
            "multiclass": {
                "id": PRIEST_MC, "prerequisite": "Wisdom 13", "requirements": "[wis:13]",
                "rules": [{"kind": "grant", "id": "ID_INTERNAL_GRANT_MULTICLASS", "type": "Grants"}]
            }
        }]
    }))
    .expect("synthetic class")
}

fn engine() -> Option<&'static Engine> {
    static CELL: OnceLock<Option<(ContentStore, Engine)>> = OnceLock::new();
    CELL.get_or_init(|| {
        let (core, srd) = (core_pack_dir(), srd51_pack_dir());
        if !core.is_dir() || !srd.is_dir() {
            return None;
        }
        let store = ContentStoreBuilder::new()
            .add_pack_dir(&core)
            .expect("core pack")
            .add_pack_dir(&srd)
            .expect("SRD 5.1 pack")
            .add_file(
                "imported-cleric.json",
                PackOrigin::Custom,
                imported_cleric(),
            )
            .build();
        let engine = Engine::new(&store);
        Some((store, engine))
    })
    .as_ref()
    .map(|(_, e)| e)
}

/// SRD Sorcerer 1 with every sorcerer spell chosen, then a new multiclass level into `cleric_mc`.
fn sorcerer_then(e: &Engine, cleric_mc: &str) -> CharacterSnapshot {
    let mut h = Hero::new(e, true);
    h.base([8, 14, 14, 10, 14, 15])
        .race_class(RACE_HUMAN, &class("Sorcerer"));
    while let Some((key, id)) = h.snap().slots.iter().find_map(|s| {
        (s.ty == "Spell" && s.chosen.is_none())
            .then(|| s.options.first().map(|o| (s.key.clone(), o.id.clone())))
            .flatten()
    }) {
        h.pick_key(&key, &id);
    }
    h.s = commands::level_up_new_multiclass(h.e, &h.s, None)
        .expect("new multiclass level")
        .state;
    h.pick("ID_LEVEL_2", "Multiclass (Level 2)", 1, cleric_mc);
    h.snap()
}

#[test]
fn bundled_multiclass_cleric_offers_cleric_cantrips_and_a_list() {
    let Some(e) = engine() else {
        assert!(!on_ci(), "CI must have the bundled packs");
        return;
    };
    let s = sorcerer_then(e, "ID_WOTC_PHB_MULTICLASS_CLERIC");
    let cantrips: Vec<_> = s
        .slots
        .iter()
        .filter(|x| x.name == "Cantrip (Cleric)")
        .collect();
    assert_eq!(cantrips.len(), 3);
    assert!(cantrips.iter().all(|x| !x.options.is_empty()));
    let cleric = s.caster("Cleric").expect("a Cleric block");
    assert!(cleric.prepare_from_list && !cleric.spells.is_empty());
}

#[test]
fn imported_multiclass_cleric_offers_cleric_cantrips() {
    let Some(e) = engine() else {
        assert!(!on_ci(), "CI must have the bundled packs");
        return;
    };
    let s = sorcerer_then(e, PRIEST_MC);
    let cantrips: Vec<_> = s.slots.iter().filter(|x| x.name == CANTRIPS).collect();
    assert_eq!(cantrips.len(), 3, "three cleric cantrip slots");
    for slot in &cantrips {
        let names: Vec<_> = slot.options.iter().map(|o| o.name.as_str()).collect();
        assert!(
            names.contains(&"Guidance") && names.contains(&"Sacred Flame"),
            "{} #{} offers the cleric cantrips, got {names:?}",
            slot.name,
            slot.key.ordinal
        );
        assert!(
            !names.contains(&"Fire Bolt"),
            "never another class's cantrip"
        );
    }
}

#[test]
fn imported_multiclass_cleric_has_a_cleric_block_to_prepare_from() {
    let Some(e) = engine() else {
        assert!(!on_ci(), "CI must have the bundled packs");
        return;
    };
    let s = sorcerer_then(e, PRIEST_MC);
    let cleric = s.caster("Cleric").expect("a Cleric spellcasting block");
    assert_eq!(cleric.ability.to_lowercase(), "wisdom");
    assert!(cleric.prepare && cleric.prepare_from_list);
    // Cleric 1 with Wisdom 14: level 1 + modifier 2
    assert_eq!(cleric.prepare_limit, Some(3));
    let names: Vec<_> = cleric.spells.iter().map(|x| x.name.as_str()).collect();
    assert!(
        names.contains(&"Cure Wounds") && names.contains(&"Bless"),
        "the cleric list of level 1 spells to prepare, got {names:?}"
    );
    assert!(cleric.spells.iter().all(|x| x.level <= 1));
    // the sorcerer keeps its own block and picks
    let sorcerer = s.caster("Sorcerer").expect("the Sorcerer block");
    assert!(!sorcerer.spells.is_empty());
    assert!(!sorcerer.spells.iter().any(|x| x.name == "Cure Wounds"));
}
