//! Rules Q&A with citations to content element ids.
//!
//! The task retrieves the top-k elements from the local BM25 index, asks the model to answer
//! ONLY from those excerpts and to cite ids inline as `[[ID_...]]`, then post-validates the
//! answer: ids that were not retrieved are stripped (and confidence lowered), and sentences
//! without any citation are flagged.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::prompt::{base_system, fence, Ruleset};
use super::qa::Confidence;
use super::{run_task, AiRunner, AiTask, RunOptions, TaskResult};
use crate::retrieval::{Filters, Hit, LazyIndex};
use crate::AiError;

/// Number of excerpts retrieved by default.
pub const DEFAULT_TOP_K: usize = 6;
/// Each excerpt is cut to this many characters in the prompt.
pub const MAX_EXCERPT_CHARS: usize = 1_500;
/// The phrase the model must use when the excerpts do not answer the question.
pub const NOT_COVERED_PHRASE: &str = "not covered by your enabled content";
/// Sentences shorter than this many words are not flagged as uncited claims.
const MIN_CLAIM_WORDS: usize = 5;

/// One retrieved element offered to the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Excerpt {
    /// Element id (citation key).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Element type.
    #[serde(rename = "type")]
    pub ty: String,
    /// Source name.
    pub source: String,
    /// Plain text of the element.
    pub text: String,
}

/// Inputs of [`CitedQa`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CitedQaInput {
    /// The user's question.
    pub question: String,
    /// Active ruleset.
    pub ruleset: Ruleset,
    /// Retrieved elements, best first.
    pub excerpts: Vec<Excerpt>,
    /// Optional short character summary (engine-computed numbers etc.).
    #[serde(default)]
    pub character_summary: Option<String>,
}

/// The model's raw structured answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawCitedAnswer {
    /// Answer in Markdown with inline `[[ID_...]]` citations.
    pub answer_markdown: String,
    /// Ids the model says it relied on.
    #[serde(default)]
    pub citations: Vec<String>,
    /// Model-reported confidence.
    pub confidence: Confidence,
    /// True when the excerpts do not answer the question.
    pub not_covered: bool,
}

/// A validated citation, resolved to its element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CitedSource {
    /// Element id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Element type.
    #[serde(rename = "type")]
    pub ty: String,
}

/// Final answer after post-validation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CitedAnswer {
    /// Answer in Markdown; hallucinated `[[...]]` markers are removed.
    pub answer_markdown: String,
    /// Valid cited ids (declared plus inline), in first-use order.
    pub citations: Vec<String>,
    /// The valid citations resolved to names and types for display.
    pub sources: Vec<CitedSource>,
    /// Confidence after validation (never higher than the model reported).
    pub confidence: Confidence,
    /// True when the enabled content does not cover the question.
    pub not_covered: bool,
    /// Cited ids that were not among the retrieved elements and were stripped.
    pub stripped_citations: Vec<String>,
    /// Sentences that carry no citation.
    pub uncited_claims: Vec<String>,
    /// True when the answer claims coverage but no valid citation remains.
    pub unverified: bool,
}

/// The Q&A task.
#[derive(Debug, Clone)]
pub struct CitedQa {
    /// Inputs.
    pub input: CitedQaInput,
}

impl AiTask for CitedQa {
    type Output = RawCitedAnswer;
    const NAME: &'static str = "rules_qa_cited";

    fn system_prompt(&self) -> String {
        format!(
            "{}\n\nRuleset: {}\n\
Answer ONLY from the excerpts inside <context>; do not use outside knowledge, even if you know the rule.\n\
Cite every claim inline with the id of the excerpt it comes from, written exactly as [[ID_...]] (copy the id from the excerpt header).\n\
Never cite an id that is not in <context>.\n\
If the excerpts do not answer the question, set not_covered to true, confidence to \"low\", and write that the question is \"{}\".\n\
When a <character> summary is given, apply the rule to those numbers but cite only excerpts.\n\
Fields: answer_markdown (string), citations (ids you relied on), confidence (high|medium|low), not_covered (boolean).",
            base_system("answer a rules question strictly from the supplied rule excerpts, with citations"),
            self.input.ruleset.describe(),
            NOT_COVERED_PHRASE,
        )
    }

    fn user_prompt(&self) -> String {
        let i = &self.input;
        let body: String = i
            .excerpts
            .iter()
            .map(|e| {
                let text: String = e.text.chars().take(MAX_EXCERPT_CHARS).collect();
                format!("[[{}]] {} ({}, {})\n{}", e.id, e.name, e.ty, e.source, text)
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let mut p = fence("context", &body);
        p.push_str("\n\n");
        if let Some(c) = &i.character_summary {
            p.push_str(&fence("character", c));
            p.push_str("\n\n");
        }
        p.push_str("Question:\n");
        p.push_str(&fence("user_input", &i.question));
        p
    }

    fn output_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["answer_markdown", "confidence", "not_covered"],
            "properties": {
                "answer_markdown": {"type": "string", "minLength": 1},
                "citations": {"type": "array", "items": {"type": "string"}},
                "confidence": {"enum": ["high", "medium", "low"]},
                "not_covered": {"type": "boolean"}
            }
        })
    }
}

/// Retrieve, ask, and post-validate. With no retrieved elements the CLI is not called and a
/// deterministic "not covered" answer is returned.
///
/// # Errors
/// Propagates [`AiError`] from the runner or from output validation.
#[allow(clippy::too_many_arguments)]
pub async fn answer_question(
    runner: &dyn AiRunner,
    index: &LazyIndex,
    question: &str,
    ruleset: Ruleset,
    character_summary: Option<String>,
    filters: &Filters,
    top_k: usize,
    opts: &RunOptions<'_>,
) -> Result<TaskResult<CitedAnswer>, AiError> {
    let hits = index.retrieve(question, top_k, filters);
    if hits.is_empty() {
        return Ok(TaskResult {
            output: not_covered_answer(),
            prompt_version: super::PROMPT_VERSION,
            attempts: 0,
            usage: None,
        });
    }
    let excerpts = hits
        .iter()
        .map(|h| Excerpt {
            id: h.id.clone(),
            name: h.name.clone(),
            ty: h.ty.clone(),
            source: h.source.clone(),
            text: index
                .index()
                .text(&h.id)
                .map_or_else(|| h.snippet.clone(), str::to_string),
        })
        .collect();
    let task = CitedQa {
        input: CitedQaInput {
            question: question.to_string(),
            ruleset,
            excerpts,
            character_summary,
        },
    };
    let r = run_task(runner, &task, opts).await?;
    Ok(TaskResult {
        output: validate_answer(r.output, &hits),
        prompt_version: r.prompt_version,
        attempts: r.attempts,
        usage: r.usage,
    })
}

/// The deterministic answer used when retrieval finds nothing.
#[must_use]
pub fn not_covered_answer() -> CitedAnswer {
    CitedAnswer {
        answer_markdown: format!("This is {NOT_COVERED_PHRASE}."),
        citations: Vec::new(),
        sources: Vec::new(),
        confidence: Confidence::Low,
        not_covered: true,
        stripped_citations: Vec::new(),
        uncited_claims: Vec::new(),
        unverified: false,
    }
}

fn lower(c: Confidence) -> Confidence {
    match c {
        Confidence::High => Confidence::Medium,
        Confidence::Medium | Confidence::Low => Confidence::Low,
    }
}

/// Post-validate `raw` against the retrieved `hits`.
#[must_use]
pub fn validate_answer(raw: RawCitedAnswer, hits: &[Hit]) -> CitedAnswer {
    let allowed: HashSet<&str> = hits.iter().map(|h| h.id.as_str()).collect();
    if raw.not_covered {
        return CitedAnswer {
            answer_markdown: raw.answer_markdown,
            confidence: Confidence::Low,
            not_covered: true,
            ..not_covered_answer()
        };
    }
    let (text, inline_valid, inline_bad) = rewrite_markers(&raw.answer_markdown, &allowed);
    let mut citations: Vec<String> = Vec::new();
    let mut stripped: Vec<String> = Vec::new();
    for id in inline_bad {
        if !stripped.contains(&id) {
            stripped.push(id);
        }
    }
    for id in inline_valid
        .into_iter()
        .chain(raw.citations.iter().cloned())
    {
        if allowed.contains(id.as_str()) {
            if !citations.contains(&id) {
                citations.push(id);
            }
        } else if !stripped.contains(&id) {
            stripped.push(id);
        }
    }
    let uncited_claims = uncited_sentences(&text);
    let unverified = citations.is_empty();
    let mut confidence = raw.confidence;
    if !stripped.is_empty() {
        confidence = lower(confidence);
    }
    if unverified {
        confidence = Confidence::Low;
    } else if !uncited_claims.is_empty() && confidence == Confidence::High {
        confidence = Confidence::Medium;
    }
    let sources = citations
        .iter()
        .filter_map(|id| hits.iter().find(|h| &h.id == id))
        .map(|h| CitedSource {
            id: h.id.clone(),
            name: h.name.clone(),
            ty: h.ty.clone(),
        })
        .collect();
    CitedAnswer {
        answer_markdown: text,
        citations,
        sources,
        confidence,
        not_covered: false,
        stripped_citations: stripped,
        uncited_claims,
        unverified,
    }
}

/// Remove `[[id]]` markers whose id is not allowed. Returns the new text, the valid ids in
/// order of appearance, and the stripped ids.
fn rewrite_markers(text: &str, allowed: &HashSet<&str>) -> (String, Vec<String>, Vec<String>) {
    let mut out = String::with_capacity(text.len());
    let mut valid = Vec::new();
    let mut bad = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else { break };
        let id = after[..end].trim();
        if allowed.contains(id) {
            out.push_str(&rest[..start + 2 + end + 2]);
            valid.push(id.to_string());
        } else {
            out.push_str(rest[..start].trim_end_matches(' '));
            bad.push(id.to_string());
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    (out, valid, bad)
}

/// Sentences (long enough to be a claim) that contain no `[[...]]` marker.
fn uncited_sentences(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut current = String::new();
        let chars: Vec<char> = line.chars().collect();
        for (n, &c) in chars.iter().enumerate() {
            current.push(c);
            let ends = matches!(c, '.' | '!' | '?')
                && chars.get(n + 1).map_or(true, |x| x.is_whitespace());
            if ends {
                push_if_uncited(&mut found, &current);
                current.clear();
            }
        }
        push_if_uncited(&mut found, &current);
    }
    found
}

fn push_if_uncited(found: &mut Vec<String>, sentence: &str) {
    let s = sentence.trim();
    if s.contains("[[") || s.split_whitespace().count() < MIN_CLAIM_WORDS {
        return;
    }
    found.push(s.to_string());
}

#[cfg(test)]
mod tests {
    use super::super::testutil::ScriptedRunner;
    use super::*;

    fn hit(id: &str) -> Hit {
        Hit {
            id: id.into(),
            name: format!("Name {id}"),
            ty: "Condition".into(),
            ruleset: None,
            source: "SRD 5.2.1".into(),
            snippet: String::new(),
            score: 1.0,
        }
    }

    fn raw(md: &str, cites: &[&str], c: Confidence, nc: bool) -> RawCitedAnswer {
        RawCitedAnswer {
            answer_markdown: md.into(),
            citations: cites.iter().map(|s| (*s).to_string()).collect(),
            confidence: c,
            not_covered: nc,
        }
    }

    #[test]
    fn prompt_template_snapshot_is_stable() {
        let task = CitedQa {
            input: CitedQaInput {
                question: "What does Grappled do?".into(),
                ruleset: Ruleset::Dnd2024,
                excerpts: vec![Excerpt {
                    id: "ID_2024_CONDITION_GRAPPLED".into(),
                    name: "Grappled".into(),
                    ty: "Condition".into(),
                    source: "SRD 5.2.1".into(),
                    text: "Speed 0.".into(),
                }],
                character_summary: Some("Fighter 3, Speed 30".into()),
            },
        };
        let expected = "<context>\n[[ID_2024_CONDITION_GRAPPLED]] Grappled (Condition, SRD 5.2.1)\nSpeed 0.\n</context>\n\n<character>\nFighter 3, Speed 30\n</character>\n\nQuestion:\n<user_input>\nWhat does Grappled do?\n</user_input>";
        assert_eq!(task.user_prompt(), expected);
        let sys = task.system_prompt();
        assert!(sys.contains("Answer ONLY from the excerpts inside <context>"));
        assert!(sys.contains("[[ID_...]]"));
        assert!(sys.contains("not covered by your enabled content"));
        assert!(sys.ends_with("not_covered (boolean)."));
    }

    #[test]
    fn hallucinated_ids_are_stripped_and_confidence_lowered() {
        let hits = [hit("ID_A")];
        let r = raw(
            "Speed is 0 while grappled [[ID_A]]. You also gain flight [[ID_FAKE]].",
            &["ID_A", "ID_GHOST"],
            Confidence::High,
            false,
        );
        let a = validate_answer(r, &hits);
        assert_eq!(a.citations, vec!["ID_A"]);
        assert_eq!(a.stripped_citations, vec!["ID_FAKE", "ID_GHOST"]);
        assert!(!a.answer_markdown.contains("ID_FAKE"));
        assert_eq!(a.confidence, Confidence::Medium);
        assert_eq!(a.sources[0].name, "Name ID_A");
        assert!(!a.unverified);
    }

    #[test]
    fn all_citations_invalid_is_unverified_and_low() {
        let a = validate_answer(
            raw(
                "The rule is simple and clear [[ID_X]].",
                &["ID_X"],
                Confidence::High,
                false,
            ),
            &[hit("ID_A")],
        );
        assert!(a.unverified && a.citations.is_empty());
        assert_eq!(a.confidence, Confidence::Low);
    }

    #[test]
    fn uncited_sentences_are_flagged() {
        let a = validate_answer(
            raw(
                "Grappled sets your speed to 0 [[ID_A]]. Everyone also loses their reaction entirely.",
                &["ID_A"],
                Confidence::High,
                false,
            ),
            &[hit("ID_A")],
        );
        assert_eq!(
            a.uncited_claims,
            vec!["Everyone also loses their reaction entirely."]
        );
        assert_eq!(a.confidence, Confidence::Medium);
    }

    #[test]
    fn not_covered_is_normalised() {
        let a = validate_answer(
            raw(
                "Not covered by your enabled content.",
                &["ID_A"],
                Confidence::High,
                true,
            ),
            &[hit("ID_A")],
        );
        assert!(a.not_covered && a.citations.is_empty());
        assert_eq!(a.confidence, Confidence::Low);
        assert!(a.uncited_claims.is_empty() && !a.unverified);
    }

    #[tokio::test]
    async fn run_task_returns_raw_answer_without_repair_for_bad_ids() {
        let json = r#"{"answer_markdown":"x [[ID_NOPE]]","citations":["ID_NOPE"],"confidence":"high","not_covered":false}"#;
        let runner = ScriptedRunner::texts(&[json]);
        let task = CitedQa {
            input: CitedQaInput {
                question: "q".into(),
                ruleset: Ruleset::Dnd2014,
                excerpts: vec![],
                character_summary: None,
            },
        };
        let r = run_task(&runner, &task, &RunOptions::default())
            .await
            .unwrap();
        assert_eq!(r.attempts, 1);
    }
}
