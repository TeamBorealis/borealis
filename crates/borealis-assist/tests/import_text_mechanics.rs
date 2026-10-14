//! A mechanic the model leaves in the text (an armor class formula with no rule) gets exactly
//! one repair; what the repair still leaves as text is a review note, never a failure.
//! Scripted fake provider, no CLI.

mod import_fake;

use borealis_assist::import::anything::{ElementStatus, ImportSource};
use borealis_assist::{import_anything, Edition, ImportDraft, ImportStep};
use import_fake::*;
use serde_json::json;

const SOURCE: &str =
    "# Bark Hide\n\nYour skin hardens like bark. When you aren't wearing armor, your \
Armor Class equals 12 + your Dexterity modifier.\n";

const TEXT: &str =
    "<p>When you aren't wearing armor, your Armor Class equals 12 + your Dexterity modifier.</p>";

fn feat(rules: serde_json::Value) -> String {
    json!({"element": {"id": "X", "name": "Bark Hide", "type": "Feat", "description": TEXT,
        "rules": rules}, "notes": []})
    .to_string()
}

fn ac_rules() -> serde_json::Value {
    json!([
        {"kind": "stat", "name": "ac:bark hide:base", "value": "12"},
        {"kind": "stat", "name": "ac:bark hide:dexterity", "value": "dexterity:modifier"},
        {"kind": "stat", "name": "ac:bark hide", "value": "ac:bark hide:base"},
        {"kind": "stat", "name": "ac:bark hide", "value": "ac:bark hide:dexterity"},
        {"kind": "stat", "name": "ac:calculation", "value": "ac:bark hide", "bonus": "calculation",
         "equipped": "[armor:none]"}
    ])
}

async fn run(repair: String) -> (Fake, ImportDraft) {
    let fake = Fake::default()
        .on(
            "import_detect",
            "",
            &[detect("feat", "Bark Hide", None, "2024")],
        )
        .on("import_generate", "", &[feat(json!([]))])
        .on("import_repair", "", &[repair]);
    let req = request(ImportSource::Text(SOURCE.into()), "tm", Edition::Y2024);
    let draft = match import_anything(&fake, &base(), &req, &opts(), None)
        .await
        .unwrap()
    {
        ImportStep::Draft(d) => *d,
        ImportStep::NeedsChoice(_) => panic!("no choice expected"),
    };
    (fake, draft)
}

#[tokio::test]
async fn a_formula_left_in_the_text_is_repaired_once() {
    let (fake, draft) = run(feat(ac_rules())).await;
    assert_eq!(fake.calls("import_repair"), 1);
    let repair = &fake.prompts("import_repair")[0];
    assert!(repair.contains("no rule implements it"), "{repair}");
    let el = &draft.elements[0];
    assert!(el.json["rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["name"] == "ac:calculation"));
    assert_ne!(el.status, ElementStatus::Failed);
    assert!(!el.issues.iter().any(|i| i.contains("left as text")));
}

#[tokio::test]
async fn a_repair_that_still_writes_no_rule_leaves_a_review_note() {
    let (fake, draft) = run(feat(json!([]))).await;
    assert_eq!(fake.calls("import_repair"), 1, "one repair, never more");
    let el = &draft.elements[0];
    assert_eq!(el.status, ElementStatus::NeedsReview);
    assert!(
        el.issues
            .iter()
            .any(|i| i.starts_with("left as text after a repair")),
        "{:?}",
        el.issues
    );
}
