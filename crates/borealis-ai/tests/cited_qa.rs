//! Retrieval + cited Q&A against the real bundled content packs (no CLI is called).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use borealis_ai::retrieval::{Filters, LazyIndex};
use borealis_ai::task::cited_qa::{answer_question, DEFAULT_TOP_K};
use borealis_ai::task::qa::Confidence;
use borealis_ai::task::{AiRunner, BoxFuture, Ruleset as AiRuleset, RunOptions};
use borealis_ai::{AiError, AiEvent, AiOutcome, AiRequest};
use borealis_content::{ContentStore, ContentStoreBuilder, Ruleset};
use tokio::sync::mpsc;

const GRAPPLED_2024: &str = "ID_2024_CONDITION_GRAPPLED";

fn content_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

fn store() -> Arc<ContentStore> {
    let root = content_root();
    let mut b = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        b = b.add_pack_dir(&root.join(pack)).unwrap();
    }
    Arc::new(b.build())
}

fn index() -> LazyIndex {
    LazyIndex::new(store())
}

fn filters(ruleset: Ruleset) -> Filters {
    Filters {
        ruleset: Some(ruleset),
        ..Filters::default()
    }
}

/// Replies with one fixed text and records the prompts it saw.
struct FixedRunner {
    reply: String,
    prompts: Mutex<Vec<String>>,
}

impl FixedRunner {
    fn new(reply: &str) -> Self {
        Self {
            reply: reply.into(),
            prompts: Mutex::new(Vec::new()),
        }
    }
}

impl AiRunner for FixedRunner {
    fn run<'a>(
        &'a self,
        req: &'a AiRequest,
        _sink: Option<&'a mpsc::UnboundedSender<AiEvent>>,
    ) -> BoxFuture<'a, Result<AiOutcome, AiError>> {
        self.prompts.lock().unwrap().push(req.prompt.clone());
        let text = self.reply.clone();
        Box::pin(async move {
            Ok(AiOutcome {
                text,
                ..AiOutcome::default()
            })
        })
    }
}

#[test]
fn grappled_condition_ranks_first() {
    let hits = index().retrieve("grappled condition", 5, &filters(Ruleset::Dnd2024));
    assert_eq!(hits[0].id, GRAPPLED_2024, "{hits:#?}");
    assert_eq!(hits[0].ty, "Condition");
    assert!(!hits[0].snippet.contains('<'));
}

#[test]
fn ruleset_filter_excludes_other_edition() {
    let idx = index();
    let all = idx.retrieve("fireball", 20, &Filters::default());
    assert!(all.iter().any(|h| h.ruleset == Some(Ruleset::Dnd2014)));
    assert!(all.iter().any(|h| h.ruleset == Some(Ruleset::Dnd2024)));
    for rs in [Ruleset::Dnd2014, Ruleset::Dnd2024] {
        let hits = idx.retrieve("fireball", 20, &filters(rs));
        assert!(!hits.is_empty());
        assert!(hits
            .iter()
            .all(|h| h.ruleset.is_none_or(|r| r.compatible_with(rs))));
    }
    let h2014 = idx.retrieve("grappled condition", 20, &filters(Ruleset::Dnd2014));
    assert!(h2014.iter().all(|h| h.id != GRAPPLED_2024));
}

#[test]
fn enabled_packs_filter_limits_results() {
    let idx = index();
    let f = Filters {
        enabled_packs: Some(["srd-5.1".to_string()].into()),
        ..Filters::default()
    };
    let hits = idx.retrieve("fireball", 20, &f);
    assert!(!hits.is_empty());
    assert!(hits.iter().all(|h| h.source == "SRD 5.1"), "{hits:#?}");
}

#[test]
fn gibberish_query_returns_nothing() {
    assert!(index()
        .retrieve("zxqvbn wlkjh", 5, &Filters::default())
        .is_empty());
}

#[tokio::test]
async fn hallucinated_citation_is_stripped_end_to_end() {
    let reply = format!(
        r#"{{"answer_markdown":"A grappled creature has Speed 0 [[{GRAPPLED_2024}]] and can fly [[ID_INVENTED]].","citations":["{GRAPPLED_2024}","ID_INVENTED"],"confidence":"high","not_covered":false}}"#
    );
    let runner = FixedRunner::new(&reply);
    let r = answer_question(
        &runner,
        &index(),
        "What does the grappled condition do?",
        AiRuleset::Dnd2024,
        None,
        &filters(Ruleset::Dnd2024),
        DEFAULT_TOP_K,
        &RunOptions::default(),
    )
    .await
    .unwrap();
    let a = r.output;
    assert_eq!(a.citations, vec![GRAPPLED_2024]);
    assert_eq!(a.stripped_citations, vec!["ID_INVENTED"]);
    assert!(!a.answer_markdown.contains("ID_INVENTED"));
    assert_eq!(a.confidence, Confidence::Medium);
    let prompt = runner.prompts.lock().unwrap()[0].clone();
    assert!(prompt.contains(&format!("[[{GRAPPLED_2024}]] Grappled")));
}

#[tokio::test]
async fn no_hits_skips_the_cli_and_reports_not_covered() {
    let runner = FixedRunner::new("unused");
    let r = answer_question(
        &runner,
        &index(),
        "zxqvbn wlkjh",
        AiRuleset::Dnd2014,
        None,
        &Filters::default(),
        DEFAULT_TOP_K,
        &RunOptions::default(),
    )
    .await
    .unwrap();
    assert!(r.output.not_covered);
    assert_eq!(r.attempts, 0);
    assert!(runner.prompts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn model_declared_not_covered_is_kept() {
    let runner = FixedRunner::new(
        r#"{"answer_markdown":"That is not covered by your enabled content.","citations":[],"confidence":"low","not_covered":true}"#,
    );
    let r = answer_question(
        &runner,
        &index(),
        "grappled condition",
        AiRuleset::Dnd2024,
        None,
        &filters(Ruleset::Dnd2024),
        3,
        &RunOptions::default(),
    )
    .await
    .unwrap();
    assert!(r.output.not_covered && r.output.citations.is_empty());
}
