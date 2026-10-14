//! The sweep judges a select only after the picks a player would make first.

use std::path::Path;

use borealis_content::ContentStoreBuilder;
use borealis_content::Ruleset;
use serde_json::{json, Value};

use super::*;
use crate::import::anything::types::{ImportPlan, PlannedElement};

const SUB: &str = "ID_T_ARCHETYPE_VELVET_HAND";
const FEATURE: &str = "ID_T_ARCHETYPE_FEATURE_HIGH_BORN";

fn ctx() -> PackContext {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut base = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.2.1"] {
        base = base.add_pack_dir(&root.join(pack)).expect("pack loads");
    }
    PackContext {
        base,
        ruleset: Ruleset::Dnd2024,
    }
}

fn planned(id: &str, ty: &str, role: Role, owner: Option<&str>) -> PlannedElement {
    serde_json::from_value(json!({"id": id, "ty": ty, "name": id, "role": role,
        "level": 3, "owner": owner, "brief": "", "excerpt": null}))
    .expect("planned element")
}

fn drafted(id: &str, ty: &str, role: Role, json: Value) -> DraftElement {
    serde_json::from_value(
        json!({"id": id, "name": id, "ty": ty, "role": role, "level": 3,
        "json": json, "status": "ok", "issues": [], "excerpt": null, "balance": null,
        "repair_rounds": 0, "revisions": 0}),
    )
    .expect("draft element")
}

/// A subclass of the bundled Fighter whose level-3 feature has `rules`.
fn sweep_with(rules: Value) -> Vec<SweepIssue> {
    let plan: ImportPlan = serde_json::from_value(json!({"edition": "2024", "notes": [],
        "groups": [{"kind": "subclass", "name": "Velvet Hand", "root_id": SUB, "outline": null,
            "parent": {"class_id": "ID_2024_CLASS_FIGHTER", "class_name": "Fighter",
                "supports": "Fighter Subclass", "subclass_levels": [3, 7, 10, 15, 18]},
            "choice": null}],
        "elements": [planned(SUB, "Archetype", Role::Subclass, None),
            planned(FEATURE, "Archetype Feature", Role::SubclassFeature, Some(SUB))]}))
    .expect("plan");
    let sub = json!({"id": SUB, "name": "Velvet Hand", "type": "Archetype", "source": "T",
        "supports": ["Fighter Subclass"], "data": {"class": "ID_2024_CLASS_FIGHTER"},
        "rules": [{"kind": "grant", "type": "Archetype Feature", "id": FEATURE, "level": 3}]});
    let feature = json!({"id": FEATURE, "name": "High Born", "type": "Archetype Feature",
        "source": "T", "data": {"class": "Fighter", "level": 3}, "rules": rules});
    let elements = vec![
        drafted(SUB, "Archetype", Role::Subclass, sub),
        drafted(FEATURE, "Archetype Feature", Role::SubclassFeature, feature),
    ];
    engine_sweep(&ctx(), &plan, &elements)
}

#[test]
fn expertise_over_skills_picked_first_is_not_an_empty_select() {
    // Two skill proficiencies, then expertise in one of the character's skills: the expertise
    // options exist once the skills are picked.
    let issues = sweep_with(json!([
        {"kind": "select", "type": "Proficiency", "name": "High Born (Skill)", "number": 2,
         "supports": "Skill", "level": 3},
        {"kind": "expertise", "count": 1, "level": 3}
    ]));
    assert!(
        issues
            .iter()
            .all(|i| !i.message.contains("offers no options")),
        "{issues:?}"
    );
}

#[test]
fn a_select_with_nothing_to_offer_is_still_flagged() {
    let issues = sweep_with(json!([
        {"kind": "select", "type": "Proficiency", "name": "Nothing Here",
         "supports": "ID_T_NO_SUCH_SUPPORT", "level": 3}
    ]));
    assert!(
        issues
            .iter()
            .any(|i| i.element_id.as_deref() == Some(FEATURE)
                && i.message.contains("offers no options")),
        "{issues:?}"
    );
}
