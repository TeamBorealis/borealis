//! The spellcasting section (spec 4.6), per caster type and per field: a new section on a class's Spellcasting
//! feature, the class table's spellcasting columns kept in step (2024), a custom table read back from stored slot
//! stats, shapes the builder can't reproduce kept as blocks, the section off, and the plain sentences of its
//! problems. The bundled SRD casters are in `tests/caster_section.rs` and `tests/form_identity.rs`.

use crate::expr_build::condition::ConditionContext;
use crate::form::{
    add_child, assign_ids, element_to_form, form_to_element, form_to_elements, kind,
    new_class_form, CatalogBlockCodec, FormContext, FormState, NoLookup,
};
use crate::ids::IdGenerator;
use borealis_content::{Element, Ruleset};
use serde_json::{json, Value};

fn with_cx<T>(f: impl FnOnce(&FormContext<'_>) -> T) -> T {
    let codec = CatalogBlockCodec;
    let cx = FormContext {
        conditions: ConditionContext::new(["Warden"]),
        lookup: &NoLookup,
        blocks: &codec,
    };
    f(&cx)
}

fn ids() -> IdGenerator {
    IdGenerator::new("HB", Vec::<String>::new(), Vec::<String>::new()).expect("generator")
}

fn levels(v: &[u32]) -> Value {
    let mut out = v.to_vec();
    out.resize(20, *v.last().unwrap_or(&0));
    json!(out)
}

/// A new 2024 class "Warden" with a Spellcasting feature at level 1 with these section values.
fn warden(edition: Ruleset, section: &[(&str, Value)]) -> FormState {
    let mut class = new_class_form(edition);
    class.fields.insert("name".into(), json!("Warden"));
    class.fields.insert("hitDie".into(), json!("d10"));
    class.fields.insert(
        "skills".into(),
        json!({"count": 1, "skills": [{"id": "ID_PROFICIENCY_SKILL_STEALTH", "name": "", "type": "Proficiency"}]}),
    );
    let mut sc = kind("classFeature").unwrap().new_form("n9");
    sc.fields.insert("name".into(), json!("Spellcasting"));
    sc.fields.insert("casts".into(), json!(true));
    sc.fields.insert("ability".into(), json!("Wisdom"));
    for (k, v) in section {
        sc.fields.insert((*k).into(), v.clone());
    }
    let class = add_child(&class, sc, Some(1)).expect("a class takes features");
    assign_ids(&class, &mut ids())
}

fn spellcasting(out: &[Element]) -> Value {
    let e = out
        .iter()
        .find(|e| e.spellcasting.is_some())
        .expect("the spellcasting feature");
    serde_json::to_value(e).unwrap()
}

fn stats(feature: &Value, name: &str) -> Vec<(String, Option<u64>)> {
    feature["rules"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "stat" && r["name"] == name)
        .map(|r| {
            (
                r["value"].as_str().unwrap().to_string(),
                r["level"].as_u64(),
            )
        })
        .collect()
}

fn rules_of<'v>(feature: &'v Value, kind: &str) -> Vec<&'v Value> {
    feature["rules"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == kind)
        .collect()
}

#[test]
fn a_new_2024_half_caster_writes_the_srd_shapes_and_its_class_table() {
    with_cx(|cx| {
        let prepared = [
            2, 3, 4, 5, 6, 6, 7, 7, 9, 9, 10, 10, 11, 11, 12, 12, 14, 14, 15, 15,
        ];
        let form = warden(
            Ruleset::Dnd2024,
            &[
                ("casterType", json!("halfUp")),
                ("spellList", json!("Ranger")),
                ("swap", json!(true)),
                ("prepared", json!(prepared)),
                ("preparedChange", json!("longRestOne")),
            ],
        );
        let out = form_to_elements(&form, cx).expect("writes");
        let f = spellcasting(&out);
        assert_eq!(
            f["spellcasting"],
            json!({"name": "Warden", "ability": "Wisdom", "prepare": true, "allow_replace": true,
                "list": {"supports": "Ranger", "known": true}})
        );
        assert_eq!(
            stats(&f, "warden:spellcasting:slots:1"),
            [
                ("2".into(), None),
                ("1".into(), Some(3)),
                ("1".into(), Some(5))
            ]
        );
        assert_eq!(
            stats(&f, "warden:spellcasting:slots:5"),
            [("1".into(), Some(17)), ("1".into(), Some(19))]
        );
        assert_eq!(
            stats(&f, "warden:spellcasting:prepare")[0],
            ("2".into(), None)
        );
        assert_eq!(
            rules_of(&f, "grant"),
            [
                &json!({"kind": "grant", "id": "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_HALF_UP",
                "type": "Grants", "requirements": "ID_INTERNAL_GRANT_MULTICLASS"})
            ]
        );
        assert_eq!(
            rules_of(&f, "prepared_spell_change"),
            [&json!({"kind": "prepared_spell_change", "when": "long_rest", "count": "one"})]
        );
        // the class table: the engine reads `prepared`; the slot columns as the SRD classes have them
        let class = serde_json::to_value(&out[0]).unwrap();
        let table = &class["data"]["progression"];
        assert_eq!(table["prepared"], json!(prepared));
        assert_eq!(table["slot_1"][0], 2);
        assert_eq!(table["slot_5"][16], 1);
        assert!(
            table.get("slot_6").is_none() && table.get("cantrips").is_none(),
            "{table}"
        );
        // read back: the same values, and the class's table is not written again while the section is unchanged
        let group =
            crate::form::element_group_to_form(&out[0], &out[1..].iter().collect::<Vec<_>>(), cx);
        let again = form_to_elements(&group, cx).expect("writes");
        assert_eq!(again, out);
        let sc = group
            .children
            .iter()
            .find(|c| c.fields["casts"] == json!(true))
            .unwrap();
        assert_eq!(sc.fields["casterType"], json!("halfUp"));
        assert_eq!(sc.fields["casterName"], json!(""), "it follows its class");
        assert_eq!(sc.fields["prepared"], json!(prepared));
    });
}

#[test]
fn every_caster_type_writes_its_slots_and_multiclass_grant() {
    with_cx(|cx| {
        for (ty, edition, first, grant) in [
            ("full", Ruleset::Dnd2014, ("2", Some(1)), "FULL"),
            ("halfDown", Ruleset::Dnd2014, ("2", Some(2)), "HALF"),
            ("halfUp", Ruleset::Dnd2014, ("2", Some(1)), "HALF_UP"),
            ("third", Ruleset::Dnd2024, ("2", Some(3)), "THIRD"),
            ("pact", Ruleset::Dnd2024, ("1", None), "SOLO"),
        ] {
            let out = form_to_elements(&warden(edition, &[("casterType", json!(ty))]), cx)
                .expect("writes");
            let f = spellcasting(&out);
            let first_stat = stats(&f, "warden:spellcasting:slots:1")[0].clone();
            assert_eq!(first_stat, (first.0.to_string(), first.1), "{ty}");
            let mc = rules_of(&f, "grant");
            assert_eq!(
                mc[0]["id"],
                json!(format!(
                    "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_{grant}"
                )),
                "{ty}"
            );
            assert_eq!(!rules_of(&f, "pact_magic").is_empty(), ty == "pact", "{ty}");
        }
        // 2014 pact magic is the slots:count pool
        let out = form_to_elements(
            &warden(Ruleset::Dnd2014, &[("casterType", json!("pact"))]),
            cx,
        )
        .unwrap();
        let f = spellcasting(&out);
        assert_eq!(stats(&f, "warden:spellcasting:slots:count").len(), 4);
        assert!(rules_of(&f, "pact_magic").is_empty());
        // a 2024 pact caster's class table has the pact columns the engine reads
        let out = form_to_elements(
            &warden(Ruleset::Dnd2024, &[("casterType", json!("pact"))]),
            cx,
        )
        .unwrap();
        let table = serde_json::to_value(&out[0]).unwrap()["data"]["progression"].clone();
        assert_eq!(table["pact_slots"][16], 4);
        assert_eq!(table["pact_slot_level"][8], 5);
    });
}

#[test]
fn a_custom_table_and_an_explicit_multiclass_grant_round_trip() {
    with_cx(|cx| {
        let mut grid = vec![vec![0u32; 9]; 20];
        for (l, row) in grid.iter_mut().enumerate() {
            row[0] = if l >= 1 { 1 } else { 0 };
            row[1] = if l >= 9 { 1 } else { 0 };
        }
        let form = warden(
            Ruleset::Dnd2014,
            &[
                ("casterType", json!("custom")),
                ("slotTable", json!(grid)),
                ("multiclassSlots", json!("fourth")),
            ],
        );
        let out = form_to_elements(&form, cx).expect("writes");
        let f = spellcasting(&out);
        assert_eq!(
            stats(&f, "warden:spellcasting:slots:1"),
            [("1".into(), Some(2))]
        );
        assert_eq!(
            stats(&f, "warden:spellcasting:slots:2"),
            [("1".into(), Some(10))]
        );
        assert_eq!(
            rules_of(&f, "grant")[0]["id"],
            "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_FOURTH"
        );
        let e = out.iter().find(|e| e.spellcasting.is_some()).unwrap();
        let read = element_to_form(e, cx);
        assert_eq!(read.fields["casterType"], json!("custom"));
        assert_eq!(read.fields["slotTable"], json!(grid));
        assert_eq!(read.fields["multiclassSlots"], json!("fourth"));
        assert!(read.blocks.is_empty(), "{:?}", read.blocks);
        assert_eq!(form_to_element(&read, cx).unwrap(), *e);
        // edited: a cell more, the stats follow
        let mut more = grid.clone();
        more[19][2] = 1;
        let edited = read.with_field("slotTable", json!(more));
        let f = serde_json::to_value(form_to_element(&edited, cx).unwrap()).unwrap();
        assert_eq!(
            stats(&f, "warden:spellcasting:slots:3"),
            [("1".into(), Some(20))]
        );
    });
}

#[test]
fn slot_rules_the_builder_cant_reproduce_stay_blocks() {
    with_cx(|cx| {
        let e: Element = serde_json::from_value(json!({
            "id": "ID_X_CLASS_FEATURE_ODD", "name": "Spellcasting", "type": "Class Feature", "source": "X",
            "spellcasting": {"name": "Odd", "ability": "Wisdom", "prepare": true, "list": {"supports": "Odd", "known": true}},
            "rules": [
                {"kind": "stat", "name": "odd:spellcasting:slots:1", "value": "wisdom:modifier", "level": 2},
                {"kind": "grant", "id": "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_HALF", "type": "Grants",
                    "requirements": "ID_INTERNAL_GRANT_MULTICLASS"}
            ]
        }))
        .unwrap();
        let form = element_to_form(&e, cx);
        assert_eq!(form.fields["casterType"], json!(""));
        assert_eq!(
            form.fields["multiclassSlots"],
            json!("half"),
            "no type: the grant is its own value"
        );
        assert_eq!(form.blocks.len(), 1, "the slot stat stays a block");
        assert_eq!(form_to_element(&form, cx).unwrap(), e);
        // a list extension is not a section: the node stays kept
        let mut ext = e.clone();
        ext.spellcasting.as_mut().unwrap().extend = Some(true);
        let form = element_to_form(&ext, cx);
        assert_eq!(form.fields["casts"], json!(false));
        assert!(form.preserved.iter().any(|p| p.key == "spellcasting"));
        assert_eq!(form.blocks.len(), 2);
        assert_eq!(form_to_element(&form, cx).unwrap(), ext);
    });
}

#[test]
fn a_2014_known_caster_writes_choices_and_counts() {
    with_cx(|cx| {
        let form = warden(
            Ruleset::Dnd2014,
            &[
                ("casterType", json!("full")),
                ("preparation", json!("known")),
                (
                    "cantrips",
                    json!({"levels": levels(&[2, 2, 2, 3]), "statistic": true}),
                ),
                (
                    "known",
                    json!({"levels": levels(&[2, 3, 4]), "statistic": true, "replace": true}),
                ),
            ],
        );
        let out = form_to_elements(&form, cx).expect("writes");
        let f = spellcasting(&out);
        assert_eq!(f["spellcasting"].get("prepare"), None);
        assert_eq!(
            stats(&f, "warden:spellcasting:cantrips known"),
            [("2".into(), Some(1)), ("1".into(), Some(4))]
        );
        let selects = rules_of(&f, "select");
        assert_eq!(selects.len(), 5);
        assert_eq!(
            selects[4],
            &json!({"kind": "select", "type": "Spell", "name": "Spell (Warden)", "number": 1, "level": 3,
                "supports": "$(spellcasting:list),(1||2)", "spellcasting": "Warden", "allow_replace": true})
        );
        assert_eq!(
            rules_of(&f, "prepared_spell_change").len(),
            0,
            "a 2014 section has none"
        );
    });
}

#[test]
fn a_section_turned_off_writes_nothing_and_clears_the_class_table() {
    with_cx(|cx| {
        let form = warden(
            Ruleset::Dnd2024,
            &[("casterType", json!("full")), ("prepared", levels(&[4, 5]))],
        );
        let out = form_to_elements(&form, cx).expect("writes");
        assert!(out[0].data.contains_key("progression"));
        let mut off = form.clone();
        let sc = off.children.iter_mut().find(|c| c.key == "n9").unwrap();
        sc.fields.insert("casts".into(), json!(false));
        let out = form_to_elements(&off, cx).expect("writes");
        let f = out.iter().find(|e| e.name == "Spellcasting").unwrap();
        assert!(f.spellcasting.is_none());
        assert!(f.rules.is_empty(), "{:?}", f.rules);
        assert!(
            !out[0].data.contains_key("progression"),
            "the spellcasting columns go with it"
        );
    });
}

#[test]
fn problems_are_plain_sentences_at_their_fields() {
    with_cx(|cx| {
        let cases: [(&str, Value, &str); 5] = [
            ("ability", json!(""), "Choose the spellcasting ability."),
            (
                "casterType",
                json!("quarter"),
                "Choose a caster type from the list.",
            ),
            (
                "cantrips",
                json!({"levels": levels(&[3, 2]), "statistic": false}),
                "The number of cantrips known can't go down from one level to the next.",
            ),
            (
                "slotTable",
                json!([[1]]),
                "Write a whole number of slots from 0 to 9 in each cell of the table.",
            ),
            (
                "multiclassSlots",
                json!("double"),
                "Choose how the class counts for multiclass spell slots.",
            ),
        ];
        for (key, value, message) in cases {
            let mut section = vec![("casterType", json!("custom")), (key, value.clone())];
            if key == "casterType" {
                section.remove(0);
            }
            let problems = form_to_elements(&warden(Ruleset::Dnd2024, &section), cx).unwrap_err();
            let at = format!("children.3.fields.{key}");
            assert!(
                problems
                    .iter()
                    .any(|p| p.path == at && p.message == message),
                "{key}: {problems:?}"
            );
        }
    });
}

#[test]
fn a_stored_empty_spell_list_tag_writes_back_as_stored() {
    // a community subclass feature whose list tag is empty (WB-27a corpus identity): unchanged, it stays empty;
    // edited to empty, it is the spellcasting name
    with_cx(|cx| {
        let class = warden(Ruleset::Dnd2024, &[]);
        let out = form_to_elements(&class, cx).expect("writes");
        let mut sc = out
            .iter()
            .find(|e| e.spellcasting.is_some())
            .expect("the spellcasting feature")
            .clone();
        sc.spellcasting
            .as_mut()
            .unwrap()
            .list
            .as_mut()
            .unwrap()
            .supports = String::new();
        let form = element_to_form(&sc, cx);
        assert_eq!(form.fields["spellList"], json!(""));
        assert_eq!(form_to_element(&form, cx).expect("writes"), sc);
        let edited = form.with_field("spellList", json!(" "));
        let e = form_to_element(&edited, cx).expect("writes");
        let sc2 = e.spellcasting.unwrap();
        assert_eq!(sc2.list.unwrap().supports, sc2.name.unwrap());
    });
}

#[test]
fn choices_kept_beside_the_section_follow_its_name_when_the_class_is_renamed() {
    with_cx(|cx| {
        let form = warden(
            Ruleset::Dnd2024,
            &[
                ("casterType", json!("full")),
                ("spellList", json!("Wizard")),
                ("preparation", json!("spellbook")),
            ],
        );
        let mut out = form_to_elements(&form, cx).expect("writes");
        // the SRD Wizard's shape: spellbook choices the section does not write, counted under the class's name
        let at = out.iter().position(|e| e.spellcasting.is_some()).unwrap();
        let mut feature = serde_json::to_value(&out[at]).unwrap();
        let rules = feature["rules"].as_array_mut().unwrap();
        rules.push(
            json!({"kind": "select", "type": "Spell", "name": "Spellbook (Warden 1)", "number": 6,
            "spellcasting": "Warden", "supports": "$(spellcasting:list), 1"}),
        );
        rules.push(
            json!({"kind": "select", "type": "Spell", "name": "Spellbook (Warden)", "level": 2,
            "spellcasting": "Warden", "supports": "$(spellcasting:list), 1"}),
        );
        rules.push(json!({"kind": "select", "type": "Spell", "name": "Spell (Sage)", "spellcasting": "Sage",
            "supports": "Sage"}));
        out[at] = serde_json::from_value(feature).unwrap();
        let group =
            crate::form::element_group_to_form(&out[0], &out[1..].iter().collect::<Vec<_>>(), cx);
        assert_eq!(form_to_elements(&group, cx).unwrap(), out, "unchanged");

        // the class renamed (as a copy is: "Warden (copy)"): the section is counted under the new name, and so are
        // the choices it gave under the old one
        let mut renamed = group.clone();
        renamed.fields.insert("name".into(), json!("Warden (copy)"));
        let written = form_to_elements(&renamed, cx).expect("writes");
        let f = spellcasting(&written);
        assert_eq!(f["spellcasting"]["name"], json!("Warden (copy)"));
        let selects: Vec<(String, String)> = rules_of(&f, "select")
            .into_iter()
            .filter(|r| r["name"].as_str().unwrap().starts_with("Spell"))
            .map(|r| {
                (
                    r["name"].as_str().unwrap().to_string(),
                    r["spellcasting"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        assert_eq!(
            selects,
            [
                (
                    "Spellbook (Warden (copy) 1)".to_string(),
                    "Warden (copy)".to_string()
                ),
                (
                    "Spellbook (Warden (copy))".to_string(),
                    "Warden (copy)".to_string()
                ),
                ("Spell (Sage)".to_string(), "Sage".to_string()),
            ],
            "another casting's choice is left alone"
        );
    });
}
