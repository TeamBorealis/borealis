//! The level sweep on a target built by hand (the import pipeline builds its targets from a plan; its own tests are in
//! `borealis-assist`).

use std::path::Path;

use borealis_content::{ContentStoreBuilder, Ruleset};
use serde_json::{json, Value};

use super::*;

const SUB: &str = "ID_T_ARCHETYPE_VELVET_HAND";
const FEATURE: &str = "ID_T_ARCHETYPE_FEATURE_HIGH_BORN";

fn base() -> ContentStoreBuilder {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut base = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.2.1"] {
        base = base.add_pack_dir(&root.join(pack)).expect("pack loads");
    }
    base
}

fn file(elements: Value) -> ContentFile {
    ContentFile::from_json_str(
        &json!({"format": "borealis-content/1", "elements": elements}).to_string(),
    )
    .expect("content parses")
}

/// A subclass of the bundled Fighter whose level-3 feature has `rules`; `features` must appear.
fn sweep(rules: Value, features: Vec<(String, u32)>) -> Vec<SweepIssue> {
    let sub = json!({"id": SUB, "name": "Velvet Hand", "type": "Archetype", "source": "T",
        "supports": ["Fighter Subclass"], "data": {"class": "ID_2024_CLASS_FIGHTER"},
        "rules": [{"kind": "grant", "type": "Archetype Feature", "id": FEATURE, "level": 3}]});
    let feature = json!({"id": FEATURE, "name": "High Born", "type": "Archetype Feature",
        "source": "T", "data": {"class": "Fighter", "level": 3}, "rules": rules});
    let ids: HashSet<String> = [SUB, FEATURE].into_iter().map(str::to_string).collect();
    let target = SweepTarget {
        class_id: "ID_2024_CLASS_FIGHTER".into(),
        subclass_id: Some(SUB.into()),
        features,
        root_id: SUB.into(),
        casts: false,
    };
    engine_sweep(
        &base(),
        Ruleset::Dnd2024,
        "draft",
        file(json!([sub, feature])),
        &ids,
        &[target],
    )
}

#[test]
fn a_working_subclass_sweeps_clean() {
    let issues = sweep(json!([]), vec![(FEATURE.to_string(), 3)]);
    assert!(issues.is_empty(), "{issues:?}");
}

#[test]
fn a_feature_that_never_appears_is_reported_at_its_level() {
    let issues = sweep(json!([]), vec![("ID_T_NOT_GRANTED".to_string(), 7)]);
    assert!(
        issues.iter().any(|i| i.level == 7
            && i.element_id.as_deref() == Some("ID_T_NOT_GRANTED")
            && i.message.contains("missing at level 7")),
        "{issues:?}"
    );
}

#[test]
fn a_select_with_nothing_to_offer_is_reported_once() {
    let issues = sweep(
        json!([{"kind": "select", "type": "Proficiency", "name": "Nothing Here",
            "supports": "ID_T_NO_SUCH_SUPPORT", "level": 3}]),
        vec![],
    );
    let empty: Vec<_> = issues
        .iter()
        .filter(|i| i.message.contains("offers no options"))
        .collect();
    assert_eq!(empty.len(), 1, "{issues:?}");
    assert_eq!(empty[0].element_id.as_deref(), Some(FEATURE));
    assert_eq!(empty[0].level, 3);
}

#[test]
fn store_errors_stop_the_sweep() {
    // A spell without its required setters is a store error of the draft's own element.
    let bad = json!({"id": "ID_T_SPELL_BAD", "name": "Bad", "type": "Spell", "source": "T"});
    let ids: HashSet<String> = ["ID_T_SPELL_BAD".to_string()].into();
    let target = SweepTarget {
        class_id: "ID_2024_CLASS_FIGHTER".into(),
        subclass_id: None,
        features: vec![("ID_T_NOT_GRANTED".into(), 2)],
        root_id: "ID_T_SPELL_BAD".into(),
        casts: false,
    };
    let issues = engine_sweep(
        &base(),
        Ruleset::Dnd2024,
        "draft",
        file(json!([bad])),
        &ids,
        &[target],
    );
    assert!(!issues.is_empty());
    assert!(
        issues
            .iter()
            .all(|i| i.level == 0 && i.element_id.as_deref() == Some("ID_T_SPELL_BAD")),
        "{issues:?}"
    );
}

#[test]
fn a_subclass_the_parent_never_offers_is_reported() {
    let sub = json!({"id": SUB, "name": "Velvet Hand", "type": "Archetype", "source": "T",
        "supports": ["Nobody's Subclass"], "rules": []});
    let ids: HashSet<String> = [SUB.to_string()].into();
    let target = SweepTarget {
        class_id: "ID_2024_CLASS_FIGHTER".into(),
        subclass_id: Some(SUB.into()),
        features: vec![],
        root_id: SUB.into(),
        casts: false,
    };
    let issues = engine_sweep(
        &base(),
        Ruleset::Dnd2024,
        "draft",
        file(json!([sub])),
        &ids,
        &[target],
    );
    assert!(
        issues
            .iter()
            .any(|i| i.message.contains("never offers this subclass")),
        "{issues:?}"
    );
}
