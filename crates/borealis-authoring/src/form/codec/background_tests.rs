//! The background form (spec 4.3) in the 2024 and the 2014 shapes: every field read from its rules and mirrors and
//! written back unchanged, edits rewriting rules and mirrors in place, new backgrounds in the SRD shapes, the
//! validation sentences, the children (feature, variants, package) and the characteristics' numbering. The bundled
//! backgrounds are covered by `tests/form_identity.rs`.

use crate::expr_build::condition::ConditionContext;
use crate::form::{
    add_child, assign_ids, descriptors, element_group_to_form, element_to_form, form_to_element,
    form_to_elements, kind, kind_for_type, new_background_form, CatalogBlockCodec, FormContext,
    FormState, Lookup, RefInfo, WithElements,
};
use crate::ids::IdGenerator;
use crate::pack::granted_children;
use borealis_content::{Element, Ruleset};
use serde_json::{json, Value};
use std::collections::HashMap;

fn el(v: Value) -> Element {
    serde_json::from_value(v).expect("element")
}

fn names() -> HashMap<String, RefInfo> {
    [
        ("ID_PROFICIENCY_SKILL_INSIGHT", "Insight", "Proficiency"),
        ("ID_PROFICIENCY_SKILL_RELIGION", "Religion", "Proficiency"),
        ("ID_PROFICIENCY_SKILL_ARCANA", "Arcana", "Proficiency"),
        (
            "ID_PROFICIENCY_TOOL_PROFICIENCY_CALLIGRAPHERS_SUPPLIES",
            "Tool Proficiency (Calligrapher's supplies)",
            "Proficiency",
        ),
        ("ID_2024_FEAT_MAGIC_INITIATE", "Magic Initiate", "Feat"),
        ("ID_2024_FEAT_ALERT", "Alert", "Feat"),
    ]
    .into_iter()
    .map(|(id, name, ty)| {
        (
            id.to_string(),
            RefInfo {
                name: name.into(),
                ty: ty.into(),
            },
        )
    })
    .collect()
}

fn with_lookup<T>(lookup: &dyn Lookup, f: impl FnOnce(&FormContext<'_>) -> T) -> T {
    let codec = CatalogBlockCodec;
    let cx = FormContext {
        conditions: ConditionContext::new(Vec::<String>::new()),
        lookup,
        blocks: &codec,
    };
    f(&cx)
}

fn with_cx<T>(f: impl FnOnce(&FormContext<'_>) -> T) -> T {
    let lookup = names();
    with_lookup(&lookup, f)
}

/// A 2024 background in the SRD shape, with its package.
fn acolyte() -> (Element, Element) {
    let bg = el(json!({
        "id": "ID_X_BACKGROUND_ACOLYTE", "name": "Acolyte", "type": "Background", "source": "X", "ruleset": "dnd2024",
        "description": "<p>Temple.</p>",
        "rules": [
            {"kind": "ability_distribution", "name": "Ability Scores (Acolyte)", "pool": ["intelligence", "wisdom", "charisma"],
             "options": [[2, 1], [1, 1, 1]], "cap": 20},
            {"kind": "grant_feat", "feat": "ID_2024_FEAT_MAGIC_INITIATE", "params": {"spell_list": "Cleric"}},
            {"kind": "grant", "id": "ID_PROFICIENCY_SKILL_INSIGHT", "type": "Proficiency"},
            {"kind": "grant", "id": "ID_PROFICIENCY_SKILL_RELIGION", "type": "Proficiency"},
            {"kind": "grant", "id": "ID_PROFICIENCY_TOOL_PROFICIENCY_CALLIGRAPHERS_SUPPLIES", "type": "Proficiency"},
            {"kind": "equipment_package", "ref": "ID_X_EQUIPMENT_PACKAGE_ACOLYTE", "name": "Starting Equipment (Acolyte)"}
        ],
        "data": {"ability_scores": ["intelligence", "wisdom", "charisma"], "origin_feat": "ID_2024_FEAT_MAGIC_INITIATE",
                 "origin_feat_params": {"spell_list": "Cleric"}, "skills": ["Insight", "Religion"],
                 "tool": "Calligrapher’s Supplies", "equipment_package": "ID_X_EQUIPMENT_PACKAGE_ACOLYTE"}
    }));
    let package = el(json!({
        "id": "ID_X_EQUIPMENT_PACKAGE_ACOLYTE", "name": "Starting Equipment (Acolyte)", "type": "Equipment Package",
        "source": "X", "ruleset": "dnd2024",
        "data": {"owner": "ID_X_BACKGROUND_ACOLYTE", "options": [{"id": "A", "items": [], "coins": {"gp": 50}}]}
    }));
    (bg, package)
}

/// A 2014 background in the SRD shape, with its feature and a variant.
fn sage_2014() -> (Element, Element, Element) {
    let list = |name: &str, number: Option<u32>, n: usize| {
        let items: Vec<Value> = (1..=n)
            .map(|i| json!({"id": i, "text": format!("{name} {i}.")}))
            .collect();
        let mut v = json!({"kind": "select", "type": "List", "name": name, "items": items});
        if let Some(k) = number {
            v["number"] = json!(k);
        }
        v
    };
    let bg = el(json!({
        "id": "ID_X_BACKGROUND_SAGE", "name": "Sage", "type": "Background", "source": "X",
        "rules": [
            {"kind": "grant", "id": "ID_PROFICIENCY_SKILL_ARCANA", "type": "Proficiency"},
            {"kind": "select", "type": "Language", "name": "Language (Sage)", "number": 2, "supports": "Standard||Exotic"},
            {"kind": "grant", "id": "ID_X_BACKGROUND_FEATURE_RESEARCHER", "type": "Background Feature"},
            list("Personality Trait", Some(2), 3),
            list("Ideal", None, 2),
            list("Bond", None, 2),
            list("Flaw", None, 2),
            {"kind": "select", "type": "Background Variant", "name": "Background Variant (Sage)",
             "supports": "ID_X_BACKGROUND_VARIANT_SCRIBE", "optional": true}
        ],
        "equipment": {"description": "<p>A bottle of ink.</p>"}
    }));
    let feature = el(json!({
        "id": "ID_X_BACKGROUND_FEATURE_RESEARCHER", "name": "Researcher", "type": "Background Feature", "source": "X",
        "supports": ["Background Feature"], "description": "<p>You know where to look.</p>"
    }));
    let variant = el(json!({
        "id": "ID_X_BACKGROUND_VARIANT_SCRIBE", "name": "Scribe", "type": "Background Variant", "source": "X",
        "description": "<p>A scribe.</p>"
    }));
    (bg, feature, variant)
}

/// A new tag filter: any of these tags.
fn any_of(tags: &[&str]) -> Value {
    use crate::expr_build::tag_filter::{TagFilter, TagRow};
    use crate::expr_build::{ExprValue, GroupMode, Node};
    let rows = tags
        .iter()
        .map(|t| {
            Node::row(TagRow::Tag {
                tag: (*t).to_string(),
            })
        })
        .collect();
    serde_json::to_value(ExprValue::new(TagFilter {
        specific: false,
        root: Node::group(GroupMode::Any, rows),
    }))
    .unwrap()
}

fn ids(form: &FormState, taken: &[&str]) -> FormState {
    let mut gen =
        IdGenerator::new("HB", taken.iter().copied(), Vec::<String>::new()).expect("generator");
    assign_ids(form, &mut gen)
}

fn rules(e: &Element) -> Value {
    serde_json::to_value(&e.rules).expect("rules")
}

fn paths(problems: &[crate::form::FormProblem]) -> Vec<(String, String)> {
    problems
        .iter()
        .map(|p| (p.path.clone(), p.message.clone()))
        .collect()
}

#[test]
fn the_background_kinds_and_their_children() {
    assert_eq!(kind_for_type("Background").id, "background");
    for (ty, id) in [
        ("Background Feature", "backgroundFeature"),
        ("Background Variant", "backgroundVariant"),
        ("Background Characteristics", "backgroundCharacteristics"),
    ] {
        let k = kind_for_type(ty);
        assert_eq!(k.id, id);
        let parents: &[&str] = if id == "backgroundFeature" {
            // a variant makes its own feature too (P2a)
            &["background", "backgroundVariant"]
        } else {
            &["background"]
        };
        assert_eq!(k.parent_kinds, parents);
    }
    assert!(kind("equipmentPackage")
        .unwrap()
        .parent_kinds
        .contains(&"background"));
    let d = descriptors()
        .into_iter()
        .find(|d| d.id == "background")
        .unwrap();
    let field = |key: &str| d.fields.iter().find(|f| f.key == key).unwrap();
    assert_eq!(
        field("abilityScores").editions,
        Some(vec![Ruleset::Dnd2024])
    );
    assert_eq!(field("originFeat").control, "featGrant");
    assert_eq!(field("feature").editions, Some(vec![Ruleset::Dnd2014]));
    assert_eq!(field("personalityTraits").control, "writtenList");
    assert_eq!(field("choices").control, "choices");
    assert_eq!(
        field("skills").picker.as_ref().unwrap().tags,
        Some(vec!["Skill".to_string()])
    );
    assert!(field("tools").editions.is_none() && field("equipment").editions.is_none());
    assert!(d.fields.iter().all(|f| f.key != "allowDuplicate"));
}

#[test]
fn a_2024_background_reads_into_its_fields_and_writes_back_unchanged() {
    with_cx(|cx| {
        let (bg, package) = acolyte();
        let form = element_group_to_form(&bg, &[&package], cx);
        let f = &form.fields;
        assert_eq!(
            f["abilityScores"],
            json!(["Intelligence", "Wisdom", "Charisma"])
        );
        assert_eq!(f["originFeat"]["feat"]["name"], json!("Magic Initiate"));
        assert_eq!(f["originFeat"]["params"], json!({"spell_list": "Cleric"}));
        assert_eq!(f["skills"][1]["name"], json!("Religion"));
        assert_eq!(f["tools"].as_array().unwrap().len(), 1);
        assert_eq!(f["equipment"], json!([{"child": "e/0"}]));
        assert!(form.blocks.is_empty() && form.preserved.is_empty());
        let back = form_to_elements(&form, cx).expect("writes");
        assert_eq!(back, [bg, package]);
    });
}

#[test]
fn edits_rewrite_the_rules_and_the_mirrors_in_place() {
    with_cx(|cx| {
        let (bg, _) = acolyte();
        let form = element_to_form(&bg, cx)
            .with_field("abilityScores", json!(["Charisma", "Strength", "Wisdom"]))
            .with_field(
                "originFeat",
                json!({"feat": {"id": "ID_2024_FEAT_MAGIC_INITIATE", "name": "Magic Initiate", "type": "Feat"},
                       "params": {"Spell List": "Wizard"}}),
            )
            .with_field(
                "skills",
                json!([{"id": "ID_PROFICIENCY_SKILL_ARCANA", "name": "Arcana", "type": "Proficiency"},
                       {"id": "ID_PROFICIENCY_SKILL_INSIGHT", "name": "Insight", "type": "Proficiency"}]),
            );
        let e = form_to_element(&form, cx).expect("writes");
        let r = rules(&e);
        assert_eq!(r[0]["pool"], json!(["strength", "wisdom", "charisma"]));
        assert_eq!(r[0]["name"], json!("Ability Scores (Acolyte)"));
        assert_eq!(
            r[1],
            json!({"kind": "grant_feat", "feat": "ID_2024_FEAT_MAGIC_INITIATE", "params": {"spell_list": "Wizard"}})
        );
        assert_eq!(r[2]["id"], json!("ID_PROFICIENCY_SKILL_ARCANA"));
        assert_eq!(
            r[4]["id"],
            json!("ID_PROFICIENCY_TOOL_PROFICIENCY_CALLIGRAPHERS_SUPPLIES")
        );
        assert_eq!(r[5]["kind"], json!("equipment_package"));
        assert_eq!(
            e.data["ability_scores"],
            json!(["strength", "wisdom", "charisma"])
        );
        assert_eq!(
            e.data["origin_feat_params"],
            json!({"spell_list": "Wizard"})
        );
        assert_eq!(e.data["skills"], json!(["Arcana", "Insight"]));
        assert_eq!(
            e.data["tool"],
            json!("Calligrapher’s Supplies"),
            "the unchanged tool keeps its mirror"
        );
    });
}

#[test]
fn a_new_2024_background_writes_the_srd_shape() {
    with_cx(|cx| {
        let form = new_background_form(Ruleset::Dnd2024)
            .with_field("name", json!("Ember Warden"))
            .with_field("abilityScores", json!(["Wisdom", "Strength", "Constitution"]))
            .with_field(
                "originFeat",
                json!({"feat": {"id": "ID_2024_FEAT_ALERT", "name": "Alert", "type": "Feat"}, "params": {}}),
            )
            .with_field(
                "skills",
                json!([{"id": "ID_PROFICIENCY_SKILL_INSIGHT", "name": "Insight", "type": "Proficiency"},
                       {"id": "ID_PROFICIENCY_SKILL_RELIGION", "name": "Religion", "type": "Proficiency"}]),
            )
            .with_field(
                "tools",
                json!([{"id": "ID_PROFICIENCY_TOOL_PROFICIENCY_CALLIGRAPHERS_SUPPLIES",
                        "name": "Tool Proficiency (Calligrapher's supplies)", "type": "Proficiency"}]),
            );
        let package = kind("equipmentPackage").unwrap().new_form("p");
        let form = add_child(&form, package, None).expect("a background has a package");
        let form = ids(&form, &[]);
        let out = form_to_elements(&form, cx).expect("writes");
        let (e, p) = (&out[0], &out[1]);
        assert_eq!(e.id, "ID_HB_BACKGROUND_EMBER_WARDEN");
        assert_eq!(
            rules(e),
            json!([
                {"kind": "ability_distribution", "name": "Ability Scores (Ember Warden)",
                 "pool": ["strength", "constitution", "wisdom"], "options": [[2, 1], [1, 1, 1]], "cap": 20},
                {"kind": "grant_feat", "feat": "ID_2024_FEAT_ALERT", "params": {}},
                {"kind": "grant", "id": "ID_PROFICIENCY_SKILL_INSIGHT", "type": "Proficiency"},
                {"kind": "grant", "id": "ID_PROFICIENCY_SKILL_RELIGION", "type": "Proficiency"},
                {"kind": "grant", "id": "ID_PROFICIENCY_TOOL_PROFICIENCY_CALLIGRAPHERS_SUPPLIES", "type": "Proficiency"},
                {"kind": "equipment_package", "ref": p.id, "name": "Starting Equipment (Ember Warden)"}
            ])
        );
        assert_eq!(
            serde_json::to_value(&e.data).unwrap(),
            json!({"ability_scores": ["strength", "constitution", "wisdom"], "origin_feat": "ID_2024_FEAT_ALERT",
                   "origin_feat_params": {}, "skills": ["Insight", "Religion"], "tool": "Calligrapher's supplies",
                   "equipment_package": p.id})
        );
        assert_eq!(p.data["owner"], json!(e.id));
        assert_eq!(e.ruleset, Some(Ruleset::Dnd2024));
        // no characteristics, no variants: a 2024 background has none
        assert!(e
            .rules
            .iter()
            .all(|r| !matches!(r, borealis_content::Rule::Select(_))));
    });
}

#[test]
fn a_new_2024_background_says_what_it_still_needs_at_its_fields() {
    with_cx(|cx| {
        let form = ids(
            &new_background_form(Ruleset::Dnd2024).with_field("name", json!("Blank")),
            &[],
        );
        let problems = paths(&form_to_elements(&form, cx).unwrap_err());
        assert_eq!(
            problems,
            [
                (
                    "fields.abilityScores".to_string(),
                    "Pick three abilities.".to_string()
                ),
                (
                    "fields.originFeat".to_string(),
                    "Choose the origin feat.".to_string()
                ),
                ("fields.skills".to_string(), "Pick two skills.".to_string()),
            ]
        );
        let two_tools = form.with_field(
            "tools",
            json!([{"id": "ID_A", "name": "A", "type": "Proficiency"}, {"id": "ID_B", "name": "B", "type": "Proficiency"}]),
        );
        let problems = paths(&form_to_elements(&two_tools, cx).unwrap_err());
        assert!(
            problems
                .iter()
                .any(|(p, m)| p == "fields.tools"
                    && m.starts_with("A 2024 background gives one tool"))
        );
        // a 2014 background needs none of the 2024 parts
        let old = ids(
            &new_background_form(Ruleset::Dnd2014).with_field("name", json!("Old")),
            &[],
        );
        assert!(form_to_elements(&old, cx).is_ok());
    });
}

#[test]
fn the_origin_feat_s_choices_come_from_the_feat() {
    let feat = el(json!({
        "id": "ID_2024_FEAT_MAGIC_INITIATE", "name": "Magic Initiate", "type": "Feat", "source": "X",
        "rules": [{"kind": "select_param", "name": "Spell List", "options": ["Cleric", "Druid", "Wizard"]},
                  {"kind": "select_param", "name": "Spellcasting Ability", "options": ["Intelligence", "Wisdom", "Charisma"]}]
    }));
    assert_eq!(
        crate::form::feat_params(&feat),
        [
            (
                "Spell List".to_string(),
                vec!["Cleric".to_string(), "Druid".into(), "Wizard".into()]
            ),
            (
                "Spellcasting Ability".to_string(),
                vec![
                    "Intelligence".to_string(),
                    "Wisdom".into(),
                    "Charisma".into()
                ]
            ),
        ]
    );
    let extra = [feat];
    let lookup = WithElements {
        base: &crate::form::NoLookup,
        extra: &extra,
    };
    with_lookup(&lookup, |cx| {
        let (bg, _) = acolyte();
        let with = |params: Value| {
            let form = element_to_form(&bg, cx).with_field(
                "originFeat",
                json!({"feat": {"id": "ID_2024_FEAT_MAGIC_INITIATE", "name": "Magic Initiate", "type": "Feat"}, "params": params}),
            );
            form_to_element(&form, cx).map_err(|p| paths(&p))
        };
        assert!(with(json!({"spell_list": "Druid", "spellcasting_ability": "Wisdom"})).is_ok());
        assert_eq!(
            with(json!({"spell_list": "Bard"})).unwrap_err(),
            [(
                "fields.originFeat".to_string(),
                "Pick a spell list that Magic Initiate offers: Cleric, Druid or Wizard."
                    .to_string()
            )]
        );
        assert_eq!(
            with(json!({"school": "Evocation"})).unwrap_err()[0].1,
            "Magic Initiate has no choice called “school”. Clear it, or choose another feat."
        );
    });
}

#[test]
fn a_2014_background_reads_with_its_feature_characteristics_and_variant() {
    with_cx(|cx| {
        let (bg, feature, variant) = sage_2014();
        let all = [bg.clone(), feature.clone(), variant.clone()];
        let kids = granted_children(&bg, &all);
        assert_eq!(
            kids.len(),
            2,
            "the feature and the variant are its children"
        );
        let form = element_group_to_form(&bg, &kids, cx);
        let f = &form.fields;
        assert_eq!(f["skills"][0]["name"], json!("Arcana"));
        assert_eq!(f["choices"][0]["kind"], json!("language"));
        assert_eq!(f["choices"][0]["count"], json!(2));
        assert_eq!(f["choices"][0]["name"], json!("Language (Sage)"));
        assert_eq!(f["feature"], json!([{"child": "e/0"}]));
        assert_eq!(f["variants"], json!([{"child": "e/1"}]));
        assert_eq!(f["personalityTraits"]["count"], json!(2));
        assert_eq!(
            f["ideals"],
            json!({"count": 1, "items": ["Ideal 1.", "Ideal 2."]})
        );
        assert_eq!(form.children[1].kind, "backgroundVariant");
        assert!(form.blocks.is_empty() && form.preserved.is_empty());
        assert_eq!(form_to_elements(&form, cx).expect("writes"), all);
    });
}

#[test]
fn characteristics_are_numbered_and_checked() {
    with_cx(|cx| {
        let (bg, _, _) = sage_2014();
        let form = element_to_form(&bg, cx)
            .with_field(
                "ideals",
                json!({"count": 1, "items": [" Knowledge. ", "", "Power."]}),
            )
            .with_field("bonds", json!({"count": 1, "items": []}));
        let e = form_to_element(&form, cx).expect("writes");
        let r = rules(&e);
        assert_eq!(
            r[4],
            json!({"kind": "select", "type": "List", "name": "Ideal",
                   "items": [{"id": 1, "text": "Knowledge."}, {"id": 2, "text": "Power."}]})
        );
        assert!(
            r.as_array().unwrap().iter().all(|x| x["name"] != "Bond"),
            "an empty list writes no choice"
        );
        let short =
            element_to_form(&bg, cx).with_field("flaws", json!({"count": 3, "items": ["One."]}));
        assert_eq!(
            paths(&form_to_element(&short, cx).unwrap_err()),
            [(
                "fields.flaws".to_string(),
                "Write at least 3 flaws: the character picks 3.".to_string()
            )]
        );
    });
}

#[test]
fn a_new_2014_background_writes_the_srd_shape_with_its_children() {
    with_cx(|cx| {
        let form = new_background_form(Ruleset::Dnd2014)
            .with_field("name", json!("Hermit"))
            .with_field(
                "skills",
                json!([{"id": "ID_PROFICIENCY_SKILL_RELIGION", "name": "Religion", "type": "Proficiency"}]),
            )
            .with_field(
                "choices",
                json!([{"kind": "language", "count": 1, "from": any_of(&["Standard", "Exotic"])},
                       {"kind": "proficiency", "count": 1, "from": any_of(&["Gaming Set"])}]),
            )
            .with_field("personalityTraits", json!({"count": 2, "items": ["Quiet.", "Calm.", "Odd."]}))
            .with_field("ideals", json!({"count": 1, "items": ["Truth."]}))
            .with_field("bonds", json!({"count": 1, "items": ["Home."]}))
            .with_field("flaws", json!({"count": 1, "items": ["Pride."]}));
        let mut feature = kind("backgroundFeature").unwrap().new_form("f");
        feature.fields.insert("name".into(), json!("Discovery"));
        let mut variant = kind("backgroundVariant").unwrap().new_form("v");
        variant.fields.insert("name".into(), json!("Recluse"));
        let form = add_child(&form, feature, None).expect("feature");
        let form = add_child(&form, variant, None).expect("variant");
        let form = ids(&form, &[]);
        let out = form_to_elements(&form, cx).expect("writes");
        let r = rules(&out[0]);
        let kinds: Vec<(&str, &str)> = r
            .as_array()
            .unwrap()
            .iter()
            .map(|x| {
                (
                    x["kind"].as_str().unwrap(),
                    x["type"].as_str().unwrap_or_default(),
                )
            })
            .collect();
        assert_eq!(
            kinds,
            [
                ("grant", "Proficiency"),
                ("select", "Language"),
                ("select", "Proficiency"),
                ("grant", "Background Feature"),
                ("select", "List"),
                ("select", "List"),
                ("select", "List"),
                ("select", "List"),
                ("select", "Background Variant"),
            ]
        );
        assert_eq!(
            r[1],
            json!({"kind": "select", "type": "Language", "name": "Language (Hermit)", "supports": "Standard||Exotic"})
        );
        assert_eq!(r[2]["name"], json!("Tool Proficiency (Hermit)"));
        assert_eq!(r[2]["supports"], json!("Gaming Set"));
        assert_eq!(r[3]["id"], json!(out[1].id));
        assert_eq!(r[4]["number"], json!(2));
        assert_eq!(r[4]["items"][2], json!({"id": 3, "text": "Odd."}));
        assert_eq!(
            r[8],
            json!({"kind": "select", "type": "Background Variant", "name": "Background Variant (Hermit)",
                                "supports": out[2].id, "optional": true})
        );
        assert_eq!(
            (out[1].ty.as_str(), out[2].ty.as_str()),
            ("Background Feature", "Background Variant")
        );
        assert_eq!(
            out[1].ruleset,
            Some(Ruleset::Dnd2014),
            "children take the background's edition"
        );
        assert!(
            out[0].data.is_empty(),
            "a 2014 background has no 2024 mirrors"
        );
    });
}

#[test]
fn the_equipment_note_reads_and_writes_the_equipment_part() {
    with_cx(|cx| {
        let (bg, _, _) = sage_2014();
        let form = element_to_form(&bg, cx);
        assert!(form.preserved.iter().all(|p| p.key != "equipment"));
        let doc =
            serde_json::to_value(borealis_content::rich::parse("<p>Ink and a quill.</p>")).unwrap();
        let e = form_to_element(&form.with_field("equipmentNote", doc), cx).expect("writes");
        assert_eq!(
            e.equipment.unwrap().description.as_deref(),
            Some("<p>Ink and a quill.</p>")
        );
        let empty = serde_json::to_value(borealis_content::rich::parse("")).unwrap();
        let e = form_to_element(
            &element_to_form(&bg, cx).with_field("equipmentNote", empty),
            cx,
        )
        .expect("writes");
        assert!(e.equipment.is_none());
    });
}

#[test]
fn a_renamed_background_keeps_its_choices_names() {
    with_cx(|cx| {
        let (bg, _, _) = sage_2014();
        let form = element_to_form(&bg, cx);
        let mut choices = form.fields["choices"].clone();
        choices
            .as_array_mut()
            .unwrap()
            .push(json!({"kind": "language", "count": 1, "from": any_of(&["Exotic"])}));
        let e = form_to_element(
            &form
                .with_field("name", json!("Scholar"))
                .with_field("choices", choices),
            cx,
        )
        .expect("writes");
        let r = rules(&e);
        assert_eq!(
            r[1]["name"],
            json!("Language (Sage)"),
            "the stored choice keeps its key"
        );
        assert_eq!(r[2]["name"], json!("Language (Scholar)"));
        assert_eq!(r[2]["supports"], json!("Exotic"));
        // a choice of nothing says so at its row
        let bad = element_to_form(&bg, cx)
            .with_field("choices", json!([{"kind": "language", "count": 1}]));
        let p = paths(&form_to_element(&bad, cx).unwrap_err());
        assert_eq!(p[0].0, "fields.choices.0");
    });
}

#[test]
fn variants_offered_by_tag_are_read_into_the_variants_field() {
    let variant = |id: &str, name: &str, tags: &[&str]| {
        el(
            json!({"id": id, "name": name, "type": "Background Variant", "source": "X", "supports": tags}),
        )
    };
    let loaded = [
        variant("ID_X_BACKGROUND_VARIANT_SPY", "Spy", &["Variant Criminal"]),
        variant(
            "ID_X_BACKGROUND_VARIANT_SMUGGLER",
            "Smuggler",
            &["Variant Criminal", "Coast"],
        ),
        variant(
            "ID_X_BACKGROUND_VARIANT_KNIGHT",
            "Knight",
            &["Variant Noble"],
        ),
    ];
    let names = names();
    let lookup = WithElements {
        base: &names,
        extra: &loaded,
    };
    with_lookup(&lookup, |cx| {
        let bg = el(json!({
            "id": "ID_X_BACKGROUND_CRIMINAL", "name": "Criminal", "type": "Background", "source": "X",
            "rules": [
                {"kind": "grant", "id": "ID_PROFICIENCY_SKILL_INSIGHT", "type": "Proficiency"},
                {"kind": "select", "type": "Background Variant", "name": "Variant", "supports": "Variant Criminal", "optional": true}
            ]
        }));
        let form = element_to_form(&bg, cx);
        assert_eq!(
            form.fields["variants"],
            json!([
                {"element": {"id": "ID_X_BACKGROUND_VARIANT_SPY", "name": "Spy", "type": "Background Variant"}},
                {"element": {"id": "ID_X_BACKGROUND_VARIANT_SMUGGLER", "name": "Smuggler", "type": "Background Variant"}}
            ]),
            "the loaded variants its tag offers"
        );
        assert!(form.blocks.is_empty(), "the choice is the field's");
        assert_eq!(
            form_to_element(&form, cx).expect("writes"),
            bg,
            "the tag shape is kept"
        );

        // an expression of tags reads the same way
        let mut both = bg.clone();
        both.rules[1] = serde_json::from_value(
            json!({"kind": "select", "type": "Background Variant", "name": "Variant",
            "supports": "Variant Criminal, Coast", "optional": true}),
        )
        .expect("rule");
        let form2 = element_to_form(&both, cx);
        assert_eq!(form2.fields["variants"].as_array().map(Vec::len), Some(1));
        assert_eq!(form_to_element(&form2, cx).expect("writes"), both);

        // a changed list offers the variants it lists by id; an empty one offers none
        let edited = form.with_field(
            "variants",
            json!([{"element": {"id": "ID_X_BACKGROUND_VARIANT_SPY", "name": "Spy", "type": "Background Variant"}},
                   {"element": {"id": "ID_X_BACKGROUND_VARIANT_KNIGHT", "name": "Knight", "type": "Background Variant"}}]),
        );
        let e = form_to_element(&edited, cx).expect("writes");
        assert_eq!(
            rules(&e)[1],
            json!({"kind": "select", "type": "Background Variant", "name": "Variant",
                   "supports": "ID_X_BACKGROUND_VARIANT_SPY|ID_X_BACKGROUND_VARIANT_KNIGHT", "optional": true})
        );
        let none = form_to_element(&form.with_field("variants", json!([])), cx).expect("writes");
        assert_eq!(none.rules.len(), 1);
    });
}
