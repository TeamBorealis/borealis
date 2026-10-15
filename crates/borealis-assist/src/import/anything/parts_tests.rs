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

mod claimed {
    use super::super::section_lines;
    use crate::import::anything::fidelity::tests::{draft, planned};
    use crate::import::anything::types::Role;

    fn text(id: &str, name: &str, line: usize, end: usize) -> super::PlannedElement {
        let mut p = planned(id, name, "Rules Glossary Entry", Role::Text, Some(line));
        p.excerpt.as_mut().unwrap().end_line = end;
        p
    }

    #[test]
    fn a_child_is_left_out_of_its_parent_by_the_range_it_prints_itself() {
        // The child's located passage reaches past its own heading's section (a flat sub-section
        // it prints): the parent leaves all of it out, so nothing is printed twice.
        let src = "## Lamp Lore\n\nLamps are old.\n\n### Wicks\n\nWicks are cotton.\n\n\
### Wick Care\n\nTrim them daily.\n\n### Oils\n\nOil burns slow.\n";
        let parent = text("t0", "Lamp Lore", 1, 16);
        let wicks = text("t1", "Wicks", 5, 12);
        let d = draft(src, vec![parent, wicks], &[]);
        let own = section_lines(src, &d.plan.elements[1], &d.plan).join("\n");
        assert!(own.contains("Trim them daily."), "{own}");
        let parent = section_lines(src, &d.plan.elements[0], &d.plan).join("\n");
        assert!(!parent.contains("Trim them daily."), "{parent}");
        assert!(parent.contains("Oil burns slow.") && parent.contains("Lamps are old."));
        // A generated (headed) child that was generated from a passage past its heading's
        // section claims that passage too.
        let mut item = planned("i1", "Wicks", "Item", Role::Standalone, Some(5));
        item.excerpt.as_mut().unwrap().end_line = 12;
        let d = draft(src, vec![text("t0", "Lamp Lore", 1, 16), item], &[]);
        let parent = section_lines(src, &d.plan.elements[0], &d.plan).join("\n");
        assert!(!parent.contains("Trim them daily."), "{parent}");
    }

    #[test]
    fn a_section_without_a_heading_of_its_own_stops_at_the_next_heading() {
        // A name printed across a line and a heading ("Bonus Fire, or" / "#### Cold Damage"):
        // the entry is its located lines, not the unplanned sections after it.
        let src = "## Charms\n\n### Tier 1\n\nBonus Fire, or\n\n#### Cold Damage\n\nCost: 50\n\n\
#### Charm Ward\n\nCost: 100\n\n### Tier 2\n\n#### Great Ward\n\nCost: 200\n";
        let parent = text("t0", "Charms", 1, 20);
        let child = text("t1", "Bonus Fire, or Cold Damage", 5, 9);
        let ward = text("t2", "Great Ward", 17, 20);
        let d = draft(src, vec![parent, child, ward], &[]);
        let own = section_lines(src, &d.plan.elements[1], &d.plan).join("\n");
        assert!(
            own.contains("Cost: 50") && !own.contains("Charm Ward"),
            "{own}"
        );
        // The parent keeps the unplanned "Charm Ward", but no heading left with nothing under
        // it ("Tier 2" holds only a planned child).
        let parent = section_lines(src, &d.plan.elements[0], &d.plan);
        assert!(parent.contains(&"Cost: 100"), "{parent:?}");
        assert!(!parent.contains(&"### Tier 2"), "{parent:?}");
    }

    #[test]
    fn a_text_section_stops_at_a_narrative_or_end_matter_heading_of_its_level() {
        // A flattened sub-rule continues the section; a narrative section the detection named
        // (never imported) and end matter do not.
        let src = "## Ferry Rules\n\nThe ferry costs a copper.\n\n## Night Crossings\n\n\
Double fare after dark.\n\n## The Drowned Mill\n\nRead aloud: the wheel creaks in the fog.\n\n\
## Thanks\n\nTo the river folk.\n";
        let rules = || text("t0", "Ferry Rules", 1, 3);
        let mut d = draft(src, vec![rules()], &[]);
        let all = section_lines(src, &d.plan.elements[0], &d.plan).join("\n");
        assert!(all.contains("Double fare after dark."), "{all}");
        assert!(!all.contains("To the river folk."), "{all}");
        d.plan.notes.push(format!(
            "{}The Drowned Mill",
            super::super::super::plan::NARRATIVE_NOTE
        ));
        let own = section_lines(src, &d.plan.elements[0], &d.plan).join("\n");
        assert!(own.contains("Double fare after dark."), "{own}");
        assert!(!own.contains("the wheel creaks"), "{own}");
        // Without the narrative note the unplanned heading is read as a flattened sub-rule.
        assert!(all.contains("the wheel creaks"), "{all}");
    }
}
