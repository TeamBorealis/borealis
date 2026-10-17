//! Text fixes on invented sources: options of a rule printed as table rows, the rule's pick,
//! the legal notice left out of a text (replayed through `finalize_draft`, no model).

use borealis_content::{ContentStore, ContentStoreBuilder};
use serde_json::{json, Value};

use super::super::finalize_draft;
use super::super::intake::{attach_option_tables, option_tables};
use super::super::plan::{rebuild, text_group};
use super::super::types::{Edition, ImportDraft, ImportPlan, OtherItem, PlanGroup, Role};
use super::*;

fn library_builder() -> ContentStoreBuilder {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .expect("core loads")
}

fn library() -> ContentStore {
    library_builder().build()
}

/// A rule paragraph after a trademark notice, then a table of options: two print only a name,
/// one prints an effect.
const STANCES: &str = "# Duel Stances\n\n\
Wyrmforge and Emberline are trademarks of Example Press. All rights reserved.\n\n\
A duelist learns two stances from the table below.\n\n\
| Stance | Effect |\n|---|---|\n\
| Iron Guard | |\n| Swift Reed | |\n| Ember Step | You move 5 feet after a hit. |\n\n\
# Afterword\n\nThanks for reading.\n";

fn rows() -> Vec<OtherItem> {
    ["Iron Guard", "Swift Reed", "Ember Step"]
        .iter()
        .map(|n| OtherItem {
            name: n.to_string(),
            what: "table row".into(),
            note: String::new(),
        })
        .collect()
}

fn plan() -> ImportPlan {
    let rows = rows();
    let mut groups: Vec<PlanGroup> = rows
        .iter()
        .map(|o| text_group(&o.name, &o.what, ""))
        .collect();
    let tables = option_tables(&rows, STANCES);
    assert_eq!(tables.len(), 1, "a pick count makes plain rows options");
    assert_eq!(tables[0].count, Some(2));
    attach_option_tables(&mut groups, &tables);
    let mut plan = ImportPlan {
        edition: Edition::Y2014,
        groups,
        elements: Vec::new(),
        notes: Vec::new(),
    };
    rebuild(&mut plan, &library(), "HB", STANCES);
    plan
}

/// What a model might write: every option described by the whole table, invented rules on the
/// name-only ones.
fn draft() -> ImportDraft {
    let plan = plan();
    let whole = "<p>Wyrmforge and Emberline are trademarks of Example Press. All rights reserved.</p><p>A duelist learns two stances from the table below.</p><table><tr><td>Iron Guard</td></tr></table>";
    let elements: Vec<Value> = plan
        .elements
        .iter()
        .map(|p| {
            let rules = if p.role == Role::Option {
                json!([{"kind": "stat", "name": "ac", "value": "1"}])
            } else {
                json!([])
            };
            json!({"id": p.id, "name": p.name, "ty": p.ty, "role": p.role, "level": null,
                "json": {"id": p.id, "name": p.name, "type": p.ty, "source": "T",
                    "description": whole, "rules": rules},
                "status": "ok", "issues": [], "excerpt": null, "balance": null,
                "repair_rounds": 0, "revisions": 0})
        })
        .collect();
    serde_json::from_value(json!({
        "edition": "2014", "id_prefix": "HB", "source_name": "T", "pack_id": "t",
        "pack_name": "T", "source_kind": "text", "source_text": STANCES,
        "detection": {"kind": "rules", "edition": "2014", "confidence": 1.0, "items": [],
            "other": rows().iter().map(|o| json!({"name": o.name, "what": o.what})).collect::<Vec<_>>()},
        "plan": plan, "elements": elements, "sweep_issues": []}))
    .expect("draft")
}

fn description(e: &borealis_content::Element) -> String {
    serde_json::to_value(e).unwrap()["description"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

#[test]
fn each_option_is_its_own_row_and_the_rule_is_the_pick_of_two() {
    let done = finalize_draft(&library_builder(), &draft()).expect("finalizes");
    let file = &done.pack.content;
    let by_name = |n: &str| {
        file.elements
            .iter()
            .find(|e| e.name == n)
            .unwrap_or_else(|| panic!("{n} in the pack"))
    };
    let guard = by_name("Iron Guard");
    assert_eq!(
        description(guard),
        "<table><tr><th>Stance</th><th>Effect</th></tr><tr><td>Iron Guard</td><td></td></tr></table>"
    );
    assert!(guard.rules.is_empty(), "a name-only row has no rules");
    let step = by_name("Ember Step");
    assert!(description(step).contains("You move 5 feet after a hit."));
    assert!(!description(step).contains("Iron Guard"));
    assert_eq!(
        step.rules.len(),
        1,
        "a row with effect text keeps its rules"
    );
    let rule = by_name("Duel Stances");
    let rules = serde_json::to_value(&rule.rules).unwrap();
    assert_eq!(rules[0]["kind"], "select");
    assert_eq!(rules[0]["number"], 2);
    assert_eq!(rules[0]["supports"], "Duel Stances");
    assert!(!description(rule).contains("trademark"));
    assert!(description(rule).contains("learns two stances"));
}

#[test]
fn a_legal_notice_is_left_out_unless_it_is_the_whole_text() {
    let mut d = draft();
    let notice = "<p>All rights reserved by Example Press.</p>";
    let e = &mut d.elements[0];
    e.name = "Open License".into();
    e.json["description"] = json!(notice);
    strip_legal(e);
    assert_eq!(e.json["description"], notice, "end matter keeps its notice");
    e.name = "Plain Rule".into();
    strip_legal(e);
    assert_eq!(e.json["description"], notice, "nothing else to keep");
    e.json["description"] = json!(format!("<p>Roll twice.</p>{notice}"));
    strip_legal(e);
    assert_eq!(e.json["description"], "<p>Roll twice.</p>");
    assert!(e.normalised.last().unwrap().contains("legal"));
}

#[test]
fn a_notice_printed_in_one_paragraph_with_the_intro_leaves_only_the_intro() {
    let mut d = draft();
    let e = &mut d.elements[0];
    e.name = "Welcome".into();
    e.json["description"] = json!(
        "<p><strong>Welcome</strong></p><p>Writing and layout by A. Penn. Wyrmforge is a \
         trademark of Example Press. All rights reserved. This book adds <em>new rules.</em> \
         Use them with care.</p>"
    );
    strip_legal(e);
    assert_eq!(
        e.json["description"],
        "<p><strong>Welcome</strong></p><p>This book adds <em>new rules.</em> Use them with care.</p>"
    );
    // A rules sentence before the notice stays; a paragraph of the notice alone goes.
    e.json["description"] = json!(
        "<p>Roll twice. All rights reserved.</p><p>\u{a9} 2020 Example Press.</p><p>Then rest.</p>"
    );
    strip_legal(e);
    assert_eq!(e.json["description"], "<p>Roll twice.</p><p>Then rest.</p>");
}

/// Three text entries printed as the rows of one table (no pick: not options).
const OMENS: &str = "# Omens\n\nThe sky shows one of these.\n\n\
| Omen | Meaning |\n|---|---|\n| Red Moon | War comes. |\n| Pale Sun | A long winter. |\n\
| Black Star | A death in the family. |\n\nOmens are read at dusk.\n\n# Afterword\n\nThanks.\n";

#[test]
fn text_entries_that_are_rows_of_one_table_carry_their_own_row_only() {
    let rows: Vec<OtherItem> = ["Red Moon", "Pale Sun", "Black Star"]
        .iter()
        .map(|n| OtherItem {
            name: n.to_string(),
            what: "table row".into(),
            note: String::new(),
        })
        .collect();
    assert!(option_tables(&rows, OMENS).is_empty());
    let mut plan = ImportPlan {
        edition: Edition::Y2014,
        groups: rows
            .iter()
            .map(|o| text_group(&o.name, &o.what, ""))
            .collect(),
        elements: Vec::new(),
        notes: Vec::new(),
    };
    rebuild(&mut plan, &library(), "HB", OMENS);
    let whole = "<table><tr><td>Red Moon</td></tr></table><p>Omens are read at dusk.</p>";
    let elements: Vec<Value> = plan
        .elements
        .iter()
        .map(|p| {
            json!({"id": p.id, "name": p.name, "ty": p.ty, "role": p.role, "level": null,
                "json": {"description": whole}, "status": "ok", "issues": [],
                "excerpt": null, "balance": null, "repair_rounds": 0, "revisions": 0})
        })
        .collect();
    let draft: ImportDraft = serde_json::from_value(json!({
        "edition": "2014", "id_prefix": "HB", "source_name": "T", "pack_id": "t",
        "pack_name": "T", "source_kind": "text", "source_text": OMENS,
        "detection": {"kind": "rules", "edition": "2014", "confidence": 1.0, "items": []},
        "plan": plan, "elements": elements, "sweep_issues": []}))
    .expect("draft");
    let fixed = prepare(&draft);
    let pale = fixed
        .elements
        .iter()
        .find(|e| e.name == "Pale Sun")
        .expect("planned");
    assert_eq!(
        pale.json["description"],
        "<table><tr><th>Omen</th><th>Meaning</th></tr><tr><td>Pale Sun</td><td>A long winter.</td></tr></table>"
    );
}
