//! The spell form (spec 4.1), field by field: each row read from every location that holds it, an edit rewrites the
//! setter and (only when the element has it) the 2024 mirror, the spell lists are the support tags, and a new spell
//! starts with values that make a spell. The bundled spells are covered by `tests/form_identity.rs`.

use crate::expr_build::condition::ConditionContext;
use crate::form::{
    element_to_form, form_path_for_location, form_to_element, kind, kind_for_type,
    CatalogBlockCodec, FormContext, FormState, NoLookup,
};
use borealis_content::Element;
use serde_json::{json, Value};

fn with_cx<T>(f: impl FnOnce(&FormContext<'_>) -> T) -> T {
    let codec = CatalogBlockCodec;
    let cx = FormContext {
        conditions: ConditionContext::new(["Wizard"]),
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

/// A 2024 spell as the SRD writes it: setters, the `data` mirrors, a tag that is not a class.
fn spell_2024() -> Element {
    el(json!({
        "id": "ID_X_SPELL_GLOOM", "name": "Gloom", "type": "Spell", "source": "X", "ruleset": "dnd2024",
        "supports": ["Wizard", "Spell Saving Throw", "Cleric"],
        "description": "<p>Dark.</p>",
        "setters": [
            {"name": "level", "value": "2"},
            {"name": "school", "value": "Necromancy", "attributes": {"addition": "ritual"}},
            {"name": "time", "value": "Reaction, which you take when you are hit"},
            {"name": "duration", "value": "Concentration, up to 1 minute"},
            {"name": "range", "value": "60 feet"},
            {"name": "hasVerbalComponent", "value": "true"},
            {"name": "hasSomaticComponent", "value": "false"},
            {"name": "hasMaterialComponent", "value": "true"},
            {"name": "isConcentration", "value": "true"},
            {"name": "isRitual", "value": "false"},
            {"name": "materialComponent", "value": "a black pearl worth 500+ GP, which the spell consumes"}
        ],
        "data": {
            "level": 2, "school": "Necromancy", "classes": ["Wizard", "Spell Saving Throw", "Cleric"],
            "components": {"v": true, "s": false, "m": true, "x": 1},
            "ritual": false, "concentration": true,
            "material_cost": {"amount": 500, "currency": "gp", "consumed": true},
            "reaction_trigger": "when you are hit",
            "higher_level": "Using a Higher-Level Spell Slot. More gloom."
        }
    }))
}

/// A 2014 spell: setters only (as the SRD 5.1 pack and Aurora content write them).
fn spell_2014() -> Element {
    el(json!({
        "id": "ID_X_SPELL_SPARK", "name": "Spark", "type": "Spell", "source": "X",
        "supports": ["Sorcerer"],
        "setters": [
            {"name": "keywords", "value": "fire"},
            {"name": "level", "value": "0"},
            {"name": "school", "value": "Evocation"},
            {"name": "time", "value": "1 action"},
            {"name": "duration", "value": "Instantaneous"},
            {"name": "range", "value": "120 feet"},
            {"name": "hasVerbalComponent", "value": "true"},
            {"name": "hasSomaticComponent", "value": "true"},
            {"name": "hasMaterialComponent", "value": "false"},
            {"name": "isConcentration", "value": "false"},
            {"name": "isRitual", "value": "false"}
        ]
    }))
}

#[test]
fn spells_open_in_the_spell_form_without_the_tags_field() {
    let k = kind_for_type("Spell");
    assert_eq!(k.id, "spell");
    assert!(k.field("tags").is_none(), "a spell's tags are its lists");
    for key in ["level", "school", "time", "duration", "range"] {
        assert!(
            k.field(key).unwrap().required,
            "{key} (validate.rs required_setters)"
        );
    }
    let d = k.descriptor();
    let binds = |key: &str| {
        d.fields
            .iter()
            .find(|f| f.key == key)
            .unwrap()
            .bindings
            .clone()
    };
    assert_eq!(binds("level"), ["setter:level", "data.level"]);
    assert_eq!(binds("school"), ["setter:school", "data.school"]);
    assert_eq!(
        binds("somatic"),
        ["setter:hasSomaticComponent", "data.components.s"]
    );
    assert_eq!(binds("spellLists"), ["supports", "data.classes"]);
    assert_eq!(binds("materialCost"), ["data.material_cost.amount"]);
    assert_eq!(binds("reactionTrigger"), ["data.reaction_trigger"]);
    let lists = d.fields.iter().find(|f| f.key == "spellLists").unwrap();
    assert_eq!(lists.control, "spellLists");
    assert_eq!(lists.section.as_deref(), Some("Spell lists"));
}

#[test]
fn every_row_reads_from_its_locations() {
    with_cx(|cx| {
        let f = element_to_form(&spell_2024(), cx);
        assert_eq!(f.kind, "spell");
        let v = |k: &str| f.fields[k].clone();
        assert_eq!(v("level"), json!("2"));
        assert_eq!(v("school"), json!("Necromancy"));
        assert_eq!(
            v("time"),
            json!("Reaction, which you take when you are hit")
        );
        assert_eq!(v("reactionTrigger"), json!("when you are hit"));
        assert_eq!(v("range"), json!("60 feet"));
        assert_eq!(v("duration"), json!("Concentration, up to 1 minute"));
        assert_eq!(
            [v("verbal"), v("somatic"), v("material")],
            [json!(true), json!(false), json!(true)]
        );
        assert_eq!(v("concentration"), json!(true));
        assert_eq!(v("ritual"), json!(false));
        assert_eq!(v("materialCost"), json!("500"));
        assert_eq!(v("materialConsumed"), json!(true));
        assert_eq!(
            v("spellLists"),
            json!(["Wizard", "Spell Saving Throw", "Cleric"])
        );
        // only the mirror no field edits is kept
        let kept: Vec<&str> = f.preserved.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(kept, ["data.higher_level"]);
        assert_eq!(one(&f, cx), spell_2024());
        // mirrors alone (no setters) are read too
        let mut bare = spell_2024();
        bare.setters.clear();
        let g = element_to_form(&bare, cx);
        assert_eq!(g.fields["level"], json!("2"));
        assert_eq!(g.fields["somatic"], json!(false));
        assert_eq!(g.fields["material"], json!(true));
        assert_eq!(one(&g, cx), bare);
    });
}

#[test]
fn an_edit_rewrites_the_setter_and_the_mirror_in_place() {
    with_cx(|cx| {
        let f = element_to_form(&spell_2024(), cx)
            .with_field("level", json!("3"))
            .with_field("school", json!("Evocation"))
            .with_field("somatic", json!(true))
            .with_field("ritual", json!(true))
            .with_field("spellLists", json!(["Wizard", "Druid"]));
        let out = one(&f, cx);
        let setter = |n: &str| out.setter_value(n).map(str::to_string);
        assert_eq!(setter("level").as_deref(), Some("3"));
        assert_eq!(out.setters[0].name, "level", "same position");
        assert_eq!(setter("school").as_deref(), Some("Evocation"));
        assert_eq!(
            out.setters[1]
                .attributes
                .get("addition")
                .map(String::as_str),
            Some("ritual"),
            "the stored attributes stay"
        );
        assert_eq!(setter("hasSomaticComponent").as_deref(), Some("true"));
        assert_eq!(setter("isRitual").as_deref(), Some("true"));
        assert_eq!(out.data["level"], json!(3), "the mirror keeps its type");
        assert_eq!(out.data["school"], json!("Evocation"));
        assert_eq!(
            out.data["components"],
            json!({"v": true, "s": true, "m": true, "x": 1})
        );
        assert_eq!(out.data["ritual"], json!(true));
        assert_eq!(out.supports, ["Wizard", "Druid"]);
        assert_eq!(out.data["classes"], json!(["Wizard", "Druid"]));
        assert_eq!(out.data["higher_level"], spell_2024().data["higher_level"]);
        // the cost: the amount edited, the currency kept; cleared, the price goes and "used up" stays
        let cost = one(
            &element_to_form(&spell_2024(), cx).with_field("materialCost", json!("1,500")),
            cx,
        );
        assert_eq!(
            cost.data["material_cost"],
            json!({"amount": 1500, "currency": "gp", "consumed": true})
        );
        let none = one(
            &element_to_form(&spell_2024(), cx).with_field("materialCost", json!("")),
            cx,
        );
        assert_eq!(none.data["material_cost"], json!({"consumed": true}));
    });
}

#[test]
fn a_2014_spell_gets_no_mirror() {
    with_cx(|cx| {
        let f = element_to_form(&spell_2014(), cx);
        assert_eq!(f.fields["keywords"], json!(["fire"]));
        assert_eq!(f.fields["level"], json!("0"));
        assert_eq!(f.fields["spellLists"], json!(["Sorcerer"]));
        assert_eq!(one(&f, cx), spell_2014());
        let out = one(
            &f.with_field("level", json!("1"))
                .with_field("material", json!(true))
                .with_field("spellLists", json!(["Sorcerer", "Wizard", "wizard", " "])),
            cx,
        );
        assert!(out.data.is_empty(), "{:?}", out.data);
        assert_eq!(out.setter_value("level"), Some("1"));
        assert_eq!(out.setter_value("hasMaterialComponent"), Some("true"));
        assert_eq!(out.supports, ["Sorcerer", "Wizard"], "trimmed, one of each");
        // the setter order is the stored one
        let names: Vec<&str> = out.setters.iter().map(|s| s.name.as_str()).collect();
        let stored = spell_2014();
        let want: Vec<&str> = stored.setters.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, want);
    });
}

#[test]
fn bad_values_are_told_at_their_field() {
    with_cx(|cx| {
        let f = element_to_form(&spell_2014(), cx);
        for (key, value, message) in [
            (
                "level",
                json!("10"),
                "Choose the spell's level from the list.",
            ),
            (
                "level",
                json!("x"),
                "Choose the spell's level from the list.",
            ),
            (
                "materialCost",
                json!("lots"),
                "Write the cost as a number of gold pieces, like 50.",
            ),
        ] {
            let problems = form_to_element(&f.with_field(key, value), cx).unwrap_err();
            assert_eq!(problems[0].path, format!("fields.{key}"));
            assert_eq!(problems[0].message, message);
        }
    });
}

#[test]
fn a_new_spell_starts_as_a_spell_and_writes_what_the_engine_reads() {
    with_cx(|cx| {
        let k = kind("spell").unwrap();
        let mut f = k.new_form("e");
        assert_eq!(f.fields["level"], json!("1"));
        assert_eq!(
            f.fields.get("school"),
            None,
            "the school is the author's choice"
        );
        f.memo.id = Some("ID_EI_SPELL_EMBER".into());
        let f = f
            .with_field("name", json!("Ember"))
            .with_field("school", json!("Evocation"))
            .with_field("materialText", json!("a coal"))
            .with_field("spellLists", json!(["Wizard"]));
        let out = one(&f, cx);
        assert_eq!(out.ty, "Spell");
        assert_eq!(out.supports, ["Wizard"]);
        let setters: Vec<(&str, &str)> = out
            .setters
            .iter()
            .map(|s| (s.name.as_str(), s.value.as_str()))
            .collect();
        assert_eq!(
            setters,
            [
                ("level", "1"),
                ("school", "Evocation"),
                ("time", "Action"),
                ("range", "Self"),
                ("duration", "Instantaneous"),
                ("isConcentration", "false"),
                ("isRitual", "false"),
                ("hasVerbalComponent", "true"),
                ("hasSomaticComponent", "true"),
                ("hasMaterialComponent", "false"),
                ("materialComponent", "a coal"),
            ]
        );
        assert!(out.data.is_empty());
        // read back: the same values
        let again = element_to_form(&out, cx);
        for key in [
            "level",
            "school",
            "time",
            "spellLists",
            "verbal",
            "material",
        ] {
            assert_eq!(again.fields[key], f.fields[key], "{key}");
        }
    });
}

#[test]
fn validator_locations_point_at_the_spell_fields() {
    with_cx(|cx| {
        let e = spell_2024();
        let f = element_to_form(&e, cx);
        let at = |loc: &str| form_path_for_location(&f, &e, loc, cx);
        assert_eq!(at("setter:school").as_deref(), Some("fields.school"));
        assert_eq!(
            at("setter:hasSomaticComponent").as_deref(),
            Some("fields.somatic")
        );
        assert_eq!(at("supports").as_deref(), Some("fields.spellLists"));
        assert_eq!(at("data.components.s").as_deref(), Some("fields.verbal"));
        assert_eq!(
            at("data.material_cost").as_deref(),
            Some("fields.materialCost")
        );
        assert_eq!(
            at("data.higher_level").as_deref(),
            Some("preserved.data.higher_level")
        );
    });
}
