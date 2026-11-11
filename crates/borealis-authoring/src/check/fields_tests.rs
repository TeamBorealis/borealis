//! Form checks by hand: required fields of forms and blocks, ranges, dice, closed choices, nested effects, and parts
//! kept from the original never judged. The bundled corpus is covered by `tests/check_live.rs`.

use super::*;
use crate::expr_build::condition::ConditionContext;
use crate::form::{element_to_form, CatalogBlockCodec, FormContext, FormState, RefInfo};
use borealis_content::Element;
use serde_json::{json, Value};
use std::collections::HashMap;

fn with_cx<T>(f: impl FnOnce(&FormContext<'_>) -> T) -> T {
    let lookup: HashMap<String, RefInfo> = HashMap::from([(
        "ID_T_SPELL_SPARK".to_string(),
        RefInfo {
            name: "Spark".into(),
            ty: "Spell".into(),
        },
    )]);
    let codec = CatalogBlockCodec;
    let cx = FormContext {
        conditions: ConditionContext::new(["Wizard"]),
        lookup: &lookup,
        blocks: &codec,
    };
    f(&cx)
}

fn feat(rules: Value) -> Element {
    serde_json::from_value(json!({
        "id": "ID_T_FEAT_HOWL", "name": "Howl", "type": "Feat", "source": "T", "rules": rules
    }))
    .expect("element")
}

fn messages(found: &[At]) -> Vec<(String, String)> {
    found
        .iter()
        .map(|(p, f)| (p.clone(), f.message()))
        .collect()
}

fn set(form: &FormState, block: usize, field: &str, value: Value) -> FormState {
    let mut f = form.clone();
    f.blocks[block].fields.insert(field.to_string(), value);
    f
}

#[test]
fn an_empty_name_and_an_empty_required_field_are_told_in_plain_words() {
    with_cx(|cx| {
        let form = FormState::new("e", "deity")
            .with_field("name", json!(" "))
            .with_field("domains", json!(["Light"]))
            .with_field("symbol", json!("A sun"));
        assert_eq!(
            messages(&form_findings(&form, cx)),
            vec![
                ("fields.name".into(), "Give it a name.".into()),
                (
                    "fields.alignment".into(),
                    "A deity needs an alignment.".into()
                ),
            ]
        );
    });
}

#[test]
fn a_new_block_without_its_required_values_names_them() {
    with_cx(|cx| {
        let mut form = FormState::new("e", "other")
            .with_field("name", json!("Howl"))
            .with_field("type", json!("Property"));
        form.blocks.push(
            serde_json::from_value(json!({"key": "b1", "kind": "grant", "fields": {}}))
                .expect("block"),
        );
        form.blocks.push(
            serde_json::from_value(json!({"key": "b2", "kind": "resource_cost", "fields": {"resource": "", "cost": 1}}))
                .expect("block"),
        );
        assert_eq!(
            messages(&form_findings(&form, cx)),
            vec![
                (
                    "blocks.0.fields.id".into(),
                    "Gives: fill in what it gives.".into()
                ),
                (
                    "blocks.1.fields.resource".into(),
                    "Costs uses of a pool: fill in pool.".into()
                ),
            ]
        );
    });
}

#[test]
fn numbers_out_of_range_and_dice_that_are_not_dice_are_found_after_an_edit() {
    with_cx(|cx| {
        let e = feat(json!([
            {"kind": "grant", "id": "ID_T_SPELL_SPARK", "type": "Spell", "level": 3},
            {"kind": "healing", "name": "Mend", "dice": "1d8"}
        ]));
        let form = element_to_form(&e, cx);
        assert!(
            form_findings(&form, cx).is_empty(),
            "{:?}",
            messages(&form_findings(&form, cx))
        );
        let edited = set(&set(&form, 0, "level", json!(25)), 1, "dice", json!("lots"));
        assert_eq!(
            messages(&form_findings(&edited, cx)),
            vec![
                (
                    "blocks.0.fields.level".into(),
                    "Gives: from level must be a whole number from 1 to 20.".into()
                ),
                (
                    "blocks.1.fields.dice".into(),
                    "Healing ability: write dice as dice, like 2d6 or 1d8+2.".into()
                ),
            ]
        );
    });
}

#[test]
fn a_value_outside_the_engine_choices_is_found_but_a_kept_one_is_not() {
    with_cx(|cx| {
        let kept =
            feat(json!([{"kind": "grant_spell", "spell": "ID_T_SPELL_SPARK", "mode": "innate"}]));
        let form = element_to_form(&kept, cx);
        assert!(!form.blocks[0].preserved.is_empty(), "the odd mode is kept");
        assert!(form_findings(&form, cx).is_empty());
        let ok =
            feat(json!([{"kind": "grant_spell", "spell": "ID_T_SPELL_SPARK", "mode": "known"}]));
        let edited = set(&element_to_form(&ok, cx), 0, "mode", json!("innate"));
        assert_eq!(
            messages(&form_findings(&edited, cx)),
            vec![(
                "blocks.0.fields.mode".into(),
                "Gives a spell: pick how from the list.".into()
            )]
        );
    });
}

#[test]
fn nested_effects_are_checked_at_their_own_paths() {
    with_cx(|cx| {
        let e = feat(json!([{"kind": "toggle", "id": "rage", "name": "Rage",
            "effects": [{"kind": "stat", "name": "speed", "value": "10"}]}]));
        let form = element_to_form(&e, cx);
        assert!(form_findings(&form, cx).is_empty());
        let mut effects = form.blocks[0].fields["effects"].clone();
        effects[0]["fields"]["name"] = json!("");
        let edited = set(&form, 0, "effects", effects);
        assert_eq!(
            messages(&form_findings(&edited, cx)),
            vec![(
                "blocks.0.fields.effects.0.fields.name".into(),
                "Changes a number: fill in statistic.".into()
            )]
        );
    });
}

#[test]
fn children_are_checked_with_their_prefix() {
    with_cx(|cx| {
        let mut form = FormState::new("e", "other")
            .with_field("name", json!("Howl"))
            .with_field("type", json!("Property"));
        form.children.push(
            FormState::new("c", "other")
                .with_field("name", json!(""))
                .with_field("type", json!("Companion Trait")),
        );
        assert_eq!(
            messages(&form_findings(&form, cx)),
            vec![("children.0.fields.name".into(), "Give it a name.".into())]
        );
    });
}

#[test]
fn dice_text() {
    for ok in ["2d6", "1d8+2", "d4", "1d6 + 1d4", "3", "-1d4+2", "1D10"] {
        assert!(is_dice(ok), "{ok}");
    }
    for bad in ["", "lots", "2d", "d", "1d6+", "2x6", "1d6*2"] {
        assert!(!is_dice(bad), "{bad}");
    }
}
