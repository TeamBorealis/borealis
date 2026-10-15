use std::sync::atomic::{AtomicUsize, Ordering};

use borealis_ai::task::{AiRunner, BoxFuture, RunOptions};
use borealis_ai::{AiError, AiEvent, AiOutcome, AiRequest};
use serde_json::{json, Value};

use super::*;
use crate::import::anything::fidelity::tests::{draft, planned};
use crate::import::anything::types::{PlannedElement, SourceExcerpt};

/// Answers every call with `answer` (a refusal when `None`) and records the prompts.
struct Fake {
    answer: Option<Value>,
    calls: AtomicUsize,
    prompts: std::sync::Mutex<Vec<String>>,
}

impl Fake {
    fn new(answer: Option<Value>) -> Self {
        Self {
            answer,
            calls: 0.into(),
            prompts: Vec::new().into(),
        }
    }
}

impl AiRunner for Fake {
    fn run<'a>(
        &'a self,
        req: &'a AiRequest,
        _sink: Option<&'a tokio::sync::mpsc::UnboundedSender<AiEvent>>,
    ) -> BoxFuture<'a, Result<AiOutcome, AiError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.prompts.lock().unwrap().push(req.prompt.clone());
        let answer = self.answer.clone();
        Box::pin(async move {
            match answer {
                Some(el) => Ok(AiOutcome {
                    text: json!({"element": el, "notes": []}).to_string(),
                    ..Default::default()
                }),
                None => Err(AiError::Invalid("the import's call budget is spent".into())),
            }
        })
    }
}

impl VisionRunner for Fake {}

fn located(mut p: PlannedElement, start: usize, end: usize) -> PlannedElement {
    p.excerpt = Some(SourceExcerpt {
        start_line: start,
        end_line: end,
        snippet: String::new(),
    });
    p
}

fn core() -> ContentStoreBuilder {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .expect("core loads")
}

/// Two feats; both carry damage dice the source never prints, the first one's passage is
/// stale (it points at the heading only).
fn two_feats() -> ImportDraft {
    let src = "## Feats\n\n### Cinder Step\n\nWhen you hit a creature, it takes an extra 1d8 fire damage.\n\n\
### Ash Ward\n\nYou gain a +2 bonus to your Armor Class while you stand in smoke.\n";
    let a = located(
        planned(
            "ID_T_FEAT_CINDER_STEP",
            "Cinder Step",
            "Feat",
            Role::Standalone,
            None,
        ),
        1,
        2,
    );
    let b = located(
        planned(
            "ID_T_FEAT_ASH_WARD",
            "Ash Ward",
            "Feat",
            Role::Standalone,
            None,
        ),
        7,
        9,
    );
    let mut d = draft(
        src,
        vec![a, b],
        &[
            "<p>When you hit a creature, it takes an extra 1d8 fire damage.</p>",
            "<p>You gain a +2 bonus to your Armor Class while you stand in smoke.</p>",
        ],
    );
    for e in &mut d.elements {
        e.json["source"] = json!("T");
        e.json["rules"] = json!([{"kind": "extra_damage", "dice": "9d9", "type": "fire"}]);
    }
    d
}

fn fixed_cinder() -> Value {
    json!({"id": "ID_T_FEAT_CINDER_STEP", "name": "Cinder Step", "type": "Feat", "source": "T",
        "description": "<p>When you hit a creature, it takes an extra 1d8 fire damage.</p>",
        "rules": [{"kind": "extra_damage", "dice": "1d8", "type": "fire"}]})
}

async fn regenerate(
    runner: &Fake,
    d: &ImportDraft,
    range: Option<(usize, usize)>,
) -> Result<ImportDraft, AssistError> {
    regenerate_element(
        runner,
        &core(),
        d,
        "ID_T_FEAT_CINDER_STEP",
        range,
        &RunOptions::default(),
        None,
    )
    .await
}

#[tokio::test]
async fn a_range_rebuilds_one_element_from_its_passage_with_one_call() {
    // Arrange
    let d = two_feats();
    let runner = Fake::new(Some(fixed_cinder()));
    // Act
    let next = regenerate(&runner, &d, Some((3, 5))).await.unwrap();
    // Assert: one generation call (the other failing feat is not touched), from the new lines,
    // and not as a revision of the previous JSON.
    assert_eq!(runner.calls.load(Ordering::SeqCst), 1);
    let prompt = runner.prompts.lock().unwrap()[0].clone();
    assert!(prompt.contains("extra 1d8 fire damage"), "{prompt}");
    assert!(!prompt.contains("previous_output"), "{prompt}");
    let e = &next.elements[0];
    assert_eq!(e.json["rules"][0]["dice"], "1d8");
    assert_eq!(
        e.excerpt.as_ref().map(|x| (x.start_line, x.end_line)),
        Some((3, 5))
    );
    assert_eq!(
        next.plan.elements[0].excerpt.as_ref().unwrap().start_line,
        3
    );
    assert_eq!(e.revisions, 1);
    assert!(e.checks.iter().all(|c| !c.failed), "{:?}", e.checks);
    assert_eq!(next.elements[1].json["rules"][0]["dice"], "9d9");
}

#[tokio::test]
async fn no_range_regenerates_from_the_current_passage() {
    let mut d = two_feats();
    d.plan.elements[0].excerpt.as_mut().unwrap().start_line = 3;
    d.plan.elements[0].excerpt.as_mut().unwrap().end_line = 5;
    let runner = Fake::new(Some(fixed_cinder()));
    let next = regenerate(&runner, &d, None).await.unwrap();
    assert_eq!(next.elements[0].json["rules"][0]["dice"], "1d8");
    assert_eq!(runner.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn an_answer_that_fails_a_check_gets_one_repair_for_that_element_only() {
    // Arrange: the model answers with dice the passage does not print.
    let d = two_feats();
    let mut wrong = fixed_cinder();
    wrong["rules"][0]["dice"] = json!("4d4");
    let runner = Fake::new(Some(wrong));
    // Act
    let next = regenerate(&runner, &d, Some((3, 5))).await.unwrap();
    // Assert: generation plus the source-check repair; the second feat (also failing) is left.
    assert_eq!(runner.calls.load(Ordering::SeqCst), 2);
    assert_eq!(next.elements[1].json["rules"][0]["dice"], "9d9");
}

#[tokio::test]
async fn a_failed_call_returns_the_error_and_leaves_the_draft_alone() {
    let d = two_feats();
    let runner = Fake::new(None);
    let err = regenerate(&runner, &d, Some((3, 5))).await.unwrap_err();
    assert!(err.to_string().contains("budget"), "{err}");
    assert_eq!(d, two_feats());
}

#[tokio::test]
async fn a_range_outside_the_source_or_the_generated_gate_is_refused_without_a_call() {
    let mut d = two_feats();
    let runner = Fake::new(Some(fixed_cinder()));
    assert!(regenerate(&runner, &d, Some((0, 2))).await.is_err());
    assert!(regenerate(&runner, &d, Some((4, 99))).await.is_err());
    assert!(regenerate(&runner, &d, Some((5, 4))).await.is_err());
    d.elements[0].role = Role::SubclassGate;
    assert!(regenerate(&runner, &d, None).await.is_err());
    assert_eq!(runner.calls.load(Ordering::SeqCst), 0);
}
