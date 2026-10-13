//! Homebrew balance review and lint explanations (task e).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::prompt::{base_system, fence, Ruleset};
use super::validate::{issues_to_strings, ValidationIssue, MAX_ISSUES_REPORTED};
use super::AiTask;

/// Content larger than this many characters is truncated in the prompt.
pub const MAX_CONTENT_CHARS: usize = 100_000;

/// Which review to run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ReviewMode {
    /// Judge power level and design quality.
    Balance,
    /// Explain validator/lint findings in plain language with fixes.
    ExplainLint {
        /// Findings from the content validator.
        issues: Vec<ValidationIssue>,
    },
}

/// Inputs for [`Review`] (also the wire format).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewInput {
    /// Balance or lint explanation.
    #[serde(flatten)]
    pub mode: ReviewMode,
    /// Active ruleset.
    pub ruleset: Ruleset,
    /// The content-pack document under review.
    pub content: Value,
}

/// Overall balance judgement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// In line with official content.
    Balanced,
    /// Likely too strong.
    TooStrong,
    /// Likely too weak.
    TooWeak,
    /// Not enough information.
    Unclear,
}

/// Finding severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// FYI.
    Info,
    /// Worth fixing.
    Warning,
    /// Breaks the design or the rules.
    Problem,
}

/// One review finding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    /// Element concerned, if specific.
    #[serde(default)]
    pub element_id: Option<String>,
    /// How serious.
    pub severity: Severity,
    /// What is wrong.
    pub message: String,
    /// Suggested change.
    #[serde(default)]
    pub suggestion: Option<String>,
}

/// Plain-language explanation of one lint issue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LintExplanation {
    /// The issue text being explained (as provided).
    pub issue: String,
    /// What it means.
    pub explanation: String,
    /// How to fix it.
    pub fix: String,
}

/// Typed result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewResult {
    /// Balance verdict.
    pub verdict: Verdict,
    /// Short summary.
    pub summary: String,
    /// Findings.
    #[serde(default)]
    pub findings: Vec<Finding>,
    /// One entry per supplied lint issue (ExplainLint mode).
    #[serde(default)]
    pub lint_explanations: Vec<LintExplanation>,
}

/// Task (e).
#[derive(Debug, Clone)]
pub struct Review {
    /// Inputs.
    pub input: ReviewInput,
}

impl Review {
    fn lint_issues(&self) -> &[ValidationIssue] {
        match &self.input.mode {
            ReviewMode::ExplainLint { issues } => &issues[..issues.len().min(MAX_ISSUES_REPORTED)],
            ReviewMode::Balance => &[],
        }
    }
}

impl AiTask for Review {
    type Output = ReviewResult;
    const NAME: &'static str = "review";

    fn system_prompt(&self) -> String {
        let (goal, extra) = match self.input.mode {
            ReviewMode::Balance => (
                "review homebrew content for balance and design quality",
                "Compare against official content of the same kind and level. Be specific (numbers, comparisons) and constructive; give concrete suggestions.",
            ),
            ReviewMode::ExplainLint { .. } => (
                "explain content-validation findings to a homebrew author",
                "Return exactly one `lint_explanations` entry per supplied issue, in order, copying the issue text into `issue`. Explain in plain language and give a concrete fix. Set verdict to \"unclear\" unless you also assessed balance.",
            ),
        };
        format!(
            "{}\n\nRuleset: {}\n{extra}",
            base_system(goal),
            self.input.ruleset.describe()
        )
    }

    fn user_prompt(&self) -> String {
        let raw = serde_json::to_string_pretty(&self.input.content).unwrap_or_default();
        let mut content: String = raw.chars().take(MAX_CONTENT_CHARS).collect();
        if raw.chars().count() > MAX_CONTENT_CHARS {
            content.push_str("\n...[truncated]");
        }
        let mut p = fence("content", &content);
        let issues = self.lint_issues();
        if !issues.is_empty() {
            p.push_str("\n\nIssues to explain:\n");
            for (i, s) in issues_to_strings(issues).iter().enumerate() {
                p.push_str(&format!("{}. {s}\n", i + 1));
            }
        }
        p
    }

    fn output_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["verdict", "summary"],
            "properties": {
                "verdict": {"enum": ["balanced", "too_strong", "too_weak", "unclear"]},
                "summary": {"type": "string"},
                "findings": {"type": "array", "items": {
                    "type": "object",
                    "required": ["severity", "message"],
                    "properties": {
                        "element_id": {"type": ["string", "null"]},
                        "severity": {"enum": ["info", "warning", "problem"]},
                        "message": {"type": "string"},
                        "suggestion": {"type": ["string", "null"]}
                    }
                }},
                "lint_explanations": {"type": "array", "items": {
                    "type": "object",
                    "required": ["issue", "explanation", "fix"],
                    "properties": {"issue": {"type": "string"}, "explanation": {"type": "string"}, "fix": {"type": "string"}}
                }}
            }
        })
    }

    fn check(&self, out: &ReviewResult) -> Vec<String> {
        let want = self.lint_issues().len();
        if want > 0 && out.lint_explanations.len() != want {
            return vec![format!(
                "expected {want} lint_explanations (one per issue) but got {}",
                out.lint_explanations.len()
            )];
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::ScriptedRunner;
    use super::super::{run_task, RunOptions};
    use super::*;

    fn lint_input() -> ReviewInput {
        ReviewInput {
            mode: ReviewMode::ExplainLint {
                issues: vec![
                    ValidationIssue::new("/elements/0", "\"source\" is a required property"),
                    ValidationIssue::new("/elements/1/id", "bad id"),
                ],
            },
            ruleset: Ruleset::Dnd2014,
            content: json!({"format":"borealis-content/1","elements":[]}),
        }
    }

    #[test]
    fn lint_prompt_numbers_issues() {
        let u = Review {
            input: lint_input(),
        }
        .user_prompt();
        assert!(
            u.contains("1. /elements/0")
                && u.contains("2. /elements/1/id")
                && u.contains("<content>")
        );
    }

    #[test]
    fn wire_format_uses_flattened_mode() {
        let v = serde_json::to_value(lint_input()).unwrap();
        assert_eq!(v["mode"], "explain_lint");
        assert_eq!(v["issues"].as_array().unwrap().len(), 2);
        let b: ReviewInput =
            serde_json::from_value(json!({"mode":"balance","ruleset":"dnd2014","content":{}}))
                .unwrap();
        assert_eq!(b.mode, ReviewMode::Balance);
    }

    #[tokio::test]
    async fn explanation_count_must_match_issue_count() {
        let one = r#"{"verdict":"unclear","summary":"s","lint_explanations":[{"issue":"a","explanation":"e","fix":"f"}]}"#;
        let two = r#"{"verdict":"unclear","summary":"s","lint_explanations":[{"issue":"a","explanation":"e","fix":"f"},{"issue":"b","explanation":"e","fix":"f"}]}"#;
        let runner = ScriptedRunner::texts(&[one, two]);
        let r = run_task(
            &runner,
            &Review {
                input: lint_input(),
            },
            &RunOptions::default(),
        )
        .await
        .unwrap();
        assert_eq!(r.attempts, 2);
        assert_eq!(r.output.lint_explanations.len(), 2);
    }

    #[tokio::test]
    async fn balance_review_with_nullable_fields() {
        let ok = r#"{"verdict":"too_strong","summary":"s","findings":[{"element_id":null,"severity":"warning","message":"m","suggestion":null}]}"#;
        let runner = ScriptedRunner::texts(&[ok]);
        let input = ReviewInput {
            mode: ReviewMode::Balance,
            ruleset: Ruleset::Dnd2024,
            content: json!({}),
        };
        let r = run_task(&runner, &Review { input }, &RunOptions::default())
            .await
            .unwrap();
        assert_eq!(r.output.verdict, Verdict::TooStrong);
        assert!(r.output.findings[0].element_id.is_none());
    }

    #[test]
    fn huge_content_is_truncated() {
        let big = json!({"x": "y".repeat(MAX_CONTENT_CHARS * 2)});
        let u = Review {
            input: ReviewInput {
                mode: ReviewMode::Balance,
                ruleset: Ruleset::Dnd2014,
                content: big,
            },
        }
        .user_prompt();
        assert!(u.contains("[truncated]") && u.len() < MAX_CONTENT_CHARS + 200);
    }
}
