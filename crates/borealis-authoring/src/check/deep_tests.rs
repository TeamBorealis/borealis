//! The engine checks of drafts on the 2024 SRD: a subclass swept with the class that offers it, a choice with nothing
//! to offer, a feat the test character can't take, a balance hint.

use super::*;
use crate::form::{element_group_to_form, element_to_form, CatalogBlockCodec, FormContext};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::OnceLock;

fn base() -> &'static ContentStoreBuilder {
    static BASE: OnceLock<ContentStoreBuilder> = OnceLock::new();
    BASE.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let mut b = ContentStoreBuilder::new();
        for pack in ["core", "srd-5.2.1"] {
            b = b.add_pack_dir(&root.join(pack)).expect("pack loads");
        }
        b
    })
}

fn loaded() -> &'static ContentStore {
    static STORE: OnceLock<ContentStore> = OnceLock::new();
    STORE.get_or_init(|| base().clone().build())
}

fn element(v: Value) -> Element {
    serde_json::from_value(v).expect("element")
}

fn run(root: &Element, children: &[Element]) -> Vec<Problem> {
    let codec = CatalogBlockCodec;
    let cx = FormContext::from_store(loaded(), &codec);
    let kids: Vec<&Element> = children.iter().collect();
    let form = if children.is_empty() {
        element_to_form(root, &cx)
    } else {
        element_group_to_form(root, &kids, &cx)
    };
    deep_problems(&DeepInput {
        base: base(),
        ruleset: Ruleset::Dnd2024,
        pack_name: "mine",
        form: &form,
        written: None,
        cx: &cx,
    })
}

const SUB: &str = "ID_T_ARCHETYPE_VELVET_HAND";
const FEATURE: &str = "ID_T_ARCHETYPE_FEATURE_HIGH_BORN";

fn subclass(feature_rules: Value) -> (Element, Element) {
    let sub = element(
        json!({"id": SUB, "name": "Velvet Hand", "type": "Archetype", "source": "T",
        "supports": ["Fighter Subclass"], "description": "<p>x</p>",
        "rules": [{"kind": "grant", "type": "Archetype Feature", "id": FEATURE, "level": 3}]}),
    );
    let feature = element(
        json!({"id": FEATURE, "name": "High Born", "type": "Archetype Feature",
        "source": "T", "description": "<p>x</p>", "data": {"class": "Fighter", "level": 3}, "rules": feature_rules}),
    );
    (sub, feature)
}

#[test]
fn a_subclass_is_swept_with_the_class_that_offers_it() {
    let (sub, feature) = subclass(json!([]));
    let targets = sweep_targets(&[sub.clone(), feature.clone()], loaded(), Ruleset::Dnd2024);
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].class_id, "ID_2024_CLASS_FIGHTER");
    assert_eq!(targets[0].features, vec![(FEATURE.to_string(), 3)]);
    assert_eq!(run(&sub, &[feature]), vec![]);
}

#[test]
fn a_choice_with_nothing_to_offer_is_told_at_its_level_and_block() {
    let (sub, feature) = subclass(
        json!([{"kind": "select", "type": "Proficiency", "name": "Nothing Here",
        "supports": "ID_T_NO_SUCH_SUPPORT", "level": 3}]),
    );
    let problems = run(&sub, &[feature]);
    let empty: Vec<&Problem> = problems
        .iter()
        .filter(|p| p.code == "sweep.empty_select")
        .collect();
    assert_eq!(empty.len(), 1, "{problems:?}");
    assert_eq!(
        empty[0].message,
        "At level 3 the choice \"Nothing Here\" has nothing to offer."
    );
    assert_eq!(empty[0].path.as_deref(), Some("children.0.blocks.0"));
    assert_eq!(empty[0].element_id.as_deref(), Some(FEATURE));
    assert_eq!(empty[0].severity, ProblemSeverity::Warning);
}

#[test]
fn a_subclass_no_class_offers_is_told() {
    let sub = element(
        json!({"id": SUB, "name": "Velvet Hand", "type": "Archetype", "source": "T",
        "supports": ["Nobody's Subclass"], "description": "<p>x</p>"}),
    );
    let problems = run(&sub, &[]);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].code, "sweep.subclass_never_offered");
    assert_eq!(problems[0].path.as_deref(), Some("fields.tags"));
}

#[test]
fn a_feat_choice_from_a_filter_nothing_matches_is_told_without_a_sweep() {
    let feat = element(
        json!({"id": "ID_T_FEAT_ODD", "name": "Odd", "type": "Feat", "source": "T",
        "description": "<p>x</p>",
        "rules": [{"kind": "select", "type": "Language", "name": "Odd Tongue", "supports": "Nobody Speaks This"},
            {"kind": "select", "type": "Language", "name": "Any Tongue", "supports": "Standard"}]}),
    );
    let problems = run(&feat, &[]);
    let empty: Vec<(&str, &str)> = problems
        .iter()
        .filter(|p| p.code == "lint.select_empty")
        .map(|p| (p.path.as_deref().unwrap_or(""), p.message.as_str()))
        .collect();
    assert_eq!(
        empty,
        vec![(
            "blocks.0",
            "Nothing loaded matches what this choice offers. Check what it lets the player choose from."
        )],
        "{problems:?}"
    );
}

#[test]
fn a_spell_far_above_the_srd_gets_a_balance_hint() {
    let spell = element(
        json!({"id": "ID_T_SPELL_DOOM", "name": "Doom", "type": "Spell", "source": "T",
        "supports": ["Wizard"],
        "description": "<p>Each creature in a 20-foot-radius sphere takes 12d10 fire damage on a failed save.</p>",
        "setters": [{"name": "level", "value": "1"}, {"name": "school", "value": "Evocation"},
            {"name": "time", "value": "Action"}, {"name": "duration", "value": "Instantaneous"},
            {"name": "range", "value": "150 feet"}]}),
    );
    let problems = run(&spell, &[]);
    let hints: Vec<&Problem> = problems
        .iter()
        .filter(|p| p.code.starts_with("balance."))
        .collect();
    assert_eq!(hints.len(), 1, "{problems:?}");
    assert_eq!(hints[0].severity, ProblemSeverity::Info);
    assert!(
        hints[0]
            .message
            .starts_with("Compared with SRD spells of its level: deals far more damage"),
        "{}",
        hints[0].message
    );
}

#[test]
fn a_draft_that_does_not_load_has_no_engine_problems() {
    let bad =
        element(json!({"id": "ID_T_SPELL_BAD", "name": "Bad", "type": "Spell", "source": "T"}));
    assert_eq!(run(&bad, &[]), vec![]);
}
