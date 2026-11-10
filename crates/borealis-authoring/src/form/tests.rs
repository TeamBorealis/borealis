//! Hand-written cases per field and per part of the form model (spec 18.1 "every kind ... on hand-written cases per
//! field"). The bundled corpus is covered by `tests/form_identity.rs`.

use super::*;
use crate::expr_build::condition::{ConditionLanguage, ConditionRow};
use crate::expr_build::{ExprLanguage, ExprValue, GroupMode, Node};
use crate::ids::IdGenerator;
use borealis_content::rich;
use borealis_content::{Append, Element, Rule};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};

fn lookup() -> HashMap<String, RefInfo> {
    let mut m = HashMap::new();
    for (id, name, ty) in [
        ("ID_LANGUAGE_COMMON", "Common", "Language"),
        ("ID_2024_LANGUAGE_COMMON", "Common", "Language"),
        ("ID_WIZARD", "Wizard", "Class"),
    ] {
        m.insert(
            id.to_string(),
            RefInfo {
                name: name.into(),
                ty: ty.into(),
            },
        );
    }
    m
}

fn with_cx<T>(f: impl FnOnce(&FormContext<'_>) -> T) -> T {
    let l = lookup();
    let codec = CatalogBlockCodec;
    let cx = FormContext {
        conditions: ConditionContext::new(["Warlock", "Wizard"]),
        lookup: &l,
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

fn language_2024() -> Element {
    el(json!({
        "id": "ID_2024_LANGUAGE_GIANT", "name": "Giant", "type": "Language", "source": "SRD 5.2.1",
        "ruleset": "dnd2024", "supports": ["Lore", "Standard", "Old"],
        "description": "<p>Giant language.</p>",
        "setters": [{"name": "speakers", "value": "Giants"}, {"name": "standard", "value": "true"},
                    {"name": "script", "value": "Dwarvish"}],
        "data": {"rarity": "standard", "family": "giant"},
        "maps_to": {"dnd2014": "ID_LANGUAGE_COMMON", "kind": "approximate"}
    }))
}

fn language_2014() -> Element {
    el(json!({
        "id": "ID_LANGUAGE_ABYSSAL", "name": "Abyssal", "type": "Language", "source": "Core",
        "supports": ["ID_INTERNAL_SUPPORT_LANGUAGE_EXOTIC"],
        "setters": [{"name": "Speakers", "value": "Demons"}, {"name": "Script", "value": "Infernal"},
                    {"name": "Exotic", "value": "true"}],
        "description": "Abyssal (Language)."
    }))
}

#[test]
fn language_fields_read_from_every_location() {
    with_cx(|cx| {
        let f = element_to_form(&language_2024(), cx);
        assert_eq!(f.kind, "language");
        assert_eq!(f.id(), Some("ID_2024_LANGUAGE_GIANT"));
        assert_eq!(f.fields["rarity"], json!("standard"));
        assert_eq!(f.fields["script"], json!("Dwarvish"));
        assert_eq!(f.fields["speakers"], json!("Giants"));
        assert_eq!(f.fields["tags"], json!(["Lore", "Old"]));
        assert_eq!(f.fields["edition"], json!("dnd2024"));
        assert_eq!(f.fields["compendium"], json!(true));
        assert_eq!(
            f.fields["mapsTo"],
            json!({"dnd2014": {"id": "ID_LANGUAGE_COMMON", "name": "Common", "type": "Language"},
                   "dnd2024": null, "kind": "approximate"})
        );
        let keys: Vec<&str> = f.preserved.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys, ["data.family"]);
        assert_eq!(f.preserved[0].label, "Stored detail “family”");
        assert_eq!(one(&f, cx), language_2024());
    });
}

#[test]
fn edited_rarity_rewrites_setter_tag_and_mirror_in_place() {
    with_cx(|cx| {
        let f = element_to_form(&language_2024(), cx).with_field("rarity", json!("exotic"));
        let e = one(&f, cx);
        let names: Vec<&str> = e.setters.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["speakers", "exotic", "script"]);
        assert_eq!(e.supports, ["Lore", "Exotic", "Old"]);
        assert_eq!(e.data["rarity"], json!("exotic"));
        // a 2014 language has no data mirror: none is added; its setter keeps the stored spelling
        let f = element_to_form(&language_2014(), cx);
        assert_eq!(f.fields["rarity"], json!("exotic"));
        assert_eq!(one(&f, cx), language_2014());
        let e = one(&f.with_field("rarity", json!("exotic")), cx);
        assert_eq!(e, language_2014());
        let e = one(&f.with_field("rarity", json!("secret")), cx);
        assert!(e.data.is_empty());
        assert_eq!(e.setters[2].name, "secret");
        assert_eq!(
            e.supports,
            ["ID_INTERNAL_SUPPORT_LANGUAGE_EXOTIC", "Secret"]
        );
        let e = one(&f.with_field("rarity", json!("")), cx);
        assert_eq!(e.setters.len(), 2);
        let bad = form_to_element(&f.with_field("rarity", json!("rare")), cx).unwrap_err();
        assert_eq!(bad[0].path, "fields.rarity");
    });
}

#[test]
fn edited_text_fields_are_normalized_and_keep_setter_order_and_spelling() {
    with_cx(|cx| {
        let f = element_to_form(&language_2014(), cx)
            .with_field("name", json!("  Deep\u{a0}Speech \u{7}"))
            .with_field("script", json!(" Elvish "))
            .with_field("speakers", json!(""));
        let e = one(&f, cx);
        assert_eq!(e.name, "Deep Speech");
        let pairs: Vec<(&str, &str)> = e
            .setters
            .iter()
            .map(|s| (s.name.as_str(), s.value.as_str()))
            .collect();
        assert_eq!(pairs, [("Script", "Elvish"), ("Exotic", "true")]);
        // a field the UI does not send is unchanged
        let mut partial = element_to_form(&language_2014(), cx);
        partial.fields.remove("script");
        assert_eq!(one(&partial, cx), language_2014());
    });
}

#[test]
fn edition_tags_summary_flags_and_chips() {
    with_cx(|cx| {
        let f = element_to_form(&language_2014(), cx)
            .with_field("edition", json!("dnd2024"))
            .with_field(
                "tags",
                json!([" New ", "", "ID_INTERNAL_SUPPORT_LANGUAGE_EXOTIC"]),
            )
            .with_field("summary", json!("Spoken by demons."))
            .with_field("allowDuplicate", json!(true))
            .with_field("keywords", json!(["fiend", " planar ", ""]));
        let e = one(&f, cx);
        assert_eq!(e.ruleset, Some(borealis_content::Ruleset::Dnd2024));
        assert_eq!(
            e.supports,
            ["New", "ID_INTERNAL_SUPPORT_LANGUAGE_EXOTIC"],
            "the field's order, in its slots"
        );
        assert_eq!(e.setter_value("short"), Some("Spoken by demons."));
        assert_eq!(e.setter_value("allow duplicate"), Some("true"));
        assert_eq!(e.setter_value("keywords"), Some("fiend, planar"));
        let back = element_to_form(&e, cx);
        assert_eq!(back.fields["keywords"], json!(["fiend", "planar"]));
        assert_eq!(back.fields["allowDuplicate"], json!(true));
        let off = one(
            &back
                .with_field("allowDuplicate", json!(false))
                .with_field("edition", json!("")),
            cx,
        );
        assert!(off.setter("allow duplicate").is_none());
        assert!(off.ruleset.is_none());
        let bad = form_to_element(&back.with_field("edition", json!("dnd2030")), cx).unwrap_err();
        assert_eq!(bad[0].path, "fields.edition");
    });
}

#[test]
fn requirements_keep_their_spelling_until_edited() {
    with_cx(|cx| {
        let mut e = language_2014();
        e.requirements = "[str:13],[warlock:2]".into();
        e.prerequisite = "Strength 13".into();
        let f = element_to_form(&e, cx);
        assert_eq!(one(&f, cx).requirements, "[str:13],[warlock:2]");
        let model = Node::group(
            GroupMode::Any,
            vec![
                Node::row(ConditionRow::ClassLevel {
                    class: "warlock".into(),
                    min: 3,
                }),
                Node::row(ConditionRow::Has {
                    id: "ID_WIZARD".into(),
                }),
            ],
        );
        let edited = f
            .with_field(
                "requirements",
                serde_json::to_value(ExprValue::new(model.clone())).unwrap(),
            )
            .with_field("prerequisiteText", json!("Warlock 3 "));
        let out = one(&edited, cx);
        assert_eq!(
            out.requirements,
            ConditionLanguage::render(&model, &cx.conditions)
        );
        assert_eq!(out.prerequisite, "Warlock 3");
        let empty: crate::expr_build::condition::Condition = Node::group(GroupMode::All, vec![]);
        let cleared = f.with_field(
            "requirements",
            serde_json::to_value(ExprValue::new(empty)).unwrap(),
        );
        assert_eq!(one(&cleared, cx).requirements, "");
        // builder problems point at the field
        let bad = Node::group(
            GroupMode::All,
            vec![Node::row(ConditionRow::Feature {
                name: String::new(),
            })],
        );
        let problems = check_fields(
            &f.with_field(
                "requirements",
                serde_json::to_value(ExprValue::new(bad)).unwrap(),
            ),
            cx,
        );
        assert!(
            problems
                .iter()
                .any(|p| p.path.starts_with("fields.requirements")),
            "{problems:?}"
        );
    });
}

#[test]
fn description_sheet_compendium_and_counterpart() {
    with_cx(|cx| {
        let mut e = language_2014();
        e.sheet = Some(serde_json::from_value(json!({"display": false, "alt": "Aby",
            "descriptions": [{"text": "Demon tongue", "level": 3}], "extra": {"attributes": {"x": "1"}}})).unwrap());
        e.compendium = Some(serde_json::from_value(json!({"display": false})).unwrap());
        let f = element_to_form(&e, cx);
        assert_eq!(one(&f, cx), e);
        assert_eq!(f.fields["sheet"]["lines"][0]["level"], json!(3));
        assert!(f.preserved.iter().any(|p| p.key == "sheet.extra"));
        let doc = rich::parse("<p>New <b>text</b></p>");
        let edited = f
            .with_field("description", serde_json::to_value(&doc).unwrap())
            .with_field("sheet", json!({"display": null, "alt": " ", "usage": "1/Long Rest", "action": null,
                "lines": [{"text": " Roar ", "level": null, "usage": null, "action": null}, {"text": ""}]}))
            .with_field("compendium", json!(true))
            .with_field("mapsTo", json!({"dnd2014": null, "dnd2024": {"id": "ID_2024_LANGUAGE_COMMON",
                "name": "", "type": ""}, "kind": "equivalent"}));
        let out = one(&edited, cx);
        assert_eq!(out.description, "<p>New <b>text</b></p>");
        let sheet = out.sheet.clone().unwrap();
        assert_eq!(sheet.usage.as_deref(), Some("1/Long Rest"));
        assert!(sheet.alt.is_none());
        assert_eq!(sheet.descriptions.len(), 1);
        assert_eq!(sheet.descriptions[0].text, "Roar");
        assert_eq!(
            sheet.extra.attributes["x"], "1",
            "the kept sheet settings stay"
        );
        assert!(out.compendium.is_none());
        assert_eq!(
            out.maps_to.as_ref().unwrap().dnd2024.as_deref(),
            Some("ID_2024_LANGUAGE_COMMON")
        );
        let cleared = one(
            &edited
                .with_field("sheet", Value::Null)
                .with_field(
                    "mapsTo",
                    json!({"dnd2014": null, "dnd2024": null, "kind": null}),
                )
                .without_preserved("sheet.extra"),
            cx,
        );
        assert!(cleared.sheet.is_none() && cleared.maps_to.is_none());
        let bad = form_to_element(&f.with_field("sheet", json!("nope")), cx).unwrap_err();
        assert_eq!(bad[0].path, "fields.sheet");
    });
}

fn spell() -> Element {
    el(json!({
        "id": "ID_X_SPELL_EMBER", "name": "Ember", "type": "Spell", "source": "X", "ruleset": "dnd2024",
        "supports": ["Wizard"], "description": "<p>Fire.</p>",
        "setters": [{"name": "level", "value": "1"}, {"name": "short", "value": "Hot."},
                    {"name": "school", "value": "Evocation", "attributes": {"addition": "x"}}],
        "data": {"level": 1, "classes": ["Wizard"]},
        "rules": [
            {"kind": "stat", "name": "ac", "value": "1", "merge": true},
            {"kind": "unknown", "tag": "odd", "text": "?"},
            {"kind": "grant", "id": "ID_WIZARD", "type": "Class"},
            {"kind": "resource", "id": "embers", "max": {"type": "fixed", "value": 2}, "bogus": 1},
            {"kind": "made_up", "x": 1}
        ],
        "profile_overrides": {"dnd2014": {"school": "Conjuration"}},
        "renamed_from": "ID_OLD",
        "extra": {"attributes": {"aurora": "1"}, "children": [{"tag": "note", "text": "hi"}]}
    }))
}

#[test]
fn generic_form_rows_blocks_and_preserved_parts() {
    with_cx(|cx| {
        let e = spell();
        let f = element_to_form(&e, cx);
        assert_eq!(
            f.kind, OTHER_KIND,
            "spells open in the generic form until their own form exists"
        );
        assert_eq!(f.fields["type"], json!("Spell"));
        assert_eq!(f.fields["summary"], json!("Hot."));
        assert_eq!(f.fields["setters"].as_array().unwrap().len(), 2);
        assert_eq!(one(&f, cx), e);
        let keys: BTreeSet<&str> = f.preserved.iter().map(|p| p.key.as_str()).collect();
        let want: BTreeSet<&str> = [
            "data.classes",
            "data.level",
            "profile_overrides",
            "renamed_from",
            "extra.attribute.aurora",
            "extra.child.0",
        ]
        .into();
        assert_eq!(keys, want);
        // blocks: kept parts of rules
        let kinds: Vec<&str> = f.blocks.iter().map(|b| b.kind.as_str()).collect();
        assert_eq!(kinds, ["stat", "unknown", "grant", "resource", "made_up"]);
        assert_eq!(f.blocks[0].preserved[0].key, "merge");
        assert_eq!(f.blocks[1].preserved[0].key, "rule");
        assert_eq!(f.blocks[3].preserved[0].key, "bogus");
        assert!(f.blocks[4].fields.is_empty());
        // edit a row in place and add one: the new row goes last
        let mut rows = f.fields["setters"].clone();
        rows[0]["value"] = json!("2");
        rows.as_array_mut()
            .unwrap()
            .push(json!({"name": "range", "value": "Self", "attributes": {}}));
        let mut g = f
            .with_field("setters", rows)
            .without_preserved("data.level");
        g.blocks[1] = g.blocks[1].clone();
        g.blocks[1].preserved.clear();
        g.blocks[0].preserved.clear();
        let out = one(&g, cx);
        let names: Vec<&str> = out.setters.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["level", "short", "school", "range"]);
        assert_eq!(out.setters[0].value, "2");
        assert_eq!(out.setters[2].attributes["addition"], "x");
        assert!(!out.data.contains_key("level"));
        assert_eq!(out.rules.len(), 4, "the unknown rule was removed");
        assert!(matches!(&out.rules[0], Rule::Stat(s) if s.merge.is_none()));
    });
}

#[test]
fn wire_form_strips_and_merge_restores_kept_bytes() {
    with_cx(|cx| {
        let f = element_to_form(&spell(), cx);
        let ui = to_ui(&f);
        assert!(ui.memo.is_empty());
        assert!(ui.preserved.iter().all(|p| p.kept.is_none()));
        assert!(ui
            .blocks
            .iter()
            .flat_map(|b| &b.preserved)
            .all(|p| p.kept.is_none()));
        let wire = serde_json::to_value(&ui).unwrap();
        assert!(wire.get("memo").is_none());
        assert!(wire["preserved"][0].get("kept").is_none());
        assert_eq!(one(&merge_ui(&f, &ui), cx), spell());
        // the UI removes one part and forges another: the removed one is gone, the forged one is dropped
        let mut edited = ui.without_preserved("renamed_from");
        edited.preserved.push(PreservedPart {
            key: "multiclass".into(),
            label: "x".into(),
            kept: Some(KeptPart {
                location: "multiclass".into(),
                value: json!({"id": "ID_EVIL"}),
                slot: None,
            }),
        });
        let out = one(&merge_ui(&f, &edited), cx);
        assert!(out.renamed_from.is_none());
        assert!(out.multiclass.is_none());
        // the UI form alone has no id and no kept bytes
        let alone = form_to_element(&ui, cx).unwrap_err();
        assert_eq!(alone[0].path, "id");
    });
}

#[test]
fn children_are_written_after_their_parent_and_matched_by_key() {
    with_cx(|cx| {
        let parent = language_2014();
        let child = language_2024();
        let f = element_group_to_form(&parent, &[&child], cx);
        assert_eq!(f.children[0].key, "e/0");
        assert_eq!(
            form_to_elements(&f, cx).unwrap(),
            vec![parent.clone(), child.clone()]
        );
        let mut ui = to_ui(&f);
        ui.children
            .push(FormState::new("new", "language").with_field("name", json!("Gnoll")));
        ui.children.swap(0, 1);
        let merged = merge_ui(&f, &ui);
        assert_eq!(merged.children[1].id(), Some("ID_2024_LANGUAGE_GIANT"));
        assert!(merged.children[0].id().is_none());
        let problems = form_to_elements(&merged, cx).unwrap_err();
        assert_eq!(problems[0].path, "children.0.id");
        let mut ids =
            IdGenerator::new("GL", ["ID_LANGUAGE_ABYSSAL"], Vec::<String>::new()).unwrap();
        let assigned = assign_ids(&merged, &mut ids);
        let out = form_to_elements(&assigned, cx).unwrap();
        assert_eq!(out[1].id, "ID_GL_LANGUAGE_ABYSSAL_GNOLL");
        assert_eq!(out[1].ty, "Language");
        assert_eq!(out[1].name, "Gnoll");
        assert_eq!(out[2], child);
        assert_eq!(
            form_to_elements(&assign_ids(&assigned, &mut ids), cx).unwrap(),
            out,
            "ids are frozen"
        );
    });
}

#[test]
fn a_new_element_gets_its_id_and_type() {
    with_cx(|cx| {
        let mut ids = IdGenerator::new("GL", ["ID_GL_DEITY_ASHA"], Vec::<String>::new()).unwrap();
        let f = FormState::new("e", "deity")
            .with_field("name", json!("Asha"))
            .with_field("domains", json!(["Light", "Life"]));
        let e = one(&assign_ids(&f, &mut ids), cx);
        assert_eq!(e.id, "ID_GL_DEITY_ASHA_2");
        assert_eq!(e.ty, "Deity");
        assert_eq!(e.setter_value("domains"), Some("Light, Life"));
        let other = FormState::new("e", OTHER_KIND)
            .with_field("name", json!("Wolf"))
            .with_field("type", json!("Companion"));
        assert_eq!(
            one(&assign_ids(&other, &mut ids), cx).id,
            "ID_GL_COMPANION_WOLF"
        );
        let unknown = FormState::new("e", "nope");
        assert!(form_to_element(&unknown, cx).is_err());
    });
}

#[test]
fn appends_round_trip_and_edit() {
    with_cx(|cx| {
        let a: Append = serde_json::from_value(json!({
            "id": "ID_WIZARD", "type": "Class", "supports": ["Fire Mage"],
            "setters": [{"name": "short", "value": "x"}],
            "rules": [{"kind": "grant", "id": "ID_LANGUAGE_COMMON", "type": "Language"}],
            "spellcasting": {"name": "Wizard", "extend": true, "extends": [{"supports": "ID_X_SPELL_EMBER"}]},
            "extra": {"attributes": {"z": "1"}}
        }))
        .unwrap();
        let f = append_to_form(&a, cx);
        assert_eq!(f.kind, APPEND_KIND);
        assert!(f.id().is_none());
        assert_eq!(f.fields["target"]["name"], json!("Wizard"));
        assert_eq!(form_to_append(&f, cx).unwrap(), a);
        assert_eq!(form_to_append(&merge_ui(&f, &to_ui(&f)), cx).unwrap(), a);
        let edited = f
            .with_field(
                "target",
                json!({"id": "ID_LANGUAGE_COMMON", "name": "Common", "type": "Language"}),
            )
            .with_field("type", json!(""))
            .with_field("tags", json!(["Fire Mage", "Ice Mage"]));
        let out = form_to_append(&edited, cx).unwrap();
        assert_eq!(out.id, "ID_LANGUAGE_COMMON");
        assert!(out.ty.is_none());
        assert_eq!(out.supports, ["Fire Mage", "Ice Mage"]);
        assert!(out.spellcasting.is_some());
        let none = form_to_append(&f.with_field("target", Value::Null), cx).unwrap_err();
        assert_eq!(none[0].path, "fields.target");
        let mut ids = IdGenerator::new("GL", Vec::<String>::new(), Vec::<String>::new()).unwrap();
        assert!(
            assign_ids(&f, &mut ids).id().is_none(),
            "an append has no id of its own"
        );
    });
}

#[test]
fn problem_locations_map_to_form_paths() {
    with_cx(|cx| {
        let mut e = spell();
        e.multiclass = Some(serde_json::from_value(json!({"id": "ID_M"})).unwrap());
        e.requirements = "[str:13]".into();
        let mut f = element_to_form(&e, cx);
        // the unknown rule's kept part is removed: the rules after it move up one
        f.blocks[1].preserved.clear();
        let out = one(&f, cx);
        let at = |loc: &str| form_path_for_location(&f, &out, loc, cx);
        assert_eq!(at("requirements").as_deref(), Some("fields.requirements"));
        assert_eq!(at("rules[0]").as_deref(), Some("blocks.0"));
        assert_eq!(at("rules[1]").as_deref(), Some("blocks.2"));
        assert_eq!(at("rules[9]"), None);
        assert_eq!(at("setters[1]").as_deref(), Some("fields.summary"));
        assert_eq!(at("setters[0]").as_deref(), Some("fields.setters"));
        assert_eq!(at("setter:school").as_deref(), Some("fields.setters"));
        assert_eq!(at("data.level").as_deref(), Some("preserved.data.level"));
        assert_eq!(
            at("multiclass.rules[0]").as_deref(),
            Some("preserved.multiclass")
        );
        assert_eq!(at("description").as_deref(), Some("fields.description"));
        assert_eq!(at("spellcasting.list"), None);
        let lang = element_to_form(&language_2024(), cx);
        let lang_e = one(&lang, cx);
        assert_eq!(
            form_path_for_location(&lang, &lang_e, "setter:script", cx).as_deref(),
            Some("fields.script")
        );
        assert_eq!(
            form_path_for_location(&lang, &lang_e, "data.rarity", cx).as_deref(),
            Some("fields.rarity")
        );
        let group = element_group_to_form(&language_2014(), &[&language_2024()], cx);
        let elements = form_to_elements(&group, cx).unwrap();
        assert_eq!(
            form_path_for_issue(
                &group,
                &elements,
                "ID_2024_LANGUAGE_GIANT",
                Some("setters[2]"),
                cx
            )
            .as_deref(),
            Some("children.0.fields.script")
        );
        assert_eq!(
            form_path_for_issue(&group, &elements, "ID_2024_LANGUAGE_GIANT", None, cx).as_deref(),
            Some("children.0")
        );
        assert_eq!(
            form_path_for_issue(&group, &elements, "ID_LANGUAGE_ABYSSAL", None, cx).as_deref(),
            Some("")
        );
    });
}

#[test]
fn kinds_bind_every_location_once() {
    for k in kinds() {
        let mut keys = BTreeSet::new();
        let mut setters = BTreeSet::new();
        let mut data = BTreeSet::new();
        let mut singles = BTreeSet::new();
        let tabs: BTreeSet<&str> = k.tabs.iter().map(|(id, _)| *id).collect();
        for f in &k.fields {
            assert!(keys.insert(f.key), "{}: duplicate key {}", k.id, f.key);
            assert!(tabs.contains(f.tab), "{}: {} on a missing tab", k.id, f.key);
            assert!(!f.codec.bindings().is_empty(), "{}: {}", k.id, f.key);
            for s in f.codec.setter_names() {
                assert!(setters.insert(s), "{}: setter {s} bound twice", k.id);
            }
            for d in f.codec.data_keys() {
                assert!(data.insert(d), "{}: data.{d} bound twice", k.id);
            }
            if f.codec.setter_names().is_empty() && f.codec.data_keys().is_empty() {
                assert!(
                    singles.insert(format!("{:?}", f.codec)),
                    "{}: {:?} bound twice",
                    k.id,
                    f.codec
                );
            }
        }
        assert!(
            k.field("name").is_some() && k.field("rules").is_some(),
            "{}",
            k.id
        );
        let d = k.descriptor();
        assert_eq!(d.fields.len(), k.fields.len());
        assert_eq!(kind(k.id), Some(k));
    }
    let small: BTreeSet<&str> = kinds()
        .iter()
        .filter(|k| k.group == KindGroup::Small)
        .flat_map(|k| k.element_types.iter().copied())
        .collect();
    for ty in [
        "Language",
        "Proficiency",
        "Condition",
        "Damage Type",
        "Skill",
        "Deity",
        "Vision",
        "Size",
        "Alignment",
        "Weapon Property",
        "Weapon Group",
        "Weapon Category",
        "Armor Group",
        "Mastery Property",
        "Spellcasting Focus Group",
        "Dragonmark",
        "Rules Glossary Entry",
    ] {
        assert!(small.contains(ty), "spec 4.10 small type {ty}");
        assert_eq!(kind_for_type(ty).element_types, [ty]);
    }
    assert_eq!(kind_for_type("Companion").id, OTHER_KIND);
    assert_eq!(kind_for_type("Spell").id, OTHER_KIND);
    assert!(kind(APPEND_KIND).is_some());
    assert!(
        kinds().iter().all(|k| k.id != APPEND_KIND),
        "appends are not a New element kind"
    );
    let all = descriptor::descriptors();
    assert_eq!(all.len(), kinds().len());
    let lang = all.iter().find(|d| d.id == "language").unwrap();
    let rarity = lang.fields.iter().find(|f| f.key == "rarity").unwrap();
    assert_eq!(
        rarity.bindings,
        [
            "setter:standard",
            "setter:exotic",
            "setter:secret",
            "supports:Standard|Exotic|Secret",
            "data.rarity"
        ]
    );
}

#[test]
fn bad_values_and_kept_parts_report_their_paths() {
    with_cx(|cx| {
        let f = element_to_form(&spell(), cx);
        let bad = f
            .with_field("setters", json!("x"))
            .with_field("name", json!(3));
        let problems = form_to_element(&bad, cx).unwrap_err();
        let paths: Vec<&str> = problems.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, ["fields.name", "fields.setters"]);
        let mut broken = f.clone();
        broken.preserved[0].kept.as_mut().unwrap().location = "nowhere".into();
        let problems = form_to_element(&broken, cx).unwrap_err();
        assert!(problems[0].path.starts_with("preserved."), "{problems:?}");
        let mut block = f.clone();
        block.blocks[2].fields.remove("id");
        let problems = form_to_element(&block, cx).unwrap_err();
        assert_eq!(problems[0].path, "blocks.2.fields");
        assert_eq!(check_fields(&f, cx), []);
    });
}
