mod common;

use borealis_ai::task::RunOptions;
use borealis_assist::explain::{explain_number, explain_number_offline, template_text};
use borealis_assist::ProseSource;
use common::{engine, fighter, FakeRunner};
use serde_json::json;

fn reply(text: &str) -> String {
    json!({ "explanation": text }).to_string()
}

#[tokio::test]
async fn hallucinated_number_falls_back_to_the_template() {
    let state = fighter(1, [15, 12, 14, 10, 10, 10]);
    let trace = engine().explain(&state, "ac");
    assert!(!trace.steps.is_empty(), "the engine traces ac");
    let runner = FakeRunner::new(&[&reply(
        "Your armor class is 99 because a mighty shield adds a bonus of 77.",
    )]);

    let out = explain_number(&runner, engine(), &state, "ac", &RunOptions::default()).await;

    assert_eq!(out.source, ProseSource::Template);
    let why = out.fallback_reason.expect("reason");
    assert!(why.contains("77") && why.contains("99"), "{why}");
    assert_eq!(out.text, template_text(&trace));
    assert!(!out.text.contains("99"));
    assert_eq!(out.value, trace.value);
}

#[tokio::test]
async fn prose_that_only_uses_trace_numbers_is_kept() {
    let state = fighter(1, [15, 12, 14, 10, 10, 10]);
    let trace = engine().explain(&state, "ac");
    let prose = format!("Your armor class is {} right now.", trace.value);
    let runner = FakeRunner::new(&[&reply(&prose)]);

    let out = explain_number(&runner, engine(), &state, "ac", &RunOptions::default()).await;

    assert_eq!(out.source, ProseSource::Model);
    assert_eq!(out.text, prose);
    assert!(out.fallback_reason.is_none());
    let sent = runner.prompt(0);
    assert!(sent.contains("final value:"), "{sent}");
}

#[tokio::test]
async fn model_failure_never_breaks_the_explanation() {
    let state = fighter(1, [15, 12, 14, 10, 10, 10]);
    let runner = FakeRunner::failing();
    let out = explain_number(&runner, engine(), &state, "ac", &RunOptions::default()).await;
    assert_eq!(out.source, ProseSource::Template);
    assert!(out.fallback_reason.unwrap().contains("model call failed"));
}

#[tokio::test]
async fn unknown_stat_does_not_call_the_model() {
    let state = fighter(1, [15, 12, 14, 10, 10, 10]);
    let runner = FakeRunner::new(&[]);
    let out = explain_number(
        &runner,
        engine(),
        &state,
        "no-such-stat",
        &RunOptions::default(),
    )
    .await;
    assert_eq!(runner.calls(), 0);
    assert_eq!(out.source, ProseSource::Template);
}

#[test]
fn offline_explanation_is_the_template_without_ai() {
    let state = fighter(1, [15, 12, 14, 10, 10, 10]);
    let out = explain_number_offline(engine(), &state, "ac");
    assert_eq!(out.source, ProseSource::Template);
    assert_eq!(out.text, template_text(&engine().explain(&state, "ac")));
    assert!(out.text.starts_with("ac is "));
}
