//! Generation + repair, deterministic balance report, number-checked commentary.

mod common;

use borealis_ai::task::{ContentKind, RunOptions};
use borealis_assist::{homebrew_with_balance, HomebrewRequest, ProseSource, Severity};
use common::{pack_ctx, FakeRunner};
use serde_json::{json, Value};

fn request() -> HomebrewRequest {
    HomebrewRequest {
        kind: ContentKind::Spell,
        description: "A fiery burst".into(),
        name: None,
        id_prefix: Some("HB".into()),
        source_name: "Homebrew".into(),
    }
}

fn gen_reply(id: &str, dice: &str, area: &str, extra_rules: Value) -> String {
    json!({"content": {"format":"borealis-content/1","elements":[{
        "id": id, "name":"Test Burst","type":"Spell","source":"Homebrew","supports":["Wizard"],
        "description": format!("<p>Each creature in a {area} must make a Dexterity saving throw. A creature takes {dice} fire damage on a failed save.</p>"),
        "setters":[{"name":"level","value":"1"},{"name":"school","value":"Evocation"},{"name":"time","value":"1 action"},
                   {"name":"duration","value":"Instantaneous"},{"name":"range","value":"60 feet"}],
        "rules": extra_rules}]},
        "notes":"Burst of fire.","warnings":[]})
    .to_string()
}

#[tokio::test]
async fn overpowered_spell_is_flagged_and_numbers_come_from_the_engine() {
    let runner = FakeRunner::new(&[
        &gen_reply(
            "ID_HB_SPELL_TEST_BURST",
            "10d10",
            "30-foot-radius sphere",
            json!([]),
        ),
        r#"{"commentary":"The damage is far above the SRD ceiling for its level, so lower the dice."}"#,
    ]);
    let out = homebrew_with_balance(&runner, &pack_ctx(), &request(), &RunOptions::default())
        .await
        .unwrap();
    assert_eq!(out.attempts, 1);
    assert_eq!(out.reports[0].max_severity(), Severity::High);
    assert!((out.reports[0].flags[0].value - 55.0).abs() < 1e-9);
    assert!(out.engine_check.passes(), "{:?}", out.engine_check);
    assert_eq!(out.commentary_source, ProseSource::Model);
}

#[tokio::test]
async fn commentary_with_invented_numbers_falls_back_to_the_template() {
    let runner = FakeRunner::new(&[
        &gen_reply(
            "ID_HB_SPELL_TEST_BURST",
            "10d10",
            "30-foot-radius sphere",
            json!([]),
        ),
        r#"{"commentary":"Reduce it to 4d6 and it would be fine."}"#,
    ]);
    let out = homebrew_with_balance(&runner, &pack_ctx(), &request(), &RunOptions::default())
        .await
        .unwrap();
    assert_eq!(out.commentary_source, ProseSource::Template);
    assert!(
        out.commentary.contains("spell.damage_avg is 55"),
        "{}",
        out.commentary
    );
    assert!(!out.commentary.contains("4d6"));
}

#[tokio::test]
async fn balanced_spell_needs_no_commentary_call() {
    let runner = FakeRunner::new(&[&gen_reply(
        "ID_HB_SPELL_TEST_BURST",
        "3d6",
        "15-foot cone",
        json!([]),
    )]);
    let out = homebrew_with_balance(&runner, &pack_ctx(), &request(), &RunOptions::default())
        .await
        .unwrap();
    assert_eq!(runner.calls(), 1);
    assert_eq!(out.reports[0].max_severity(), Severity::Ok);
    assert_eq!(out.commentary_source, ProseSource::Template);
}

#[tokio::test]
async fn dangling_reference_is_repaired_before_the_report() {
    let bad = json!([{"kind":"grant","id":"ID_NOT_A_REAL_THING","type":"Proficiency"}]);
    let runner = FakeRunner::new(&[
        &gen_reply("ID_HB_SPELL_TEST_BURST", "3d6", "15-foot cone", bad),
        &gen_reply("ID_HB_SPELL_TEST_BURST", "3d6", "15-foot cone", json!([])),
    ]);
    let out = homebrew_with_balance(&runner, &pack_ctx(), &request(), &RunOptions::default())
        .await
        .unwrap();
    assert_eq!(out.attempts, 2);
    assert!(runner.prompt(1).contains("ID_NOT_A_REAL_THING"));
    assert!(out.engine_check.dangling_refs.is_empty());
}
