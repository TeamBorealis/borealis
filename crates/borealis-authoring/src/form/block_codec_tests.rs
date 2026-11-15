//! Hand-written cases for the catalog block codec: field shapes, the stored-text rule, effect lists, kind changes,
//! sentences and builder problems. The bundled corpus is covered by `tests/form_identity.rs`.

use super::*;
use crate::expr_build::condition::ConditionContext;
use crate::form::{
    check_fields, element_to_form, form_to_element, merge_ui, refresh_blocks, to_ui, FormContext,
    FormState, RefInfo,
};
use borealis_content::{Element, Rule};
use serde_json::{json, Value};
use std::collections::HashMap;

fn with_cx<T>(f: impl FnOnce(&FormContext<'_>) -> T) -> T {
    let lookup: HashMap<String, RefInfo> = HashMap::from([(
        "ID_WIZARD".to_string(),
        RefInfo {
            name: "Wizard".into(),
            ty: "Class".into(),
        },
    )]);
    let codec = CatalogBlockCodec;
    let cx = FormContext {
        conditions: ConditionContext::new(["Wizard", "Monk"]),
        lookup: &lookup,
        blocks: &codec,
    };
    f(&cx)
}

fn with_rules(rules: Value) -> Element {
    serde_json::from_value(json!({
        "id": "ID_GL_FEAT_HOWL", "name": "Howl", "type": "Feat", "source": "Gloom", "rules": rules
    }))
    .expect("element")
}

fn rules(form: &FormState, cx: &FormContext<'_>) -> Value {
    serde_json::to_value(form_to_element(form, cx).expect("writes").rules).expect("json")
}

fn set(form: &FormState, block: usize, field: &str, value: Value) -> FormState {
    let mut f = form.clone();
    f.blocks[block].fields.insert(field.to_string(), value);
    f
}

#[test]
fn blocks_hold_their_catalog_fields_and_a_plain_sentence() {
    with_cx(|cx| {
        let e = with_rules(json!([
            {"kind": "grant", "id": "ID_WIZARD", "type": "Class", "level": 3, "requirements": "[level:3]"},
            {"kind": "stat", "name": "speed", "value": "10", "equipped": "[armor:none]", "merge": true},
            {"kind": "select", "type": "List", "name": "Coat", "items": [{"id": 1, "text": "Grey"}]},
            {"kind": "unknown", "tag": "howl"}
        ]));
        let f = element_to_form(&e, cx);
        let kinds: Vec<&str> = f.blocks.iter().map(|b| b.kind.as_str()).collect();
        assert_eq!(kinds, ["grant", "stat", "list", "unknown"]);
        let grant = &f.blocks[0];
        assert_eq!(
            grant.fields["id"],
            json!({"id": "ID_WIZARD", "name": "Wizard", "type": "Class"})
        );
        assert_eq!(grant.fields["level"], json!(3));
        assert_eq!(grant.fields["requirements"]["original"], json!("[level:3]"));
        assert_eq!(
            grant.fields["requirements"]["value"]["shape"],
            json!("model")
        );
        assert!(
            grant.sentence.starts_with("From level 3: "),
            "{}",
            grant.sentence
        );
        assert!(grant.sentence.contains("Wizard") && !grant.sentence.contains("ID_"));
        let stat = &f.blocks[1];
        assert_eq!(stat.fields["equipped"]["original"], json!("[armor:none]"));
        let kept: Vec<&str> = stat.preserved.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(kept, ["merge"]);
        assert!(
            stat.sentence.contains("while wearing no armor"),
            "{}",
            stat.sentence
        );
        assert_eq!(f.blocks[2].fields["type"], json!("List"));
        assert_eq!(f.blocks[3].preserved[0].key, "rule");
        assert!(f.blocks[3].preserved[0].label.contains("howl"));
        assert_eq!(form_to_element(&f, cx).expect("writes"), e);
    });
}

#[test]
fn an_unchanged_field_keeps_its_stored_text_and_an_edited_one_is_canonical() {
    with_cx(|cx| {
        let e = with_rules(json!([
            {"kind": "resource", "id": "howl", "name": "Howl  Loud",
             "max": {"type": "formula", "expr": "max(1,wisdom:modifier)"}, "recharge": ["long_rest"]},
            {"kind": "attack_bonus", "weapon": "melee", "value": 2}
        ]));
        let f = element_to_form(&e, cx);
        // another field edited: the unchanged ones write their stored text
        let g = set(&f, 0, "recharge", json!(["short_rest"]));
        let out = rules(&g, cx);
        assert_eq!(out[0]["name"], json!("Howl  Loud"));
        assert_eq!(out[0]["max"]["expr"], json!("max(1,wisdom:modifier)"));
        assert_eq!(out[0]["recharge"], json!(["short_rest"]));
        assert_eq!(out[1]["value"], json!(2));
        // edited: canonical
        let mut max = f.blocks[0].fields["max"].clone();
        max["expr"]["value"]["model"]["args"][0]["value"] = json!(2);
        let g = set(&set(&f, 0, "name", json!(" Howl  Loud ")), 0, "max", max);
        let out = rules(&g, cx);
        assert_eq!(out[0]["name"], json!("Howl Loud"));
        assert_eq!(out[0]["max"]["expr"], json!("max(2, wisdom:modifier)"));
        // written canonically, the attack bonus stays a whole number: `attack_rules` reads nothing else
        let mut forgotten = f.clone();
        forgotten.blocks[1].memo = None;
        let out = rules(&forgotten, cx);
        assert_eq!(out[1]["value"], json!(2));
    });
}

#[test]
fn an_edited_effect_list_keeps_the_stored_text_of_unchanged_effects() {
    with_cx(|cx| {
        let e = with_rules(json!([
            {"kind": "toggle", "id": "frenzy", "name": "Frenzy", "effects": [
                {"kind": "stat", "name": "speed", "value": "10", "alt": "Fast  feet"},
                {"kind": "roll_advantage", "roll": "ability_check", "ability": "strength"},
                {"kind": "grant", "id": "ID_WIZARD", "type": "Class"}
            ]}
        ]));
        let f = element_to_form(&e, cx);
        let effects = f.blocks[0].fields["effects"]
            .as_array()
            .expect("effects")
            .clone();
        assert_eq!(effects.len(), 3);
        assert_eq!(effects[1]["kind"], json!("roll_advantage"));
        assert!(effects[1]["sentence"]
            .as_str()
            .is_some_and(|s| !s.is_empty()));
        assert!(effects.iter().all(|x| x.get("memo").is_none()));
        let mut edited = effects.clone();
        edited[1]["fields"]["ability"] = json!("dexterity");
        edited.remove(2);
        let out = rules(&set(&f, 0, "effects", Value::Array(edited)), cx);
        let written = out[0]["effects"].as_array().expect("effects");
        assert_eq!(written.len(), 2);
        assert_eq!(
            written[0]["alt"],
            json!("Fast  feet"),
            "unchanged effect, stored text"
        );
        assert_eq!(written[1]["ability"], json!("dexterity"));
    });
}

#[test]
fn a_block_whose_kind_changed_gets_no_memo_and_no_kept_parts() {
    with_cx(|cx| {
        let e =
            with_rules(json!([{"kind": "stat", "name": "speed", "value": "10", "merge": true}]));
        let stored = element_to_form(&e, cx);
        let mut ui = to_ui(&stored);
        assert!(ui.blocks[0].memo.is_none(), "the memo never travels");
        ui.blocks[0].kind = "grant".into();
        ui.blocks[0].fields = [(
            "id".to_string(),
            json!({"id": "ID_WIZARD", "name": "Wizard", "type": "Class"}),
        )]
        .into();
        let merged = merge_ui(&stored, &ui);
        assert!(merged.blocks[0].memo.is_none() && merged.blocks[0].preserved.is_empty());
        let out = form_to_element(&merged, cx).expect("writes");
        assert!(matches!(&out.rules[0], Rule::Grant(g) if g.id == "ID_WIZARD"));
        // the same kind: memo and kept parts come back from the backend's copy
        let same = merge_ui(&stored, &to_ui(&stored));
        assert_eq!(same, stored);
    });
}

#[test]
fn refresh_writes_the_sentence_of_the_edited_values() {
    with_cx(|cx| {
        let e = with_rules(
            json!([{"kind": "toggle", "id": "frenzy", "name": "Frenzy", "effects": [
            {"kind": "stat", "name": "speed", "value": "10"}
        ]}, {"kind": "stat", "name": "speed", "value": "10"}]),
        );
        let f = element_to_form(&e, cx);
        let mut g = set(&f, 1, "value", json!("30"));
        g.blocks[0]
            .fields
            .get_mut("effects")
            .and_then(|v| v.get_mut(0))
            .expect("effect")["fields"]["value"] = json!("20");
        let r = refresh_blocks(&g, cx);
        assert!(
            r.blocks[1].sentence.contains("+30"),
            "{}",
            r.blocks[1].sentence
        );
        let nested = r.blocks[0].fields["effects"][0]["sentence"]
            .as_str()
            .unwrap();
        assert!(nested.contains("+20"), "{nested}");
        assert_eq!(
            refresh_blocks(&f, cx),
            f,
            "unedited blocks keep their sentences"
        );
    });
}

#[test]
fn builder_problems_and_bad_values_of_blocks_have_block_paths() {
    with_cx(|cx| {
        let e = with_rules(json!([
            {"kind": "resource", "id": "howl", "max": {"type": "formula", "expr": "wisdom:modifier"}}
        ]));
        let f = element_to_form(&e, cx);
        let mut max = f.blocks[0].fields["max"].clone();
        max["expr"]["value"]["model"] = json!({"term": "number", "value": 5000});
        let problems = check_fields(&set(&f, 0, "max", max), cx);
        assert!(
            problems
                .iter()
                .any(|p| p.path.starts_with("blocks.0.fields.max.expr")),
            "{problems:?}"
        );
        let bad = set(&f, 0, "id", json!(3));
        let problems = form_to_element(&bad, cx).unwrap_err();
        assert_eq!(problems[0].path, "blocks.0.fields.id");
        let unknown = set(&f, 0, "made_up", json!(1));
        let problems = form_to_element(&unknown, cx).unwrap_err();
        assert_eq!(problems[0].path, "blocks.0.fields.made_up");
    });
}

#[test]
fn a_new_list_block_writes_a_select_of_type_list() {
    with_cx(|cx| {
        let mut f = element_to_form(&with_rules(json!([])), cx);
        f.blocks.push(Block {
            key: "n1".into(),
            kind: "list".into(),
            sentence: String::new(),
            fields: [
                ("name".to_string(), json!("Coat")),
                ("items".to_string(), json!([{"id": 1, "text": "Grey"}])),
            ]
            .into(),
            preserved: Vec::new(),
            memo: None,
        });
        let out = form_to_element(&f, cx).expect("writes");
        assert!(matches!(&out.rules[0], Rule::Select(s) if s.ty == "List" && s.name == "Coat"));
    });
}
