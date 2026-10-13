//! `[type:x]` requirement terms (`engine.md` 5.1) and `Companion` selects that offer content
//! companions plus the bundled SRD beasts, on small synthetic content over the bundled packs.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::Engine;
use serde_json::{json, Value};
use std::path::Path;

const COMPANION_SUPPORTS: &str = "Beast,(Tiny||Small||Medium),(1/4||1/8||0)";

fn engine(packs: &[&str], extra: Vec<Value>) -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let file: ContentFile = serde_json::from_value(
        json!({"format": "borealis-content/1", "ruleset": "dnd2014", "elements": extra}),
    )
    .expect("synthetic content");
    let mut b = ContentStoreBuilder::new();
    for p in packs {
        b = b.add_pack_dir(&root.join(p)).expect("bundled pack");
    }
    Engine::new(&b.add_file("synthetic.json", PackOrigin::Core, file).build())
}

fn companion(id: &str, name: &str, ty: &str, size: &str, cr: &str) -> Value {
    json!({"id": id, "name": name, "type": "Companion", "source": "Test", "setters": [
        {"name": "type", "value": ty}, {"name": "size", "value": size}, {"name": "challenge", "value": cr},
        {"name": "strength", "value": "12"}, {"name": "dexterity", "value": "15"},
        {"name": "constitution", "value": "12"}, {"name": "intelligence", "value": "3"},
        {"name": "wisdom", "value": "12"}, {"name": "charisma", "value": "6"},
        {"name": "alignment", "value": "Unaligned"}, {"name": "ac", "value": "13"}]})
}

fn content() -> Vec<Value> {
    vec![
        json!({"id": "ID_T_CASTER", "name": "Tcaster", "type": "Class", "source": "Test",
               "setters": [{"name": "hd", "value": "d8"}], "rules": [
            {"kind": "grant", "id": "ID_T_SPELL", "type": "Spell", "level": 1},
            {"kind": "select", "type": "Feat", "name": "Test Feat", "level": 1},
            {"kind": "select", "type": "Companion", "name": "Beast Friend", "supports": COMPANION_SUPPORTS, "level": 1}]}),
        json!({"id": "ID_T_MUNDANE", "name": "Tmundane", "type": "Class", "source": "Test",
               "setters": [{"name": "hd", "value": "d10"}], "rules": [
            {"kind": "select", "type": "Feat", "name": "Test Feat", "level": 1}]}),
        json!({"id": "ID_T_SPELL", "name": "Test Bolt", "type": "Spell", "source": "Test",
               "setters": [{"name": "level", "value": "0"}, {"name": "school", "value": "Evocation"}]}),
        json!({"id": "ID_T_FEAT_CASTER", "name": "Caster Feat", "type": "Feat", "source": "Test",
               "requirements": "[type:spell]"}),
        json!({"id": "ID_T_FEAT_CASTER_UPPER", "name": "Caster Feat Upper", "type": "Feat", "source": "Test",
               "requirements": "[type:Spell],[str:10]"}),
        json!({"id": "ID_T_FEAT_MUNDANE", "name": "Mundane Feat", "type": "Feat", "source": "Test",
               "requirements": "![type:spell]"}),
        json!({"id": "ID_T_FEAT_EQUIPPED_SYNTAX", "name": "Shield Feat", "type": "Feat", "source": "Test",
               "requirements": "[shield:any]"}),
        companion("ID_T_COMP_WOLF", "Wolf", "Beast", "Medium", "1/4"),
        companion("ID_T_COMP_BEAR", "Big Bear", "Beast", "Large", "1"),
        companion("ID_T_COMP_IMP", "Little Devil", "Fiend", "Tiny", "1/4"),
    ]
}

fn with_class(e: &Engine, class: &str) -> CharacterState {
    let s = commands::new_character(e, Ruleset::Dnd2014);
    let s = commands::set_base_scores(e, &s, [12, 12, 12, 12, 12, 12]).map_or(s, |o| o.state);
    let key = slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    commands::select(e, &s, &key, Choice::Element(class.into()))
        .expect("class")
        .state
}

fn options(snap: &CharacterSnapshot, name: &str) -> Vec<String> {
    snap.slots
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no slot {name}"))
        .options
        .iter()
        .map(|o| o.id.clone())
        .collect()
}

#[test]
fn type_terms_gate_feats_on_present_element_types() {
    let e = engine(&["core"], content());
    let caster = options(&e.derive(&with_class(&e, "ID_T_CASTER")), "Test Feat");
    assert!(
        caster.contains(&"ID_T_FEAT_CASTER".to_string()),
        "{caster:?}"
    );
    assert!(caster.contains(&"ID_T_FEAT_CASTER_UPPER".to_string()));
    assert!(!caster.contains(&"ID_T_FEAT_MUNDANE".to_string()));
    assert!(!caster.contains(&"ID_T_FEAT_EQUIPPED_SYNTAX".to_string()));

    let mundane = options(&e.derive(&with_class(&e, "ID_T_MUNDANE")), "Test Feat");
    assert!(
        !mundane.contains(&"ID_T_FEAT_CASTER".to_string()),
        "{mundane:?}"
    );
    assert!(!mundane.contains(&"ID_T_FEAT_CASTER_UPPER".to_string()));
    assert!(mundane.contains(&"ID_T_FEAT_MUNDANE".to_string()));
}

#[test]
fn text_test_values_are_not_bad_expressions() {
    let e = engine(&["core"], content());
    let snap = e.derive(&with_class(&e, "ID_T_CASTER"));
    let bad: Vec<_> = snap
        .diagnostics
        .iter()
        .filter(|d| d.code == "content.bad_expression")
        .filter(|d| {
            d.element
                .as_deref()
                .is_some_and(|el| el.starts_with("ID_T_"))
        })
        .collect();
    assert!(bad.is_empty(), "{bad:?}");
}

#[test]
fn companion_select_filters_content_companions_by_derived_tags() {
    let e = engine(&["core"], content());
    let opts = options(&e.derive(&with_class(&e, "ID_T_CASTER")), "Beast Friend");
    assert_eq!(opts, vec!["ID_T_COMP_WOLF".to_string()]);
}

#[test]
fn companion_select_offers_bundled_beasts_and_prefers_content_companions() {
    let e = engine(&["core", "srd-5.1", "srd-5.1-monsters"], content());
    let state = with_class(&e, "ID_T_CASTER");
    let opts = options(&e.derive(&state), "Beast Friend");
    for id in [
        "ID_T_COMP_WOLF",
        "ID_SRD51_MONSTER_CAT",
        "ID_SRD51_MONSTER_MASTIFF",
    ] {
        assert!(opts.contains(&id.to_string()), "{id} missing from {opts:?}");
    }
    for id in [
        "ID_SRD51_MONSTER_WOLF",       // a content companion of the same name wins
        "ID_SRD51_MONSTER_BROWN_BEAR", // Large, CR 1
        "ID_T_COMP_BEAR",
        "ID_T_COMP_IMP",
        "ID_SRD51_MONSTER_IMP", // fiend
    ] {
        assert!(!opts.contains(&id.to_string()), "{id} offered: {opts:?}");
    }
    assert!(opts.len() >= 10, "{}", opts.len());

    let snap = e.derive(&state);
    let key = slot_key(&snap, "ID_T_CASTER", "Beast Friend", 1).expect("companion slot");
    let chosen = commands::select(
        &e,
        &state,
        &key,
        Choice::Element("ID_SRD51_MONSTER_CAT".into()),
    )
    .expect("pick cat")
    .state;
    let snap = e.derive(&chosen);
    assert!(snap
        .elements
        .iter()
        .any(|r| r.element.id == "ID_SRD51_MONSTER_CAT"));
    let slot = snap
        .slots
        .iter()
        .find(|s| s.name == "Beast Friend")
        .unwrap();
    assert_eq!(
        slot.chosen.as_ref().map(|c| c.id.as_str()),
        Some("ID_SRD51_MONSTER_CAT")
    );
    assert!(snap.invalid_selections.is_empty());
}

#[test]
fn foreign_edition_beasts_are_not_offered() {
    let e = engine(
        &["core", "srd-5.1", "srd-5.1-monsters", "srd-5.2.1-monsters"],
        content(),
    );
    let opts = options(&e.derive(&with_class(&e, "ID_T_CASTER")), "Beast Friend");
    assert!(opts.contains(&"ID_SRD51_MONSTER_CAT".to_string()));
    assert!(
        !opts.iter().any(|o| o.starts_with("ID_SRD521_")),
        "{opts:?}"
    );
}
