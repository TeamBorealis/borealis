//! The small types' own fields (spec 4.10) and the generic form's rows (4.12), on hand-written cases: each field read
//! from every location it binds, an unchanged field writes the stored bytes back, an edit rewrites every location (2024
//! mirrors only when the element has them or is a new 2024 one). The bundled elements are covered by
//! `tests/form_identity.rs`.

use crate::expr_build::condition::ConditionContext;
use crate::form::{
    element_to_form, form_to_element, kind, CatalogBlockCodec, FormContext, FormState, NoLookup,
};
use borealis_content::Element;
use serde_json::{json, Value};

fn with_cx<T>(f: impl FnOnce(&FormContext<'_>) -> T) -> T {
    let codec = CatalogBlockCodec;
    let cx = FormContext {
        conditions: ConditionContext::new(Vec::<String>::new()),
        lookup: &NoLookup,
        blocks: &codec,
    };
    f(&cx)
}

fn el(v: Value) -> Element {
    serde_json::from_value(v).expect("element")
}

fn one(form: &FormState, cx: &FormContext<'_>) -> Element {
    form_to_element(form, cx).expect("writes")
}

fn ids(v: &Value) -> Vec<&str> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r["id"].as_str())
        .collect()
}

/// The core shape of an armor group: plain grants, then a grant on a level (a block) and a statistic (a block).
fn armor_group() -> Element {
    el(json!({
        "id": "ID_X_PROFICIENCY_LIGHT", "name": "Armor Proficiency (Light Armor)", "type": "Proficiency",
        "source": "Core", "supports": ["Armor"],
        "rules": [
            {"kind": "stat", "name": "light armor:proficiency", "value": "1"},
            {"kind": "grant", "id": "ID_PADDED", "type": "Proficiency"},
            {"kind": "grant", "id": "ID_LEATHER", "type": "Proficiency"},
            {"kind": "grant", "id": "ID_STUDDED", "type": "Proficiency", "level": 5}
        ],
        "description": "Light armor."
    }))
}

#[test]
fn a_proficiency_group_holds_its_plain_grants_in_what_it_covers() {
    with_cx(|cx| {
        let f = element_to_form(&armor_group(), cx);
        assert_eq!(f.kind, "proficiency");
        assert_eq!(ids(&f.fields["covers"]), ["ID_PADDED", "ID_LEATHER"]);
        let kinds: Vec<&str> = f.blocks.iter().map(|b| b.kind.as_str()).collect();
        assert_eq!(kinds, ["stat", "grant"], "a levelled grant stays a block");
        assert_eq!(one(&f, cx), armor_group());
        // an edit keeps the other rules where they were and the covers grants in their places
        let covers = json!([{"id": "ID_PADDED", "name": "", "type": "Proficiency"},
                            {"id": "ID_HIDE", "name": "", "type": "Proficiency"},
                            {"id": "ID_CHAIN", "name": "", "type": "Proficiency"}]);
        let e = one(&f.with_field("covers", covers), cx);
        let rules: Vec<Value> = e
            .rules
            .iter()
            .map(|r| serde_json::to_value(r).unwrap())
            .collect();
        assert_eq!(rules[0]["kind"], "stat");
        assert_eq!(
            rules[1..3],
            [
                json!({"kind": "grant", "id": "ID_PADDED", "type": "Proficiency"}),
                json!({"kind": "grant", "id": "ID_HIDE", "type": "Proficiency"})
            ]
        );
        assert_eq!(
            rules[3]["id"], "ID_CHAIN",
            "a new one after the stored ones"
        );
        assert_eq!(rules[4]["level"], 5);
        // a new group: its grants lead
        let new = kind("proficiency")
            .unwrap()
            .new_form("e")
            .with_field("name", json!("Hide Weaves"))
            .with_field(
                "covers",
                json!([{"id": "ID_HIDE", "name": "", "type": "Proficiency"}]),
            );
        let mut new = new;
        new.memo.id = Some("ID_X_PROFICIENCY_HIDE_WEAVES".into());
        let e = one(&new, cx);
        assert_eq!(
            serde_json::to_value(&e.rules).unwrap(),
            json!([{"kind": "grant", "id": "ID_HIDE", "type": "Proficiency"}])
        );
    });
}

fn condition_2024() -> Element {
    el(json!({
        "id": "ID_2024_CONDITION_DAZED", "name": "Dazed", "type": "Condition", "source": "X",
        "ruleset": "dnd2024", "supports": ["Condition", "Mind"], "description": "<p>Dazed.</p>",
        "data": {"tag": "Condition"}
    }))
}

#[test]
fn a_condition_is_listed_by_its_tag_and_mirror() {
    with_cx(|cx| {
        let f = element_to_form(&condition_2024(), cx);
        assert_eq!(f.kind, "condition");
        assert_eq!(f.fields["listed"], true);
        assert_eq!(f.fields["tags"], json!(["Mind"]));
        assert!(f.preserved.is_empty());
        assert_eq!(one(&f, cx), condition_2024());
        let off = one(&f.with_field("listed", json!(false)), cx);
        assert_eq!(off.supports, ["Mind"]);
        assert_eq!(
            serde_json::to_value(&off.data).unwrap(),
            json!({"listed": false}),
            "marked unlisted (IpY2E)"
        );
        // a 2014 condition has no tag and no mirror: listing it adds the tag only
        let old = el(
            json!({"id": "ID_X_CONDITION_RESIST", "name": "Resistance (Fire)", "type": "Condition",
                            "source": "Internal", "supports": ["Resistance"], "description": "Fire."}),
        );
        let f = element_to_form(&old, cx);
        assert_eq!(f.fields["listed"], false);
        assert_eq!(one(&f, cx), old);
        let on = one(&f.with_field("listed", json!(true)), cx);
        assert_eq!(on.supports, ["Resistance", "Condition"]);
        assert!(on.data.is_empty());
    });
}

/// IpY2E: the Play tab lists every condition that is not an engine helper (the SRD 5.1 conditions carry no tag), so an
/// unlisted condition is marked `data.listed: false`, in both editions; listing it again takes the mark off.
#[test]
fn an_unlisted_condition_is_marked_so_the_play_tab_leaves_it_out() {
    with_cx(|cx| {
        for edition in ["dnd2014", "dnd2024"] {
            let mut f = kind("condition")
                .unwrap()
                .new_form("e")
                .with_field("name", json!("Ember Ward"))
                .with_field("edition", json!(edition));
            f.memo.id = Some("ID_X_CONDITION_EMBER_WARD".into());
            let off = one(&f.with_field("listed", json!(false)), cx);
            assert!(!off.supports.iter().any(|t| t == "Condition"), "{edition}");
            assert_eq!(off.data.get("listed"), Some(&json!(false)), "{edition}");
            assert_eq!(off.data.get("tag"), None, "{edition}");
            let on = one(&f, cx);
            assert!(on.supports.iter().any(|t| t == "Condition"), "{edition}");
            assert_eq!(on.data.get("listed"), None, "{edition}");
            // read back: off, kept off when unchanged, listed again without the mark
            let read = element_to_form(&off, cx);
            assert_eq!(read.fields["listed"], false, "{edition}");
            assert_eq!(one(&read, cx), off, "{edition}");
            let again = one(&read.with_field("listed", json!(true)), cx);
            assert_eq!(again.data.get("listed"), None, "{edition}");
            assert!(again.supports.iter().any(|t| t == "Condition"), "{edition}");
        }
    });
}

fn glossary_entry() -> Element {
    el(json!({
        "id": "ID_2024_RULE_DASH", "name": "Dash", "type": "Rules Glossary Entry", "source": "X",
        "ruleset": "dnd2024", "supports": ["Rules Glossary", "Action"], "description": "<p>Run.</p>",
        "data": {"tag": "Action"}
    }))
}

#[test]
fn a_rules_text_group_is_its_mirror_and_its_tag() {
    with_cx(|cx| {
        let f = element_to_form(&glossary_entry(), cx);
        assert_eq!(f.kind, "rulesText");
        assert_eq!(f.fields["glossaryGroup"], "Action");
        assert_eq!(f.fields["tags"], json!(["Rules Glossary"]));
        assert_eq!(one(&f, cx), glossary_entry());
        let e = one(&f.with_field("glossaryGroup", json!("Hazard")), cx);
        assert_eq!(e.supports, ["Rules Glossary", "Hazard"]);
        assert_eq!(e.data["tag"], "Hazard");
        let e = one(&f.with_field("glossaryGroup", json!("")), cx);
        assert_eq!(e.supports, ["Rules Glossary"]);
        assert_eq!(
            e.data["tag"],
            Value::Null,
            "the 2024 shape keeps a null group"
        );
        // a stored null group reads as none and stays null
        let none = el(
            json!({"id": "ID_2024_RULE_D20", "name": "D20 Test", "type": "Rules Glossary Entry",
                             "source": "X", "supports": ["Rules Glossary"], "description": "<p>Roll.</p>",
                             "data": {"tag": null}}),
        );
        let f = element_to_form(&none, cx);
        assert_eq!(f.fields["glossaryGroup"], "");
        assert_eq!(one(&f, cx), none);
    });
}

#[test]
fn the_generic_form_edits_setter_rows_with_attributes() {
    with_cx(|cx| {
        let wolf = el(json!({
            "id": "ID_X_COMPANION_WOLF", "name": "Wolf", "type": "Companion", "source": "X",
            "setters": [{"name": "short", "value": "A wolf."}, {"name": "speed", "value": "40"},
                        {"name": "bite", "value": "1d6", "attributes": {"type": "piercing"}}],
            "description": "A wolf."
        }));
        let f = element_to_form(&wolf, cx);
        assert_eq!(f.kind, "other");
        assert_eq!(f.fields["type"], "Companion");
        assert_eq!(f.fields["summary"], "A wolf.");
        assert_eq!(
            f.fields["setters"],
            json!([{"name": "speed", "value": "40", "attributes": {}},
                   {"name": "bite", "value": "1d6", "attributes": {"type": "piercing"}}])
        );
        assert_eq!(one(&f, cx), wolf);
        let rows = json!([{"name": " speed ", "value": "50", "attributes": {}},
                          {"name": "bite", "value": "1d8", "attributes": {"type": "piercing", "reach": "5"}},
                          {"name": "", "value": "dropped", "attributes": {}}]);
        let e = one(&f.with_field("setters", rows), cx);
        assert_eq!(
            serde_json::to_value(&e.setters).unwrap(),
            json!([{"name": "short", "value": "A wolf."}, {"name": "speed", "value": "50"},
                   {"name": "bite", "value": "1d8", "attributes": {"reach": "5", "type": "piercing"}}])
        );
    });
}
