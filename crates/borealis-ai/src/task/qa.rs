//! Rules Q&A with caller-retrieved context (task b).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::prompt::{base_system, fence, Ruleset};
use super::AiTask;

/// At most this many context snippets are sent.
pub const MAX_SNIPPETS: usize = 40;
/// Each snippet is cut to this many characters.
pub const MAX_SNIPPET_CHARS: usize = 4_000;

/// One retrieved element/rule text the answer may cite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextSnippet {
    /// Stable id (element id) used for citations.
    pub id: String,
    /// Display title.
    pub title: String,
    /// Plain text of the rule/element.
    pub text: String,
}

/// Inputs for [`RulesQa`] (also the wire format).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QaInput {
    /// The user's question.
    pub question: String,
    /// Active ruleset.
    pub ruleset: Ruleset,
    /// Retrieved context; the caller does retrieval.
    #[serde(default)]
    pub context: Vec<ContextSnippet>,
    /// Optional character summary for personalised answers.
    #[serde(default)]
    pub character: Option<Value>,
}

/// How sure the model is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// Directly supported by the supplied context.
    High,
    /// Mostly supported, some inference.
    Medium,
    /// Mostly general knowledge or unclear rules.
    Low,
}

/// Typed answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QaAnswer {
    /// Answer in Markdown.
    pub answer: String,
    /// Ids of the context snippets relied on.
    #[serde(default)]
    pub citations: Vec<String>,
    /// Confidence level.
    pub confidence: Confidence,
    /// Caveats, house-rule dependencies, table-variance notes.
    #[serde(default)]
    pub caveats: Vec<String>,
}

/// Task (b).
#[derive(Debug, Clone)]
pub struct RulesQa {
    /// Inputs.
    pub input: QaInput,
}

impl AiTask for RulesQa {
    type Output = QaAnswer;
    const NAME: &'static str = "rules_qa";

    fn system_prompt(&self) -> String {
        format!(
            "{}\n\nRuleset: {}\nAnswer from the <context> snippets first and cite their ids in `citations`. \
If the context does not cover the question, answer from general knowledge, set confidence to \"low\" or \"medium\" and say so in `caveats`. \
Never cite an id that is not in the context. Be concise; show the arithmetic when a number is involved.",
            base_system("answer a rules question accurately"),
            self.input.ruleset.describe()
        )
    }

    fn user_prompt(&self) -> String {
        let i = &self.input;
        let mut p = String::new();
        if !i.context.is_empty() {
            let body: String = i
                .context
                .iter()
                .take(MAX_SNIPPETS)
                .map(|c| {
                    let text: String = c.text.chars().take(MAX_SNIPPET_CHARS).collect();
                    format!("[{}] {}\n{}", c.id, c.title, text)
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            p.push_str(&fence("context", &body));
            p.push_str("\n\n");
        }
        if let Some(c) = &i.character {
            p.push_str(&fence("character", &c.to_string()));
            p.push_str("\n\n");
        }
        p.push_str("Question:\n");
        p.push_str(&fence("user_input", &i.question));
        p
    }

    fn output_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["answer", "confidence"],
            "properties": {
                "answer": {"type": "string", "minLength": 1},
                "citations": {"type": "array", "items": {"type": "string"}},
                "confidence": {"enum": ["high", "medium", "low"]},
                "caveats": {"type": "array", "items": {"type": "string"}}
            }
        })
    }

    fn check(&self, out: &QaAnswer) -> Vec<String> {
        let known: HashSet<&str> = self.input.context.iter().map(|c| c.id.as_str()).collect();
        out.citations
            .iter()
            .filter(|c| !known.contains(c.as_str()))
            .map(|c| format!("citation {c:?} is not an id from the provided context"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::ScriptedRunner;
    use super::super::{run_task, RunOptions};
    use super::*;

    fn input() -> QaInput {
        QaInput {
            question: "Can I cast two leveled spells in one turn?".into(),
            ruleset: Ruleset::Dnd2014,
            context: vec![ContextSnippet {
                id: "ID_RULE_BONUS_SPELL".into(),
                title: "Bonus Action Spell".into(),
                text: "If you cast a spell as a bonus action...".into(),
            }],
            character: None,
        }
    }

    #[test]
    fn prompt_contains_context_ids_and_fenced_question() {
        let p = RulesQa { input: input() }.user_prompt();
        assert!(
            p.contains("[ID_RULE_BONUS_SPELL]")
                && p.contains("<context>")
                && p.contains("<user_input>")
        );
    }

    #[test]
    fn snippets_are_capped() {
        let mut i = input();
        i.context = (0..100)
            .map(|n| ContextSnippet {
                id: format!("ID_{n}"),
                title: "t".into(),
                text: "x".repeat(10_000),
            })
            .collect();
        let p = RulesQa { input: i }.user_prompt();
        assert!(p.contains("[ID_39]") && !p.contains("[ID_40]"));
        assert!(p.len() < MAX_SNIPPETS * (MAX_SNIPPET_CHARS + 100));
    }

    #[tokio::test]
    async fn unknown_citation_triggers_repair() {
        let bad = r#"{"answer":"No.","citations":["ID_MADE_UP"],"confidence":"high"}"#;
        let good = r#"{"answer":"No.","citations":["ID_RULE_BONUS_SPELL"],"confidence":"high","caveats":[]}"#;
        let runner = ScriptedRunner::texts(&[bad, good]);
        let r = run_task(&runner, &RulesQa { input: input() }, &RunOptions::default())
            .await
            .unwrap();
        assert_eq!(r.attempts, 2);
        assert_eq!(r.output.confidence, Confidence::High);
        assert!(runner.requests.lock().unwrap()[1]
            .prompt
            .contains("ID_MADE_UP"));
    }

    #[tokio::test]
    async fn invalid_confidence_value_is_rejected_by_schema() {
        let runner = ScriptedRunner::texts(&[
            r#"{"answer":"x","confidence":"certain"}"#,
            r#"{"answer":"x","confidence":"low"}"#,
        ]);
        let r = run_task(
            &runner,
            &RulesQa {
                input: QaInput {
                    context: vec![],
                    ..input()
                },
            },
            &RunOptions::default(),
        )
        .await
        .unwrap();
        assert_eq!(r.attempts, 2);
    }
}
