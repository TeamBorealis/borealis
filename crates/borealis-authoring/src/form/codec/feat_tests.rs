//! The feat form (spec 4.2), field by field, in both shapes: the 2024 category with its tags and mirror, one condition
//! for `requirements`, `data.prerequisite` and the display sentence, taking it again (`allow duplicate` or
//! `data.repeatable`), and the Feat Feature form. The bundled feats are covered by `tests/form_identity.rs`.

use crate::expr_build::condition::{Condition, ConditionContext, ConditionRow};
use crate::expr_build::{Ability, Built, ExprValue, GroupMode, Node};
use crate::form::{
    check_fields, element_to_form, form_path_for_location, form_to_element, kind, kind_for_type,
    CatalogBlockCodec, FormContext, FormState, RefInfo,
};
use borealis_content::Element;
use serde_json::{json, Value};
use std::collections::HashMap;

fn with_cx<T>(f: impl FnOnce(&FormContext<'_>) -> T) -> T {
    let codec = CatalogBlockCodec;
    let lookup: HashMap<String, RefInfo> = [(
        "ID_X_FEAT_TOUGH".to_string(),
        RefInfo {
            name: "Tough".into(),
            ty: "Feat".into(),
        },
    )]
    .into();
    let cx = FormContext {
        conditions: ConditionContext::new(["Wizard"]),
        lookup: &lookup,
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

/// A 2024 feat as the SRD writes it.
fn grappler_2024() -> Element {
    el(json!({
        "id": "ID_X_FEAT_GRAPPLER", "name": "Grappler", "type": "Feat", "source": "X", "ruleset": "dnd2024",
        "description": "<p>Grab.</p>",
        "setters": [{"name": "category", "value": "General"}],
        "rules": [{"kind": "ability_increase", "options": [{"abilities": ["strength", "dexterity"], "pick": 1, "amount": 1}]}],
        "data": {"category": "General",
                 "prerequisite": {"level_min": 4, "abilities_any": [{"strength": 13}, {"dexterity": 13}]},
                 "repeatable": {"allowed": false}},
        "prerequisite": "Level 4+, Strength or Dexterity 13+",
        "requirements": "[character:4]&&([str:13]||[dex:13])",
        "supports": ["General Feat", "Feat", "Grapple"]
    }))
}

/// A 2014 feat as Aurora writes it.
fn feat_2014() -> Element {
    el(json!({
        "id": "ID_X_FEAT_SPELL_SNIPER", "name": "Spell Sniper", "type": "Feat", "source": "X",
        "prerequisite": "The ability to cast at least one spell",
        "requirements": "[spellcasting:1]",
        "setters": [{"name": "allow duplicate", "value": "True"}],
        "supports": ["Combat"]
    }))
}

fn cond(v: &Value) -> Condition {
    let e: ExprValue<Condition> = serde_json::from_value(v.clone()).expect("condition");
    match e.value {
        Built::Model { model } => model,
        Built::Kept { text, .. } => panic!("kept {text}"),
    }
}

fn new_cond(model: Condition) -> Value {
    serde_json::to_value(ExprValue::new(model)).expect("json")
}

fn row(r: ConditionRow) -> Condition {
    Node::row(r)
}

#[test]
fn feats_open_in_the_feat_form_and_feat_features_in_theirs() {
    assert_eq!(kind_for_type("Feat").id, "feat");
    let ff = kind_for_type("Feat Feature");
    assert_eq!(ff.id, "featFeature");
    assert_eq!(ff.parent_kinds, ["feat"]);
    assert_eq!(ff.group, crate::form::KindGroup::Child);
    let k = kind("feat").expect("feat");
    for key in [
        "category",
        "requirements",
        "prerequisiteText",
        "repeatable",
        "repeatDistinct",
        "tags",
        "rules",
    ] {
        assert!(k.field(key).is_some(), "{key}");
    }
    assert!(
        k.field("allowDuplicate").is_none(),
        "Repeatable binds the setter"
    );
    let d = k.descriptor();
    let category = d.fields.iter().find(|f| f.key == "category").unwrap();
    assert_eq!(
        category.bindings,
        [
            "setter:category",
            "supports:<Category> Feat|Feat",
            "data.category"
        ]
    );
    assert_eq!(category.allow_custom, Some(true));
}

#[test]
fn every_row_reads_from_its_locations() {
    with_cx(|cx| {
        let f = element_to_form(&grappler_2024(), cx);
        assert_eq!(f.kind, "feat");
        assert_eq!(f.fields["category"], json!("General"));
        assert_eq!(
            f.fields["tags"],
            json!(["Grapple"]),
            "the category's tags are its own"
        );
        assert_eq!(
            f.fields["prerequisiteText"],
            json!("Level 4+, Strength or Dexterity 13+")
        );
        assert_eq!(f.fields["repeatable"], json!(false));
        assert_eq!(f.fields["repeatDistinct"], json!(""));
        let c = cond(&f.fields["requirements"]);
        assert_eq!(c.rows().len(), 3);
        assert!(f.preserved.is_empty(), "{:?}", f.preserved);
        assert_eq!(one(&f, cx), grappler_2024());

        let old = element_to_form(&feat_2014(), cx);
        assert_eq!(old.fields["category"], json!(""));
        assert_eq!(old.fields["repeatable"], json!(true));
        assert_eq!(old.fields["tags"], json!(["Combat"]));
        assert_eq!(one(&old, cx), feat_2014());

        // a category read from the tag alone (an imported feat), and a mirror with a code
        let imported = el(
            json!({"id": "ID_X_FEAT_I", "name": "I", "type": "Feat", "source": "X",
            "supports": ["Dragonmark Feat", "General Feat"], "data": {"category": "D"}}),
        );
        let f = element_to_form(&imported, cx);
        assert_eq!(f.fields["category"], json!("Dragonmark"));
        assert_eq!(f.fields["tags"], json!(["General Feat"]));
        assert_eq!(one(&f, cx), imported);
        // a 2014 feat's own "... Feat" tag is a plain tag
        let racial = el(
            json!({"id": "ID_X_FEAT_R", "name": "R", "type": "Feat", "source": "X",
            "supports": ["Racial Feat"]}),
        );
        let f = element_to_form(&racial, cx);
        assert_eq!(f.fields["category"], json!(""));
        assert_eq!(f.fields["tags"], json!(["Racial Feat"]));
    });
}

#[test]
fn structured_terms_the_requirements_lack_join_the_condition() {
    with_cx(|cx| {
        // the SRD Archery and Boon of Spell Recall: a feature only in data, or in data besides the level
        let archery = el(
            json!({"id": "ID_X_FEAT_ARCHERY", "name": "Archery", "type": "Feat", "source": "X",
            "prerequisite": "Fighting Style Feature",
            "data": {"prerequisite": {"requires_feature": "Fighting Style"}}}),
        );
        let f = element_to_form(&archery, cx);
        assert_eq!(
            cond(&f.fields["requirements"]).rows()[0].1,
            &ConditionRow::Feature {
                name: "Fighting Style".into()
            }
        );
        assert_eq!(one(&f, cx), archery, "unchanged: stored bytes");
        let recall = el(
            json!({"id": "ID_X_FEAT_RECALL", "name": "Recall", "type": "Feat", "source": "X",
            "prerequisite": "Level 19+, Spellcasting Feature", "requirements": "[character:19]",
            "data": {"prerequisite": {"level_min": 19, "requires_feature": "Spellcasting"}}}),
        );
        let f = element_to_form(&recall, cx);
        let rows: Vec<ConditionRow> = cond(&f.fields["requirements"])
            .rows()
            .into_iter()
            .map(|(_, r)| r.clone())
            .collect();
        assert_eq!(
            rows,
            [
                ConditionRow::CharacterLevel { min: 19 },
                ConditionRow::Spellcasting { level: 1 }
            ]
        );
        assert_eq!(one(&f, cx), recall);
        // edited, both locations say the whole condition
        let edited = f.with_field(
            "requirements",
            new_cond(Node::group(
                GroupMode::All,
                vec![
                    row(ConditionRow::CharacterLevel { min: 12 }),
                    row(ConditionRow::Spellcasting { level: 1 }),
                ],
            )),
        );
        let e = one(&edited, cx);
        assert_eq!(e.requirements, "[character:12]&&[spellcasting:1]");
        assert_eq!(
            e.data["prerequisite"],
            json!({"level_min": 12, "requires_feature": "Spellcasting"})
        );
        assert_eq!(
            e.prerequisite, "Level 12+, Spellcasting Feature",
            "the generated sentence follows the condition"
        );
    });
}

#[test]
fn an_edit_rewrites_every_location_in_place() {
    with_cx(|cx| {
        let f = element_to_form(&grappler_2024(), cx);
        let model = Node::group(
            GroupMode::All,
            vec![
                row(ConditionRow::CharacterLevel { min: 8 }),
                row(ConditionRow::Ability {
                    ability: Ability::Constitution,
                    min: 15,
                }),
                row(ConditionRow::Has {
                    id: "ID_X_FEAT_TOUGH".into(),
                }),
            ],
        );
        let e = one(
            &f.with_field("category", json!(" Epic  Boon "))
                .with_field("requirements", new_cond(model))
                .with_field("repeatable", json!(true))
                .with_field("repeatDistinct", json!("Spell List")),
            cx,
        );
        assert_eq!(e.setter_value("category"), Some("Epic Boon"));
        assert_eq!(e.supports, ["Epic Boon Feat", "Feat", "Grapple"]);
        assert_eq!(e.data["category"], json!("Epic Boon"));
        assert_eq!(e.requirements, "[character:8]&&[con:15]&&ID_X_FEAT_TOUGH");
        assert_eq!(
            e.data["prerequisite"],
            json!({"level_min": 8, "abilities_any": [{"constitution": 15}]}),
            "the mirror holds what every character must meet"
        );
        assert_eq!(e.prerequisite, "Level 8+, Constitution 15+, Tough");
        assert_eq!(
            e.data["repeatable"],
            json!({"allowed": true, "distinct_param": "spell_list"})
        );
        assert!(e.setter("allow duplicate").is_none());
        let back = element_to_form(&e, cx);
        assert_eq!(back.fields["repeatDistinct"], json!("Spell List"));

        // a reworded sentence stays when the condition changes; a cleared one is written again
        let reworded = f.with_field("prerequisiteText", json!("Level 4 and a strong grip"));
        let e = one(
            &reworded.with_field(
                "requirements",
                new_cond(Node::group(
                    GroupMode::All,
                    vec![row(ConditionRow::CharacterLevel { min: 6 })],
                )),
            ),
            cx,
        );
        assert_eq!(e.prerequisite, "Level 4 and a strong grip");
        let e = one(&f.with_field("prerequisiteText", json!("")), cx);
        assert_eq!(e.prerequisite, "Level 4+, Strength or Dexterity 13+");

        // no category: no category setter, tag or mirror
        let e = one(&f.with_field("category", json!("")), cx);
        assert!(e.setter("category").is_none());
        assert_eq!(e.supports, ["Grapple"]);
        assert!(!e.data.contains_key("category"));
    });
}

#[test]
fn a_2014_feat_keeps_its_shape() {
    with_cx(|cx| {
        let f = element_to_form(&feat_2014(), cx);
        let off = one(&f.with_field("repeatable", json!(false)), cx);
        assert!(off.setter("allow duplicate").is_none());
        assert!(off.data.is_empty(), "no 2024 data");
        let on = one(
            &element_to_form(&off, cx).with_field("repeatable", json!(true)),
            cx,
        );
        assert_eq!(
            on.data["repeatable"],
            json!({"allowed": true}),
            "a new take-again is engine data"
        );
        let e = one(
            &f.with_field(
                "requirements",
                new_cond(Node::group(
                    GroupMode::All,
                    vec![row(ConditionRow::Ability {
                        ability: Ability::Intelligence,
                        min: 13,
                    })],
                )),
            ),
            cx,
        );
        assert_eq!(e.requirements, "[int:13]");
        assert!(
            !e.data.contains_key("prerequisite"),
            "no mirror for a feat without one"
        );
        assert_eq!(
            e.prerequisite, "The ability to cast at least one spell",
            "a reworded sentence stays"
        );
        assert_eq!(e.setter_value("allow duplicate"), Some("True"));
    });
}

#[test]
fn a_new_feat_writes_the_2024_shape_from_its_fields() {
    with_cx(|cx| {
        let k = kind("feat").unwrap();
        let mut f = k.new_form("e");
        assert_eq!(f.fields["repeatable"], json!(false));
        f.memo.id = Some("ID_EI_FEAT_EMBER".into());
        let model = Node::group(
            GroupMode::All,
            vec![
                row(ConditionRow::CharacterLevel { min: 4 }),
                Node::group(
                    GroupMode::Any,
                    vec![
                        row(ConditionRow::Ability {
                            ability: Ability::Strength,
                            min: 13,
                        }),
                        row(ConditionRow::Ability {
                            ability: Ability::Charisma,
                            min: 13,
                        }),
                    ],
                ),
            ],
        );
        let f = f
            .with_field("name", json!("Ember Heart"))
            .with_field("category", json!("General"))
            .with_field("requirements", new_cond(model));
        let e = one(&f, cx);
        assert_eq!(e.ty, "Feat");
        assert_eq!(e.supports, ["General Feat", "Feat"]);
        assert_eq!(e.setter_value("category"), Some("General"));
        assert_eq!(e.requirements, "[character:4]&&([str:13]||[cha:13])");
        assert_eq!(e.prerequisite, "Level 4+, Strength or Charisma 13+");
        assert!(e.data.is_empty(), "mirrors only when the element has them");
    });
}

#[test]
fn bad_values_are_told_at_their_field() {
    with_cx(|cx| {
        let f = element_to_form(&grappler_2024(), cx);
        let bad = f.with_field("category", json!("Big|Small"));
        let p = form_to_element(&bad, cx).unwrap_err();
        assert_eq!(p[0].path, "fields.category");
        assert!(p[0].message.starts_with("A category can't contain"));
        let lone = f.with_field("repeatDistinct", json!("Spell List"));
        let p = form_to_element(&lone, cx).unwrap_err();
        assert_eq!(p[0].path, "fields.repeatDistinct");
        assert_eq!(
            p[0].message,
            "This only applies to a feat that can be taken more than once."
        );
        // condition builder problems point at the feat's prerequisite
        let empty = Node::group(
            GroupMode::All,
            vec![row(ConditionRow::Feature {
                name: String::new(),
            })],
        );
        let problems = check_fields(&f.with_field("requirements", new_cond(empty)), cx);
        assert!(
            problems
                .iter()
                .any(|p| p.path.starts_with("fields.requirements")),
            "{problems:?}"
        );
    });
}

#[test]
fn validator_locations_point_at_the_feat_fields() {
    with_cx(|cx| {
        let e = grappler_2024();
        let f = element_to_form(&e, cx);
        let at = |loc: &str| form_path_for_location(&f, &e, loc, cx);
        assert_eq!(at("requirements").as_deref(), Some("fields.requirements"));
        assert_eq!(
            at("prerequisite").as_deref(),
            Some("fields.prerequisiteText")
        );
        assert_eq!(
            at("data.prerequisite.level_min").as_deref(),
            Some("fields.requirements")
        );
        assert_eq!(at("data.repeatable").as_deref(), Some("fields.repeatable"));
        assert_eq!(at("setter:category").as_deref(), Some("fields.category"));
        assert_eq!(at("supports").as_deref(), Some("fields.tags"));
    });
}

#[test]
fn a_feat_feature_round_trips_with_the_shared_parts() {
    with_cx(|cx| {
        let opt = el(
            json!({"id": "ID_X_FEAT_FEATURE_ELEMENTAL_FIRE", "name": "Fire", "type": "Feat Feature",
            "source": "X", "supports": ["Elemental Adept Option"], "description": "<p>Fire.</p>",
            "setters": [{"name": "allow duplicate", "value": "true"}],
            "rules": [{"kind": "grant", "type": "Damage Type", "id": "ID_DAMAGE_FIRE"}]}),
        );
        let f = element_to_form(&opt, cx);
        assert_eq!(f.kind, "featFeature");
        assert_eq!(f.fields["tags"], json!(["Elemental Adept Option"]));
        assert_eq!(f.fields["allowDuplicate"], json!(true));
        assert_eq!(one(&f, cx), opt);
        let e = one(&f.with_field("name", json!("Cold ")), cx);
        assert_eq!((e.name.as_str(), e.ty.as_str()), ("Cold", "Feat Feature"));
    });
}
