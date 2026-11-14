//! IDprw: a class's skill choice is checked by the web mock as the backend checks it. The backend refuses a class
//! whose character chooses skills but is offered none, a count outside 1-18, and fewer skills offered than chosen
//! (`codec/class.rs` `write_skills`); the mock saved such a class. This test writes class forms with each kind of
//! skill choice and the problems the backend's check finds at `fields.skills` to
//! `ui/src/api/__fixtures__/homebrew-class-skills.json`; the UI test `homebrew-class-skills.test.ts` runs the mock's
//! check over the same forms.
//!
//! `BOREALIS_UPDATE_SNAPSHOTS=1` rewrites the file.

use borealis_authoring::expr_build::condition::ConditionContext;
use borealis_authoring::form::{
    check_fields, kind, to_ui, CatalogBlockCodec, FormContext, FormState, NoLookup,
};
use serde_json::{json, Value};
use std::path::Path;

const SKILLS_PATH: &str = "fields.skills";

fn skill(id: &str, name: &str) -> Value {
    json!({"id": id, "name": name, "type": "Proficiency"})
}

/// `(case, the Skills field)`.
fn cases() -> Vec<(&'static str, Value)> {
    let athletics = skill("ID_PROFICIENCY_SKILL_ATHLETICS", "Athletics");
    let survival = skill("ID_PROFICIENCY_SKILL_SURVIVAL", "Survival");
    vec![
        (
            "a new class: chooses two, none offered",
            json!({"count": 2, "skills": []}),
        ),
        ("no skill choice", json!({"count": 0, "skills": []})),
        (
            "two of two",
            json!({"count": 2, "skills": [athletics.clone(), survival.clone()]}),
        ),
        (
            "three of two",
            json!({"count": 3, "skills": [athletics.clone(), survival.clone()]}),
        ),
        (
            "none of two",
            json!({"count": 0, "skills": [athletics.clone(), survival.clone()]}),
        ),
        (
            "nineteen",
            json!({"count": 19, "skills": [athletics.clone(), survival.clone()]}),
        ),
        (
            "an empty pick is not a skill",
            json!({"count": 2, "skills": [athletics, {"id": " ", "name": "", "type": "Proficiency"}]}),
        ),
        (
            "a kept tag filter",
            json!({"count": 2, "skills": [], "tagged": "Skill,Fighter"}),
        ),
        (
            "a kept tag filter and too many",
            json!({"count": 20, "skills": [], "tagged": "Skill"}),
        ),
    ]
}

fn fixture() -> Value {
    let codec = CatalogBlockCodec;
    let cx = FormContext {
        conditions: ConditionContext::new(["Wizard"]),
        lookup: &NoLookup,
        blocks: &codec,
    };
    let class = kind("class").expect("class kind");
    let rows: Vec<Value> = cases()
        .into_iter()
        .map(|(case, skills)| {
            let form: FormState = class
                .new_form("e")
                .with_field("name", json!("Gloom Knight"))
                .with_field("skills", skills);
            let problems: Vec<Value> = check_fields(&form, &cx)
                .into_iter()
                .filter(|p| p.path == SKILLS_PATH)
                .map(|p| json!({"path": p.path, "message": p.message}))
                .collect();
            json!({"case": case, "form": to_ui(&form), "problems": problems})
        })
        .collect();
    json!({ "cases": rows })
}

#[test]
fn the_mock_fixture_holds_the_backend_s_class_skill_problems() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../ui/src/api/__fixtures__/homebrew-class-skills.json");
    let text = serde_json::to_string_pretty(&fixture()).expect("json") + "\n";
    if std::env::var_os("BOREALIS_UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &text).expect("write");
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        on_disk, text,
        "ui/src/api/__fixtures__/homebrew-class-skills.json is stale: run this test with BOREALIS_UPDATE_SNAPSHOTS=1"
    );
}
