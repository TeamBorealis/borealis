//! A serialisable union of every task, used by the LAN protocol and by UI command handlers.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::advisor::{Advisor, AdvisorInput};
use super::homebrew::{ConvertInput, ConvertText, HomebrewGenerate, HomebrewInput};
use super::narrative::{Narrative, NarrativeInput};
use super::qa::{QaInput, RulesQa};
use super::review::{Review, ReviewInput};
use super::validate::ContentValidator;
use super::{run_task, AiRunner, AiTask, RunOptions, TaskResult};
use crate::event::Usage;
use crate::{AiError, InputTrust};

/// Any task with its inputs. Wire shape: `{"task": "<name>", "input": {...}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "task", content = "input", rename_all = "snake_case")]
pub enum TaskRequest {
    /// Homebrew content generator.
    HomebrewGenerate(HomebrewInput),
    /// Free text / stat block to content JSON.
    ConvertText(ConvertInput),
    /// Rules Q&A.
    RulesQa(QaInput),
    /// Build / level-up advisor.
    Advisor(AdvisorInput),
    /// Backstory / personality / NPC.
    Narrative(NarrativeInput),
    /// Balance review / lint explanation.
    Review(ReviewInput),
}

/// Untyped task result for transports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskResponse {
    /// Task name.
    pub task: String,
    /// The typed output serialised as JSON.
    pub output: Value,
    /// Prompt version used.
    pub prompt_version: u32,
    /// Attempts used.
    pub attempts: u32,
    /// Usage totals.
    pub usage: Option<Usage>,
}

impl TaskRequest {
    /// Stable task name.
    pub fn name(&self) -> &'static str {
        match self {
            TaskRequest::HomebrewGenerate(_) => HomebrewGenerate::NAME,
            TaskRequest::ConvertText(_) => ConvertText::NAME,
            TaskRequest::RulesQa(_) => RulesQa::NAME,
            TaskRequest::Advisor(_) => Advisor::NAME,
            TaskRequest::Narrative(_) => Narrative::NAME,
            TaskRequest::Review(_) => Review::NAME,
        }
    }

    /// Trust of this task's inputs. `ConvertText` carries pasted text from elsewhere (stat blocks,
    /// web pages, documents), so it is [`InputTrust::Untrusted`]; the other tasks take text the
    /// user typed plus Borealis data and are `Trusted`. Callers combine this with what they know
    /// (for example an imported character file) and pass it to
    /// [`crate::AiSession::with_input_trust`].
    pub fn input_trust(&self) -> InputTrust {
        match self {
            TaskRequest::ConvertText(_) => InputTrust::Untrusted,
            _ => InputTrust::Trusted,
        }
    }

    /// Run the task. `validator` replaces the default content validator for the content tasks.
    pub async fn execute(
        &self,
        runner: &dyn AiRunner,
        validator: Option<Arc<dyn ContentValidator>>,
        opts: &RunOptions<'_>,
    ) -> Result<TaskResponse, AiError> {
        match self {
            TaskRequest::HomebrewGenerate(i) => {
                let mut t = HomebrewGenerate::new(i.clone());
                if let Some(v) = validator {
                    t = t.with_validator(v);
                }
                respond(run_task(runner, &t, opts).await?, HomebrewGenerate::NAME)
            }
            TaskRequest::ConvertText(i) => {
                let mut t = ConvertText::new(i.clone());
                if let Some(v) = validator {
                    t = t.with_validator(v);
                }
                respond(run_task(runner, &t, opts).await?, ConvertText::NAME)
            }
            TaskRequest::RulesQa(i) => respond(
                run_task(runner, &RulesQa { input: i.clone() }, opts).await?,
                RulesQa::NAME,
            ),
            TaskRequest::Advisor(i) => respond(
                run_task(runner, &Advisor { input: i.clone() }, opts).await?,
                Advisor::NAME,
            ),
            TaskRequest::Narrative(i) => respond(
                run_task(runner, &Narrative { input: i.clone() }, opts).await?,
                Narrative::NAME,
            ),
            TaskRequest::Review(i) => respond(
                run_task(runner, &Review { input: i.clone() }, opts).await?,
                Review::NAME,
            ),
        }
    }
}

fn respond<T: Serialize>(r: TaskResult<T>, name: &str) -> Result<TaskResponse, AiError> {
    let output = serde_json::to_value(&r.output).map_err(|e| AiError::Invalid(e.to_string()))?;
    Ok(TaskResponse {
        task: name.to_string(),
        output,
        prompt_version: r.prompt_version,
        attempts: r.attempts,
        usage: r.usage,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::prompt::Ruleset;
    use super::super::testutil::ScriptedRunner;
    use super::*;

    #[test]
    fn wire_shape_is_task_plus_input() {
        let req = TaskRequest::RulesQa(QaInput {
            question: "q".into(),
            ruleset: Ruleset::Dnd2014,
            context: vec![],
            character: None,
        });
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["task"], "rules_qa");
        assert_eq!(v["input"]["question"], "q");
        assert_eq!(serde_json::from_value::<TaskRequest>(v).unwrap(), req);
        assert_eq!(req.name(), "rules_qa");
    }

    #[test]
    fn unknown_task_is_rejected() {
        assert!(serde_json::from_value::<TaskRequest>(json!({"task":"rm_rf","input":{}})).is_err());
    }

    #[tokio::test]
    async fn execute_dispatches_and_serialises() {
        let req = TaskRequest::RulesQa(QaInput {
            question: "q".into(),
            ruleset: Ruleset::Dnd2014,
            context: vec![],
            character: None,
        });
        let runner = ScriptedRunner::texts(&[r#"{"answer":"a","confidence":"low"}"#]);
        let resp = req
            .execute(&runner, None, &RunOptions::default())
            .await
            .unwrap();
        assert_eq!(resp.task, "rules_qa");
        assert_eq!(resp.output["answer"], "a");
        assert_eq!(resp.attempts, 1);
    }

    #[tokio::test]
    async fn all_task_names_are_distinct() {
        let names = [
            HomebrewGenerate::NAME,
            ConvertText::NAME,
            RulesQa::NAME,
            Advisor::NAME,
            Narrative::NAME,
            Review::NAME,
        ];
        let set: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(set.len(), names.len());
    }
}
