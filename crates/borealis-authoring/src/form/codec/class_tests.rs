//! The class form (spec 4.5), field by field, in the 2014 and 2024 shapes: the hit die and its mirrors, the "first
//! class only" proficiencies and skill choice, the class table, the LEVELS grid with its children, the multiclass
//! section and its id (a copy's own), the generated features (written from the class's name) and a block gained again
//! at other levels. The bundled classes are covered by `tests/form_identity.rs`.

use crate::expr_build::condition::{ConditionContext, ConditionRow};
use crate::expr_build::{Ability, GroupMode, Node};
use crate::form::{
    add_child, assign_ids, element_group_to_form, element_to_form, form_to_element,
    form_to_elements, kind, new_class_form, CatalogBlockCodec, FormContext, FormState, RefInfo,
    AGAIN_AT,
};
use crate::ids::IdGenerator;
use borealis_content::{Element, Ruleset};
use serde_json::{json, Value};
use std::collections::HashMap;

fn with_cx<T>(f: impl FnOnce(&FormContext<'_>) -> T) -> T {
    let codec = CatalogBlockCodec;
    let lookup: HashMap<String, RefInfo> = [
        (
            "ID_PROFICIENCY_ARMOR_PROFICIENCY_LIGHT_ARMOR",
            "Armor Proficiency (Light Armor)",
        ),
        (
            "ID_PROFICIENCY_SKILL_ATHLETICS",
            "Skill Proficiency (Athletics)",
        ),
        (
            "ID_PROFICIENCY_SKILL_STEALTH",
            "Skill Proficiency (Stealth)",
        ),
    ]
    .into_iter()
    .map(|(id, name)| {
        (
            id.to_string(),
            RefInfo {
                name: name.into(),
                ty: "Proficiency".into(),
            },
        )
    })
    .collect();
    let cx = FormContext {
        conditions: ConditionContext::new(["Knight"]),
        lookup: &lookup,
        blocks: &codec,
    };
    f(&cx)
}

fn el(v: Value) -> Element {
    serde_json::from_value(v).expect("element")
}

/// A 2024 class as the SRD writes it, with one feature and its package.
fn knight() -> (Element, Element, Element) {
    let class = el(json!({
        "id": "ID_X_CLASS_KNIGHT", "name": "Knight", "type": "Class", "source": "X", "ruleset": "dnd2024",
        "description": "<p>Knights.</p>",
        "setters": [{"name": "hd", "value": "d10"}, {"name": "short", "value": "Strength"}],
        "rules": [
            {"kind": "grant", "id": "ID_PROFICIENCY_SAVINGTHROW_STRENGTH", "type": "Proficiency", "requirements": "!ID_X_MULTICLASS_KNIGHT"},
            {"kind": "select", "type": "Proficiency", "name": "Skill Proficiency (Knight)", "number": 1,
             "supports": "ID_PROFICIENCY_SKILL_ATHLETICS|ID_PROFICIENCY_SKILL_STEALTH", "requirements": "!ID_X_MULTICLASS_KNIGHT"},
            {"kind": "grant", "id": "ID_PROFICIENCY_ARMOR_PROFICIENCY_LIGHT_ARMOR", "type": "Proficiency", "requirements": "!ID_X_MULTICLASS_KNIGHT"},
            {"kind": "equipment_package", "requirements": "!ID_X_MULTICLASS_KNIGHT", "ref": "ID_X_EQUIPMENT_PACKAGE_KNIGHT", "name": "Starting Equipment (Knight)"},
            {"kind": "grant", "id": "ID_X_CLASS_FEATURE_KNIGHT_VOW", "type": "Class Feature"},
            {"kind": "grant", "id": "ID_X_CLASS_FEATURE_KNIGHT_ASI", "type": "Class Feature", "level": 4}
        ],
        "multiclass": {"id": "ID_X_MULTICLASS_KNIGHT", "prerequisite": "Strength 13", "requirements": "[str:13]",
            "setters": [{"name": "multiclass proficiencies", "value": "Light Armor"}],
            "rules": [{"kind": "grant", "id": "ID_INTERNAL_GRANT_MULTICLASS", "type": "Grants"},
                      {"kind": "grant", "id": "ID_PROFICIENCY_ARMOR_PROFICIENCY_LIGHT_ARMOR", "type": "Proficiency"}]},
        "data": {"hit_die": 10, "fixed_hp_per_level": 6, "saving_throws": ["strength"],
                 "progression": {"vows": [1, 1, 1, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4]},
                 "multiclass": {"prerequisite": "Strength 13", "requirements": "[str:13]"}}
    }));
    let vow = el(json!({
        "id": "ID_X_CLASS_FEATURE_KNIGHT_VOW", "name": "Vow", "type": "Class Feature", "source": "X", "ruleset": "dnd2024",
        "description": "<p>A vow.</p>",
        "rules": [{"kind": "resource", "id": "vow", "name": "Vow", "max": {"type": "table", "key": "vows"}, "recharge": ["long_rest"]}],
        "data": {"class": "Knight", "level": 1}
    }));
    let asi_rules: Vec<Value> = [4, 8]
        .iter()
        .map(|l| {
            json!({"kind": "select_feat", "level": l, "name": format!("Ability Score Improvement (Knight {l})"),
                "category": "General", "default": "ID_2024_FEAT_ABILITY_SCORE_IMPROVEMENT"})
        })
        .collect();
    let asi = el(json!({
        "id": "ID_X_CLASS_FEATURE_KNIGHT_ASI", "name": "Ability Score Improvement", "type": "Class Feature", "source": "X",
        "ruleset": "dnd2024", "description": "<p>Better.</p>",
        "rules": asi_rules,
        "data": {"class": "Knight", "level": 4}
    }));
    (class, vow, asi)
}

fn round_trip(want: &[Element], form: &FormState, cx: &FormContext<'_>) {
    let back = form_to_elements(form, cx).expect("writes");
    assert_eq!(back, want);
}

#[test]
fn a_2024_class_reads_into_its_fields_and_writes_back_unchanged() {
    with_cx(|cx| {
        let (class, vow, asi) = knight();
        let form = element_group_to_form(&class, &[&vow, &asi], cx);
        let f = &form.fields;
        assert_eq!(f["hitDie"], json!("d10"));
        assert_eq!(f["savingThrows"], json!(["Strength"]));
        assert_eq!(f["skills"]["count"], json!(1));
        assert_eq!(f["skills"]["skills"].as_array().unwrap().len(), 2);
        assert_eq!(
            f["armor"][0]["name"],
            json!("Armor Proficiency (Light Armor)")
        );
        assert_eq!(
            f["levels"],
            json!([{"level": 1, "child": "e/0"}, {"level": 4, "child": "e/1"}])
        );
        assert_eq!(f["table"][0]["key"], json!("vows"));
        assert_eq!(f["multiclass"], json!(true));
        assert_eq!(f["mcPrerequisite"], json!("Strength 13"));
        assert_eq!(form.children[1].fields["asiLevels"], json!([4, 8]));
        assert!(
            form.blocks.is_empty(),
            "every class rule is a field's: {:?}",
            form.blocks
        );
        round_trip(&[class.clone(), vow.clone(), asi.clone()], &form, cx);
    });
}

#[test]
fn edited_fields_rewrite_every_location() {
    with_cx(|cx| {
        let (class, vow, asi) = knight();
        let mut form = element_group_to_form(&class, &[&vow, &asi], cx);
        form.fields.insert("hitDie".into(), json!("d12"));
        form.fields
            .insert("savingThrows".into(), json!(["Strength", "Wisdom"]));
        let mut prereq = form.fields["mcRequirements"].clone();
        prereq["value"]["model"] = serde_json::to_value(Node::group(
            GroupMode::All,
            vec![
                Node::row(ConditionRow::Ability {
                    ability: Ability::Strength,
                    min: 13,
                }),
                Node::row(ConditionRow::Ability {
                    ability: Ability::Wisdom,
                    min: 13,
                }),
            ],
        ))
        .unwrap();
        form.fields.insert("mcRequirements".into(), prereq);
        let out = form_to_elements(&form, cx).expect("writes");
        let c = &out[0];
        assert_eq!(c.setter_value("hd"), Some("d12"));
        assert_eq!(c.data["hit_die"], json!(12));
        assert_eq!(c.data["fixed_hp_per_level"], json!(7));
        assert_eq!(c.data["saving_throws"], json!(["strength", "wisdom"]));
        let mc = c.multiclass.as_ref().unwrap();
        assert_eq!(mc.requirements, "[str:13]&&[wis:13]");
        assert_eq!(
            mc.prerequisite, "Strength 13 and Wisdom 13",
            "the sentence follows the condition"
        );
        assert_eq!(
            c.data["multiclass"]["prerequisite"],
            json!("Strength 13 and Wisdom 13")
        );
        let grants: Vec<String> =
            c.rules
                .iter()
                .filter_map(|r| match r {
                    borealis_content::Rule::Grant(g) if g.id.contains("SAVINGTHROW") => Some(
                        format!("{}{}", g.id, g.requirements.clone().unwrap_or_default()),
                    ),
                    _ => None,
                })
                .collect();
        assert_eq!(
            grants,
            [
                "ID_PROFICIENCY_SAVINGTHROW_STRENGTH!ID_X_MULTICLASS_KNIGHT",
                "ID_PROFICIENCY_SAVINGTHROW_WISDOM!ID_X_MULTICLASS_KNIGHT"
            ]
        );
    });
}

#[test]
fn a_renamed_class_renames_its_generated_choices_and_its_features_follow() {
    with_cx(|cx| {
        let (class, vow, asi) = knight();
        let mut form = element_group_to_form(&class, &[&vow, &asi], cx);
        form.fields.insert("name".into(), json!("Warden"));
        let mut levels = form.fields["levels"].clone();
        levels[1]["level"] = json!(6);
        form.fields.insert("levels".into(), levels);
        form.children[1]
            .fields
            .insert("asiLevels".into(), json!([6, 10]));
        let out = form_to_elements(&form, cx).expect("writes");
        let asi = &out[2];
        assert_eq!(asi.data["class"], json!("Warden"));
        assert_eq!(asi.data["level"], json!(6));
        let names: Vec<String> = asi
            .rules
            .iter()
            .map(|r| {
                serde_json::to_value(r).unwrap()["name"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(
            names,
            [
                "Ability Score Improvement (Warden 6)",
                "Ability Score Improvement (Warden 10)"
            ]
        );
        assert_eq!(
            out[1].data["class"],
            json!("Warden"),
            "the vow follows its class"
        );
    });
}

#[test]
fn a_copy_gets_its_own_multiclass_id_everywhere() {
    with_cx(|cx| {
        let (class, _, _) = knight();
        let mut form = element_to_form(&class, cx);
        form.memo.id = None;
        form.memo.multiclass_id = None;
        form.fields.insert("name".into(), json!("Knight (copy)"));
        let mut ids = IdGenerator::new(
            "HB",
            ["ID_X_CLASS_KNIGHT", "ID_X_MULTICLASS_KNIGHT"],
            Vec::<String>::new(),
        )
        .expect("generator");
        let form = assign_ids(&form, &mut ids);
        assert_eq!(
            form.memo.multiclass_id.as_deref(),
            Some("ID_HB_MULTICLASS_KNIGHT_COPY")
        );
        let e = form_to_element(&form, cx).expect("writes");
        let text = serde_json::to_string(&e).unwrap();
        assert!(!text.contains("ID_X_MULTICLASS_KNIGHT"), "{text}");
        assert_eq!(
            e.multiclass.as_ref().unwrap().id,
            "ID_HB_MULTICLASS_KNIGHT_COPY"
        );
        assert!(text.contains("!ID_HB_MULTICLASS_KNIGHT_COPY"));
    });
}

#[test]
fn a_new_2014_class_writes_the_2014_shapes() {
    with_cx(|cx| {
        let mut form = new_class_form(Ruleset::Dnd2014);
        assert_eq!(form.children.len(), 2, "no epic boon in 2014");
        form.fields.insert("name".into(), json!("Knight"));
        form.fields.insert("hitDie".into(), json!("d8"));
        form.fields.insert(
            "skills".into(),
            json!({"count": 1, "skills": [{"id": "ID_PROFICIENCY_SKILL_STEALTH", "name": "", "type": "Proficiency"}]}),
        );
        let mut ids =
            IdGenerator::new("HB", Vec::<String>::new(), Vec::<String>::new()).expect("generator");
        let form = assign_ids(&form, &mut ids);
        let out = form_to_elements(&form, cx).expect("writes");
        let class = serde_json::to_value(&out[0]).unwrap();
        assert_eq!(class["ruleset"], json!("dnd2014"));
        assert_eq!(class["multiclass"]["id"], json!("ID_HB_MULTICLASS_KNIGHT"));
        let feature_grants: Vec<&Value> = class["rules"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["type"] == "Class Feature")
            .collect();
        assert_eq!(
            feature_grants[0]["level"],
            json!(3),
            "the 2014 SRD writes every grant's level"
        );
        let asi = serde_json::to_value(&out[1]).unwrap();
        assert_eq!(
            asi["rules"][0],
            json!({"kind": "select", "type": "Class Feature", "name": "Improvement Option (Knight 4)", "level": 4,
                   "supports": "Improvement Option,Knight,4"})
        );
        assert_eq!(asi["rules"].as_array().unwrap().len(), 5);
        let sub = serde_json::to_value(&out[2]).unwrap();
        assert_eq!(
            sub["rules"],
            json!([{"kind": "select", "type": "Archetype", "name": "Subclass", "level": 3, "supports": "Subclass"}])
        );
    });
}

#[test]
fn a_child_on_no_level_is_a_problem_and_add_child_lists_it() {
    with_cx(|cx| {
        let mut form = new_class_form(Ruleset::Dnd2024);
        form.fields.insert("name".into(), json!("Knight"));
        form.fields.insert("hitDie".into(), json!("d8"));
        let feature = kind("classFeature").unwrap().new_form("n9");
        let mut orphaned = form.clone();
        orphaned.children.push(feature.clone());
        let mut ids =
            IdGenerator::new("HB", Vec::<String>::new(), Vec::<String>::new()).expect("generator");
        let problems = form_to_elements(&assign_ids(&orphaned, &mut ids), cx).unwrap_err();
        assert!(
            problems
                .iter()
                .any(|p| p.path == "children.3" && p.message.contains("isn't on any level")),
            "{problems:?}"
        );
        let added = add_child(&form, feature, Some(7)).expect("a class takes features");
        assert_eq!(
            added.fields["levels"].as_array().unwrap().last(),
            Some(&json!({"level": 7, "child": "n9"}))
        );
        assert!(add_child(&form, kind("spell").unwrap().new_form("s"), None).is_none());
    });
}

#[test]
fn a_block_gained_again_at_other_levels_writes_its_copies() {
    with_cx(|cx| {
        let feature = el(json!({
            "id": "ID_X_CLASS_FEATURE_KNIGHT_DICE", "name": "Dice", "type": "Class Feature", "source": "X",
            "description": "<p>d</p>",
            "rules": [{"kind": "stat", "name": "sneak-attack:count", "value": "1"},
                      {"kind": "stat", "name": "sneak-attack:count", "value": "1", "level": 3},
                      {"kind": "stat", "name": "sneak-attack:count", "value": "1", "level": 5}]
        }));
        let form = element_to_form(&feature, cx);
        assert_eq!(form.blocks.len(), 1);
        assert_eq!(form.blocks[0].fields[AGAIN_AT], json!([3, 5]));
        assert_eq!(
            form_to_element(&form, cx).unwrap(),
            feature,
            "unchanged: the stored copies"
        );
        let mut edited = form.clone();
        edited.blocks[0]
            .fields
            .insert(AGAIN_AT.into(), json!([3, 5, 7]));
        let e = form_to_element(&edited, cx).unwrap();
        assert_eq!(e.rules.len(), 4);
        assert_eq!(
            serde_json::to_value(&e.rules[3]).unwrap()["level"],
            json!(7)
        );
        let again = element_to_form(&e, cx);
        assert_eq!(again.blocks[0].fields[AGAIN_AT], json!([3, 5, 7]));
    });
}

#[test]
fn a_feat_gives_its_sub_features_through_the_same_children_list() {
    with_cx(|cx| {
        let feat = el(json!({
            "id": "ID_X_FEAT_WARDEN", "name": "Warden", "type": "Feat", "source": "X", "description": "<p>w</p>",
            "rules": [{"kind": "grant", "id": "ID_X_FEAT_FEATURE_WARDEN_WATCH", "type": "Feat Feature"}]
        }));
        let watch = el(json!({
            "id": "ID_X_FEAT_FEATURE_WARDEN_WATCH", "name": "Watch", "type": "Feat Feature", "source": "X",
            "description": "<p>w</p>"
        }));
        let form = element_group_to_form(&feat, &[&watch], cx);
        assert_eq!(form.fields["subFeatures"], json!([{"child": "e/0"}]));
        assert!(form.blocks.is_empty(), "the grant is the children list's");
        round_trip(&[feat.clone(), watch.clone()], &form, cx);
        let mut sub = kind("featFeature").unwrap().new_form("n1");
        sub.fields.insert("name".into(), json!("Ward"));
        let added = add_child(&form, sub, None).expect("a feat takes sub-features");
        assert_eq!(
            added.fields["subFeatures"],
            json!([{"child": "e/0"}, {"child": "n1"}])
        );
        let mut ids =
            IdGenerator::new("HB", ["ID_X_FEAT_WARDEN"], Vec::<String>::new()).expect("generator");
        let out = form_to_elements(&assign_ids(&added, &mut ids), cx).expect("writes");
        let grants: Vec<String> = serde_json::to_value(&out[0].rules)
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            grants,
            [
                "ID_X_FEAT_FEATURE_WARDEN_WATCH",
                "ID_HB_FEAT_FEATURE_WARDEN_WARD"
            ]
        );
    });
}

/// The ids and levels of an element's rules, in order (`select_feat@4`, `ID_...`, `stat@3`).
fn rule_shapes(e: &Element) -> Vec<String> {
    serde_json::to_value(&e.rules)
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let what = r["id"].as_str().unwrap_or(r["kind"].as_str().unwrap());
            match r["level"].as_u64() {
                Some(l) => format!("{what}@{l}"),
                None => what.to_string(),
            }
        })
        .collect()
}

#[test]
fn a_new_proficiency_goes_in_its_srd_place_among_the_stored_rules() {
    with_cx(|cx| {
        let (class, vow, asi) = knight();
        let mut form = element_group_to_form(&class, &[&vow, &asi], cx);
        form.fields.insert(
            "weapons".into(),
            json!([{"id": "ID_PROFICIENCY_WEAPON_PROFICIENCY_SIMPLE_WEAPONS", "name": "", "type": "Proficiency"}]),
        );
        let out = form_to_elements(&form, cx).expect("writes");
        assert_eq!(
            rule_shapes(&out[0]),
            [
                "ID_PROFICIENCY_SAVINGTHROW_STRENGTH",
                "select",
                "ID_PROFICIENCY_WEAPON_PROFICIENCY_SIMPLE_WEAPONS",
                "ID_PROFICIENCY_ARMOR_PROFICIENCY_LIGHT_ARMOR",
                "equipment_package",
                "ID_X_CLASS_FEATURE_KNIGHT_VOW",
                "ID_X_CLASS_FEATURE_KNIGHT_ASI@4",
            ],
            "2024: saves, skills, weapons, armor; the unchanged fields keep their ranks"
        );
    });
}

/// `rules` of the Knight's ASI feature after a block gained again at levels 3 and 5 (as stored).
fn asi_after_copies(asi: &Element) -> Element {
    let mut asi = asi.clone();
    let mut rules: Vec<borealis_content::Rule> = serde_json::from_value(json!([
        {"kind": "stat", "name": "vow:count", "value": "1"},
        {"kind": "stat", "name": "vow:count", "value": "1", "level": 3},
        {"kind": "stat", "name": "vow:count", "value": "1", "level": 5}
    ]))
    .unwrap();
    rules.append(&mut asi.rules);
    asi.rules = rules;
    asi
}

#[test]
fn a_block_s_copies_are_no_field_s_neighbours() {
    with_cx(|cx| {
        let (class, vow, asi) = knight();
        let asi = asi_after_copies(&asi);
        let mut form = element_group_to_form(&class, &[&vow, &asi], cx);
        assert_eq!(form.children[1].blocks.len(), 1);
        round_trip(&[class.clone(), vow.clone(), asi.clone()], &form, cx);
        form.children[1]
            .fields
            .insert("epicBoon".into(), json!(true));
        let out = form_to_elements(&form, cx).expect("writes");
        assert_eq!(
            rule_shapes(&out[2]),
            [
                "stat",
                "stat@3",
                "stat@5",
                "select_feat@4",
                "select_feat@8",
                "select_feat@4"
            ],
            "the epic boon after the field before it (the improvements), not before the block of the copies"
        );
    });
}

#[test]
fn blocks_after_a_block_s_copies_keep_their_places() {
    with_cx(|cx| {
        let (class, vow, asi) = knight();
        let mut asi = asi.clone();
        let more: Vec<borealis_content::Rule> = serde_json::from_value(json!([
            {"kind": "stat", "name": "vow:count", "value": "1"},
            {"kind": "stat", "name": "vow:count", "value": "1", "level": 3},
            {"kind": "stat", "name": "vow:count", "value": "1", "level": 5},
            {"kind": "grant", "id": "ID_PROFICIENCY_SKILL_STEALTH", "type": "Proficiency"}
        ]))
        .unwrap();
        asi.rules.extend(more);
        let form = element_group_to_form(&class, &[&vow, &asi], cx);
        let feature = &form.children[1];
        assert_eq!(feature.fields["asiLevels"], json!([4, 8]));
        assert_eq!(feature.blocks.len(), 2);
        round_trip(&[class.clone(), vow.clone(), asi.clone()], &form, cx);
        let mut forgetful = form.clone();
        forgetful.children[1].memo.fields.clear();
        let out = form_to_elements(&forgetful, cx).expect("writes");
        assert_eq!(
            rule_shapes(&out[2]),
            rule_shapes(&asi),
            "canonical: the stored places"
        );
    });
}

#[test]
fn a_stored_multiclass_section_without_the_engine_marker_writes_back_as_stored() {
    // a community class whose multiclass section has no rules at all (WB-27a corpus identity)
    with_cx(|cx| {
        let (mut class, vow, asi) = knight();
        class.multiclass.as_mut().unwrap().rules.clear();
        let want = [class.clone(), vow.clone(), asi.clone()];
        let form = crate::form::element_group_to_form(&class, &[&vow, &asi], cx);
        round_trip(&want, &form, cx);
    });
}
