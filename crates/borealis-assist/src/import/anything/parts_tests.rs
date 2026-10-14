//! The plan's pick written into an element: the model's own version of the pick goes.

use serde_json::{json, Value};

use super::super::types::{Edition, ImportPlan, OptionPicks, PickCount, PlannedElement, Role};

fn planned(id: &str, name: &str, role: Role, parent: Option<&str>) -> PlannedElement {
    PlannedElement {
        id: id.into(),
        ty: "Class Feature".into(),
        name: name.into(),
        role,
        level: Some(3),
        owner: None,
        brief: String::new(),
        excerpt: None,
        parent: parent.map(str::to_string),
        picks: None,
        prerequisite_level: None,
        spells: Vec::new(),
    }
}

fn plan_with_gift(choose: Vec<PickCount>) -> ImportPlan {
    let mut gift = planned("ID_T_GIFT", "Grove Gift", Role::ClassFeature, None);
    gift.picks = Some(OptionPicks {
        key: "Warden Grove Gift".into(),
        ty: "Class Feature".into(),
        choose,
        spellcasting: None,
    });
    ImportPlan {
        edition: Edition::Y2014,
        groups: Vec::new(),
        elements: vec![
            gift,
            planned(
                "ID_T_GIFT_OF_BARK",
                "Gift of Bark",
                Role::Option,
                Some("ID_T_GIFT"),
            ),
            planned(
                "ID_T_GIFT_OF_DEW",
                "Gift of Dew",
                Role::Option,
                Some("ID_T_GIFT"),
            ),
        ],
        notes: Vec::new(),
    }
}

fn selects(el: &Value) -> Vec<Value> {
    el["rules"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["kind"] == "select")
        .cloned()
        .collect()
}

#[test]
fn a_model_select_named_like_the_planned_pick_is_replaced() {
    let plan = plan_with_gift(vec![PickCount { level: 3, count: 1 }]);
    let mut el = json!({"rules": [
        {"kind": "select", "type": "Class Feature", "name": "Grove Gift", "supports": "Grove Gifts"},
        {"kind": "select", "type": "Spell", "name": "Grove Gift", "supports": "Warden,1"}]});
    super::apply(&mut el, &plan.elements[0], &plan);
    let got = selects(&el);
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!(got[0]["type"], "Spell", "another type's select stays");
    assert_eq!(got[1]["supports"], "Warden Grove Gift");
}

#[test]
fn a_model_select_over_the_planned_option_ids_is_replaced() {
    let plan = plan_with_gift(vec![PickCount { level: 3, count: 1 }]);
    let mut el = json!({"rules": [
        {"kind": "select", "type": "Class Feature", "name": "Pick a gift", "level": 3,
         "supports": "ID_T_GIFT_OF_BARK|ID_T_GIFT_OF_DEW"}]});
    super::apply(&mut el, &plan.elements[0], &plan);
    let got = selects(&el);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0]["supports"], "Warden Grove Gift");
}

#[test]
fn a_model_select_with_a_level_suffix_is_replaced_by_the_tiered_picks() {
    let plan = plan_with_gift(vec![
        PickCount { level: 3, count: 1 },
        PickCount { level: 7, count: 1 },
    ]);
    let mut el = json!({"rules": [
        {"kind": "select", "type": "Class Feature", "name": "Grove Gift (Level 7)", "level": 7,
         "supports": "Gifts"},
        {"kind": "select", "type": "Class Feature", "name": "Grove Gift Extra", "supports": "Other"}]});
    super::apply(&mut el, &plan.elements[0], &plan);
    let names: Vec<String> = selects(&el)
        .iter()
        .map(|s| s["name"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        names,
        [
            "Grove Gift Extra",
            "Grove Gift (Level 3)",
            "Grove Gift (Level 7)"
        ]
    );
    assert_eq!(super::pick_name("Grove Gift (Level 12)"), "grove gift");
    assert_eq!(super::pick_name("Grove Gift (Lore)"), "grove gift (lore)");
}
