use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use borealis_ai::error::TimeoutKind;
use borealis_ai::task::{AiRunner, BoxFuture, RunOptions};
use borealis_ai::{AiError, AiEvent, AiOutcome, AiRequest};
use serde_json::{json, Value};

use super::*;
use crate::import::anything::generate::fill_pending;
use crate::import::anything::types::{
    ElementStatus, ImportDraft, PlannedElement, Role, SourceExcerpt,
};

/// How the fake model fails a call it sees (`size` = elements asked for, `n` = call number).
type Rule = Box<dyn Fn(usize, usize) -> Option<AiError> + Send + Sync>;

/// A fake model: answers every requested id with a valid feat, unless `fail` says otherwise;
/// `omit_last` drops the last element of a batch answer (a partial answer).
struct Fake {
    fail: Rule,
    omit_last: bool,
    calls: AtomicUsize,
    sizes: std::sync::Mutex<Vec<usize>>,
}

impl Fake {
    fn new(fail: impl Fn(usize, usize) -> Option<AiError> + Send + Sync + 'static) -> Self {
        Self {
            fail: Box::new(fail),
            omit_last: false,
            calls: 0.into(),
            sizes: Vec::new().into(),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

/// The ids a prompt asks for: `- id "X":` lines of a batch, else `Use EXACTLY id "X"`.
fn requested(prompt: &str) -> (bool, Vec<String>) {
    let quoted = |rest: &str| rest.split('"').next().unwrap_or("").to_string();
    let batch: Vec<String> = prompt
        .lines()
        .filter_map(|l| l.strip_prefix("- id \""))
        .map(quoted)
        .collect();
    if !batch.is_empty() {
        return (true, batch);
    }
    let single = prompt
        .split("Use EXACTLY id \"")
        .nth(1)
        .map(quoted)
        .into_iter()
        .collect();
    (false, single)
}

fn feat_json(id: &str) -> Value {
    let name = id.trim_start_matches("ID_T_FEAT_").replace('_', " ");
    json!({"id": id, "name": name, "type": "Feat", "source": "T",
        "description": "<p>You hum a tune of brass.</p>", "rules": []})
}

impl AiRunner for Fake {
    fn run<'a>(
        &'a self,
        req: &'a AiRequest,
        _sink: Option<&'a tokio::sync::mpsc::UnboundedSender<AiEvent>>,
    ) -> BoxFuture<'a, Result<AiOutcome, AiError>> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        let (batch, ids) = requested(&req.prompt);
        self.sizes.lock().unwrap().push(ids.len());
        let failed = (self.fail)(ids.len(), n);
        let text = if batch {
            let keep = if self.omit_last && ids.len() > 1 {
                ids.len() - 1
            } else {
                ids.len()
            };
            let els: Vec<Value> = ids[..keep]
                .iter()
                .map(|id| json!({"element": feat_json(id), "notes": []}))
                .collect();
            json!({ "elements": els }).to_string()
        } else {
            json!({"element": feat_json(&ids[0]), "notes": []}).to_string()
        };
        Box::pin(async move {
            match failed {
                Some(e) => Err(e),
                None => Ok(AiOutcome {
                    text,
                    ..Default::default()
                }),
            }
        })
    }
}

fn timeout() -> AiError {
    AiError::Timeout {
        kind: TimeoutKind::Overall,
        after: Duration::from_secs(300),
    }
}

fn cli_error() -> AiError {
    AiError::Cli {
        message: "connection reset".into(),
        stderr_tail: String::new(),
        exit_code: Some(1),
    }
}

fn planned(i: usize) -> PlannedElement {
    PlannedElement {
        id: format!("ID_T_FEAT_BRASS_{i}"),
        ty: "Feat".into(),
        name: format!("Brass {i}"),
        role: Role::Standalone,
        level: None,
        owner: None,
        brief: String::new(),
        excerpt: Some(SourceExcerpt {
            start_line: i + 1,
            end_line: i + 1,
            snippet: String::new(),
        }),
        parent: None,
        picks: None,
        prerequisite_level: None,
        spells: Vec::new(),
    }
}

fn draft(n: usize) -> ImportDraft {
    let source: String = (0..n)
        .map(|i| format!("Brass {i}. You hum a tune of brass.\n"))
        .collect();
    let mut d: ImportDraft = serde_json::from_value(json!({"edition": "2024",
        "id_prefix": "ID_T", "source_name": "T", "pack_id": "t", "pack_name": "T",
        "source_kind": "text", "source_text": source,
        "detection": {"kind": "feat", "edition": "2024", "confidence": 1.0, "items": []},
        "plan": {"edition": "2024", "groups": [], "elements": [], "notes": []},
        "elements": [], "sweep_issues": []}))
    .unwrap();
    d.plan.elements = (0..n).map(planned).collect();
    d.elements = crate::import::anything::generate::pending(&d.plan);
    d
}

async fn generate(runner: &Fake, d: &mut ImportDraft) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let base = borealis_content::ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .unwrap();
    let ctx = crate::homebrew::PackContext {
        base,
        ruleset: d.edition.ruleset(),
    };
    let opts = RunOptions {
        max_repairs: 0,
        sink: None,
    };
    fill_pending(runner, &ctx, d, &opts, None).await;
}

fn failed(d: &ImportDraft) -> Vec<&str> {
    d.elements
        .iter()
        .filter(|e| e.status != ElementStatus::Ok)
        .map(|e| e.name.as_str())
        .collect()
}

fn retried(d: &ImportDraft) -> usize {
    d.elements
        .iter()
        .filter(|e| {
            e.normalised
                .iter()
                .any(|n| n.starts_with("generation retried"))
        })
        .count()
}

#[tokio::test]
async fn a_timed_out_batch_is_retried_in_halves_and_nothing_fails() {
    // Arrange: every call asking for more than 2 elements times out.
    let runner = Fake::new(|size, _| (size > 2).then(timeout));
    let mut d = draft(4);
    // Act
    generate(&runner, &mut d).await;
    // Assert: 4 -> timeout, then 2 + 2.
    assert_eq!(failed(&d), Vec::<&str>::new());
    assert_eq!(*runner.sizes.lock().unwrap(), vec![4, 2, 2]);
    assert_eq!(retried(&d), 4, "every element says it was retried");
}

#[tokio::test]
async fn a_transport_error_retries_the_same_batch_once() {
    let runner = Fake::new(|_, n| (n == 1).then(cli_error));
    let mut d = draft(3);
    generate(&runner, &mut d).await;
    assert_eq!(failed(&d), Vec::<&str>::new());
    assert_eq!(*runner.sizes.lock().unwrap(), vec![3, 3]);
}

#[tokio::test]
async fn a_partial_answer_is_split_down_to_single_elements() {
    // Every batch answer omits its last element; a single-element call answers.
    let mut runner = Fake::new(|_, _| None);
    runner.omit_last = true;
    let mut d = draft(4);
    generate(&runner, &mut d).await;
    assert_eq!(failed(&d), Vec::<&str>::new());
    // 4 (partial) -> 2 (partial) -> two single calls -> 2 (partial) -> two single calls.
    assert_eq!(*runner.sizes.lock().unwrap(), vec![4, 2, 2, 1, 1, 1, 1]);
}

#[tokio::test]
async fn a_single_element_call_is_retried_once_after_a_timeout() {
    // A lone feat is generated on the single-element path: its first call times out.
    let runner = Fake::new(|_, n| (n == 1).then(timeout));
    let mut d = draft(1);
    generate(&runner, &mut d).await;
    assert_eq!(failed(&d), Vec::<&str>::new());
    assert_eq!(runner.calls(), 2);
    assert_eq!(retried(&d), 1);
}

#[tokio::test]
async fn a_refused_call_is_never_retried() {
    // The import's call budget refuses the call: no retry spends more of it.
    let runner = Fake::new(|_, _| Some(AiError::Invalid("call cap 1 reached".into())));
    let mut d = draft(3);
    generate(&runner, &mut d).await;
    assert_eq!(runner.calls(), 1);
    assert_eq!(failed(&d).len(), 3);
    assert!(d.elements[0].issues[0].contains("call cap"));
}

#[tokio::test]
async fn a_model_that_keeps_failing_stops_the_retries() {
    let runner = Fake::new(|_, _| Some(timeout()));
    let mut d = draft(8);
    generate(&runner, &mut d).await;
    assert_eq!(failed(&d).len(), 8);
    assert!(
        runner.calls() <= MAX_FAILURES_IN_A_ROW as usize + 1,
        "{} calls",
        runner.calls()
    );
    assert!(d.elements.iter().all(|e| e.issues[0].contains("timed out")));
}

#[test]
fn retries_follow_the_kind_of_failure() {
    let mut r = Retries::default();
    assert_eq!(r.after_batch(&timeout(), 8, false), Next::Split);
    r.succeeded();
    assert_eq!(r.after_batch(&cli_error(), 8, false), Next::Again);
    r.succeeded();
    assert_eq!(r.after_batch(&cli_error(), 8, true), Next::Split);
    r.succeeded();
    assert_eq!(r.after_batch(&timeout(), 1, false), Next::Alone);
    r.succeeded();
    assert_eq!(r.after_single(&timeout(), true), Next::GiveUp);
    r.succeeded();
    assert_eq!(r.after_batch(&AiError::Cancelled, 8, false), Next::GiveUp);
}

impl crate::import::anything::steps::VisionRunner for Fake {}

fn session_limit() -> AiError {
    AiError::Cli {
        message: "You've hit your session limit \u{b7} resets 4:10am (Europe/Paris)".into(),
        stderr_tail: String::new(),
        exit_code: Some(1),
    }
}

async fn continue_import(runner: &Fake, d: ImportDraft) -> Result<ImportDraft, String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let base = borealis_content::ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .unwrap();
    let opts = RunOptions {
        max_repairs: 0,
        sink: None,
    };
    crate::import::anything::continue_draft(runner, &base, d, &opts, None)
        .await
        .map(|step| step.draft().clone())
        .map_err(|e| e.to_string())
}

#[tokio::test]
async fn a_session_limit_mid_import_stops_every_call_and_fails_the_import() {
    // Arrange: the first call answers, every later one hits the session limit.
    let runner = Fake::new(|_, n| (n > 1).then(session_limit));
    let d = draft(24);
    // Act
    let result = continue_import(&runner, d).await;
    // Assert: no draft that looks complete, the user's message names the reset, one more call.
    let msg = result.expect_err("the import must fail");
    assert!(
        msg.contains("usage limit") && msg.contains("4:10am"),
        "{msg}"
    );
    assert_eq!(runner.calls(), 2, "no call after the limit");
}

#[tokio::test]
async fn an_ordinary_failure_mid_import_still_never_aborts() {
    // Arrange: the second call fails with a transport error (retried, then passes).
    let runner = Fake::new(|_, n| (n == 2).then(cli_error));
    let d = draft(24);
    // Act
    let result = continue_import(&runner, d).await;
    // Assert
    let d = result.expect("a draft");
    assert_eq!(failed(&d), Vec::<&str>::new());
}

#[test]
fn an_account_block_is_never_retried() {
    let mut r = Retries::default();
    assert_eq!(r.after_batch(&session_limit(), 8, false), Next::GiveUp);
    assert_eq!(r.after_single(&session_limit(), false), Next::GiveUp);
    assert!(refused(&session_limit()));
}
