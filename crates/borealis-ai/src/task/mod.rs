//! Task layer: versioned prompt templates, typed results, schema validation, repair loop.
//!
//! A task implements [`AiTask`]. [`run_task`] runs it through any [`AiRunner`] (the real CLI
//! runner in production, a fake in tests), validates the answer against the task's output schema
//! plus task-specific checks, and on failure feeds the errors back for up to
//! [`RunOptions::max_repairs`] repair attempts.

pub mod advisor;
pub mod cited_qa;
pub mod homebrew;
pub mod narrative;
pub mod prompt;
pub mod qa;
pub mod request;
pub mod review;
pub mod validate;

use std::future::Future;
use std::pin::Pin;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::event::Usage;
use crate::parse::extract_json;
use crate::session::AiSession;
use crate::{AiError, AiEvent, AiOutcome, AiRequest};

pub use prompt::{ContentKind, Ruleset, PROMPT_VERSION};
pub use validate::{ContentValidator, JsonSchemaValidator, ValidationIssue};

/// Default number of repair attempts after the first try.
pub const DEFAULT_MAX_REPAIRS: u32 = 2;

/// Boxed future (keeps [`AiRunner`] object-safe without extra dependencies).
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Anything that can execute one AI request. Implemented by [`AiSession`] (local CLI) and by
/// the LAN remote client; tests use fakes.
pub trait AiRunner: Send + Sync {
    /// Execute `req`, optionally forwarding streamed events to `sink`.
    fn run<'a>(
        &'a self,
        req: &'a AiRequest,
        sink: Option<&'a mpsc::UnboundedSender<AiEvent>>,
    ) -> BoxFuture<'a, Result<AiOutcome, AiError>>;
}

impl AiRunner for AiSession {
    fn run<'a>(
        &'a self,
        req: &'a AiRequest,
        sink: Option<&'a mpsc::UnboundedSender<AiEvent>>,
    ) -> BoxFuture<'a, Result<AiOutcome, AiError>> {
        Box::pin(AiSession::run(self, req, sink))
    }
}

/// A unit of AI work with a typed result.
pub trait AiTask {
    /// Typed result.
    type Output: DeserializeOwned + Serialize;
    /// Stable task name (used on the wire and in logs).
    const NAME: &'static str;

    /// System prompt (task-specific instructions).
    fn system_prompt(&self) -> String;
    /// User prompt with all inputs fenced as data.
    fn user_prompt(&self) -> String;
    /// JSON Schema of the output, sent to the CLI and used for validation.
    fn output_schema(&self) -> Value;
    /// Extra semantic checks; each returned string is fed back to the model on repair.
    fn check(&self, _output: &Self::Output) -> Vec<String> {
        Vec::new()
    }
}

/// Knobs for [`run_task`].
#[derive(Debug, Clone, Copy)]
pub struct RunOptions<'a> {
    /// Repair attempts after the first try.
    pub max_repairs: u32,
    /// Where to forward streamed events (events of every attempt are forwarded).
    pub sink: Option<&'a mpsc::UnboundedSender<AiEvent>>,
}

impl Default for RunOptions<'_> {
    fn default() -> Self {
        Self {
            max_repairs: DEFAULT_MAX_REPAIRS,
            sink: None,
        }
    }
}

/// Typed result plus provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskResult<T> {
    /// The validated output.
    pub output: T,
    /// Prompt template version that produced it.
    pub prompt_version: u32,
    /// Attempts used (1 = no repair needed).
    pub attempts: u32,
    /// Usage summed over attempts, when reported.
    pub usage: Option<Usage>,
}

/// Run `task` through `runner` with validation and the repair loop.
pub async fn run_task<T: AiTask>(
    runner: &dyn AiRunner,
    task: &T,
    opts: &RunOptions<'_>,
) -> Result<TaskResult<T::Output>, AiError> {
    let schema = task.output_schema();
    let schema_check = JsonSchemaValidator::from_schema(&schema)?;
    let system = task.system_prompt();
    let user = task.user_prompt();
    let mut prompt = user.clone();
    let mut usage: Option<Usage> = None;
    let total_attempts = opts.max_repairs + 1;
    let mut last_errors: Vec<String> = Vec::new();

    for attempt in 1..=total_attempts {
        let req = AiRequest {
            prompt: prompt.clone(),
            system: Some(system.clone()),
            schema: Some(schema.clone()),
        };
        let outcome = runner.run(&req, opts.sink).await?;
        if let Some(u) = &outcome.usage {
            usage = Some(usage.map_or_else(|| u.clone(), |t| t.add(u)));
        }
        let raw = outcome
            .structured
            .clone()
            .or_else(|| extract_json(&outcome.text));
        match evaluate(task, &schema_check, raw) {
            Ok(output) => {
                return Ok(TaskResult {
                    output,
                    prompt_version: PROMPT_VERSION,
                    attempts: attempt,
                    usage,
                });
            }
            Err(errors) => {
                tracing::debug!(
                    task = T::NAME,
                    attempt,
                    issues = errors.len(),
                    "ai output failed validation"
                );
                if attempt < total_attempts {
                    if let Some(s) = opts.sink {
                        let _ = s.send(AiEvent::Notice {
                            message: format!(
                                "Output had {} issue(s); asking the model to fix them.",
                                errors.len()
                            ),
                        });
                    }
                    prompt = prompt::repair_prompt(&user, &outcome.text, &errors);
                }
                last_errors = errors;
            }
        }
    }
    Err(AiError::Validation {
        attempts: total_attempts,
        errors: last_errors,
    })
}

fn evaluate<T: AiTask>(
    task: &T,
    schema_check: &JsonSchemaValidator,
    raw: Option<Value>,
) -> Result<T::Output, Vec<String>> {
    use validate::{issues_to_strings, ContentValidator as _};
    let Some(raw) = raw else {
        return Err(vec!["the answer was not a valid JSON value".to_string()]);
    };
    let schema_issues = schema_check.validate(&raw);
    if !schema_issues.is_empty() {
        return Err(issues_to_strings(&schema_issues));
    }
    let output: T::Output =
        serde_json::from_value(raw).map_err(|e| vec![format!("cannot read the answer: {e}")])?;
    let semantic = task.check(&output);
    if semantic.is_empty() {
        Ok(output)
    } else {
        Err(semantic)
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    //! A scripted fake runner shared by task tests.

    use std::sync::Mutex;

    use super::*;

    /// Returns queued responses in order and records every request it saw.
    pub struct ScriptedRunner {
        responses: Mutex<Vec<Result<AiOutcome, AiError>>>,
        pub requests: Mutex<Vec<AiRequest>>,
    }

    impl ScriptedRunner {
        pub fn new(responses: Vec<Result<AiOutcome, AiError>>) -> Self {
            let mut r = responses;
            r.reverse();
            Self {
                responses: Mutex::new(r),
                requests: Mutex::new(Vec::new()),
            }
        }

        /// Convenience: each item becomes a successful text outcome.
        pub fn texts(items: &[&str]) -> Self {
            Self::new(items.iter().map(|t| Ok(outcome_text(t))).collect())
        }
    }

    pub fn outcome_text(t: &str) -> AiOutcome {
        AiOutcome {
            text: t.to_string(),
            usage: Some(Usage {
                input_tokens: 10,
                output_tokens: 5,
                cached_input_tokens: None,
                cost_usd: Some(0.001),
            }),
            ..Default::default()
        }
    }

    impl AiRunner for ScriptedRunner {
        fn run<'a>(
            &'a self,
            req: &'a AiRequest,
            _sink: Option<&'a mpsc::UnboundedSender<AiEvent>>,
        ) -> BoxFuture<'a, Result<AiOutcome, AiError>> {
            self.requests.lock().unwrap().push(req.clone());
            let next = self
                .responses
                .lock()
                .unwrap()
                .pop()
                .unwrap_or(Err(AiError::NoResult));
            Box::pin(async move { next })
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::testutil::*;
    use super::*;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Out {
        n: u32,
    }

    struct EvenTask;
    impl AiTask for EvenTask {
        type Output = Out;
        const NAME: &'static str = "even";
        fn system_prompt(&self) -> String {
            "sys".into()
        }
        fn user_prompt(&self) -> String {
            "give an even number".into()
        }
        fn output_schema(&self) -> Value {
            json!({"type":"object","required":["n"],"properties":{"n":{"type":"integer"}},"additionalProperties":false})
        }
        fn check(&self, o: &Out) -> Vec<String> {
            if o.n.is_multiple_of(2) {
                vec![]
            } else {
                vec![format!("n={} is odd", o.n)]
            }
        }
    }

    #[tokio::test]
    async fn first_try_success_has_one_attempt() {
        let r = ScriptedRunner::texts(&[r#"{"n":2}"#]);
        let res = run_task(&r, &EvenTask, &RunOptions::default())
            .await
            .unwrap();
        assert_eq!(res.output, Out { n: 2 });
        assert_eq!(res.attempts, 1);
        assert_eq!(res.prompt_version, PROMPT_VERSION);
    }

    #[tokio::test]
    async fn repair_loop_feeds_errors_back_and_sums_usage() {
        let r = ScriptedRunner::texts(&[
            r#"{"n":3}"#,
            "not json at all",
            r#"```json
{"n":4}
```"#,
        ]);
        let res = run_task(&r, &EvenTask, &RunOptions::default())
            .await
            .unwrap();
        assert_eq!(res.attempts, 3);
        assert_eq!(res.usage.unwrap().input_tokens, 30);
        let reqs = r.requests.lock().unwrap();
        assert_eq!(reqs.len(), 3);
        assert!(
            reqs[1].prompt.contains("n=3 is odd"),
            "semantic error must be echoed back"
        );
        assert!(
            reqs[2].prompt.contains("not a valid JSON"),
            "parse failure must be echoed back"
        );
        assert!(reqs
            .iter()
            .all(|q| q.system.as_deref() == Some("sys") && q.schema.is_some()));
    }

    #[tokio::test]
    async fn gives_up_after_max_repairs_with_last_errors() {
        let r = ScriptedRunner::texts(&[r#"{"n":1}"#, r#"{"n":3}"#, r#"{"n":5}"#, r#"{"n":7}"#]);
        let err = run_task(&r, &EvenTask, &RunOptions::default())
            .await
            .unwrap_err();
        match err {
            AiError::Validation { attempts, errors } => {
                assert_eq!(attempts, 3);
                assert_eq!(errors, vec!["n=5 is odd".to_string()]);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(
            r.requests.lock().unwrap().len(),
            3,
            "never exceeds 1 + max_repairs"
        );
    }

    #[tokio::test]
    async fn schema_violations_are_reported_with_paths() {
        let r = ScriptedRunner::texts(&[r#"{"n":"two"}"#, r#"{"n":2}"#]);
        let res = run_task(&r, &EvenTask, &RunOptions::default())
            .await
            .unwrap();
        assert_eq!(res.attempts, 2);
        assert!(r.requests.lock().unwrap()[1].prompt.contains("/n"));
    }

    #[tokio::test]
    async fn zero_repairs_means_single_attempt() {
        let r = ScriptedRunner::texts(&[r#"{"n":1}"#, r#"{"n":2}"#]);
        let opts = RunOptions {
            max_repairs: 0,
            sink: None,
        };
        assert!(matches!(
            run_task(&r, &EvenTask, &opts).await,
            Err(AiError::Validation { attempts: 1, .. })
        ));
    }

    #[tokio::test]
    async fn runner_errors_are_not_retried() {
        let r = ScriptedRunner::new(vec![
            Err(AiError::Cancelled),
            Ok(outcome_text(r#"{"n":2}"#)),
        ]);
        assert!(matches!(
            run_task(&r, &EvenTask, &RunOptions::default()).await,
            Err(AiError::Cancelled)
        ));
        assert_eq!(r.requests.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn structured_output_is_preferred_over_text() {
        let mut o = outcome_text("garbage");
        o.structured = Some(json!({"n": 8}));
        let r = ScriptedRunner::new(vec![Ok(o)]);
        let res = run_task(&r, &EvenTask, &RunOptions::default())
            .await
            .unwrap();
        assert_eq!(res.output.n, 8);
    }

    #[tokio::test]
    async fn repair_notice_is_forwarded_to_sink() {
        let r = ScriptedRunner::texts(&[r#"{"n":1}"#, r#"{"n":2}"#]);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let opts = RunOptions {
            max_repairs: 2,
            sink: Some(&tx),
        };
        run_task(&r, &EvenTask, &opts).await.unwrap();
        assert!(matches!(rx.try_recv(), Ok(AiEvent::Notice { .. })));
    }
}
