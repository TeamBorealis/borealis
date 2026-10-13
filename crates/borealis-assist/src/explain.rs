//! Explain-this-number.
//!
//! The engine produces a [`StatTrace`] (base, every contributing or rejected rule, final value).
//! The model only turns that trace into a short plain-language explanation. The prose is then
//! checked: every number in it must appear in the trace, otherwise (or when the model is not
//! available at all) a deterministic template explanation built from the trace is returned.

use borealis_ai::task::prompt::{base_system, fence};
use borealis_ai::task::{run_task, AiRunner, AiTask, RunOptions};
use borealis_ai::Usage;
use borealis_engine::{CharacterState, Engine, StatTrace, StepKind, TraceStep};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::numbers::NumberGuard;
use crate::ProseSource;

/// An explanation of one statistic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Explanation {
    /// The statistic that was explained.
    pub stat: String,
    /// Its engine-computed value.
    pub value: i32,
    /// The explanation text.
    pub text: String,
    /// Model prose or template.
    pub source: ProseSource,
    /// Why the template was used instead of model prose.
    pub fallback_reason: Option<String>,
    /// The deterministic trace the text is about (the UI can show it next to the prose).
    pub trace: StatTrace,
    /// Model usage, when a model call happened.
    pub usage: Option<Usage>,
}

/// Maximum number of trace lines sent to the model.
const MAX_TRACE_LINES: usize = 80;
/// Upper bound for the explanation length (characters).
const MAX_EXPLANATION_CHARS: usize = 900;

fn signed(v: i32) -> String {
    if v >= 0 {
        format!("+{v}")
    } else {
        v.to_string()
    }
}

fn step_line(s: &TraceStep, depth: usize, out: &mut Vec<String>) {
    let indent = "  ".repeat(depth);
    let status = if s.included {
        "counted".to_string()
    } else {
        format!(
            "NOT counted: {}",
            s.reason.as_deref().unwrap_or("no reason recorded")
        )
    };
    let mut line = format!("{indent}{} | {status} | {}", signed(s.value), s.label);
    if let Some(e) = &s.element_name {
        if *e != s.label {
            line.push_str(&format!(" | from {e}"));
        }
    }
    if let Some(c) = &s.condition {
        line.push_str(&format!(" | only if {c}"));
    }
    out.push(line);
    for i in &s.inputs {
        step_line(i, depth + 1, out);
    }
}

/// The trace as the plain-text table that goes into the prompt (deterministic).
#[must_use]
pub fn render_trace(t: &StatTrace) -> String {
    let mut lines = vec![
        format!("stat: {}", t.stat),
        format!("base: {}", t.base),
        format!("final value: {}", t.value),
        "steps (value | status | label):".to_string(),
    ];
    let mut steps = Vec::new();
    for s in &t.steps {
        step_line(s, 0, &mut steps);
    }
    let total = steps.len();
    steps.truncate(MAX_TRACE_LINES);
    lines.extend(steps);
    if total > MAX_TRACE_LINES {
        lines.push(format!(
            "... {} more lines omitted",
            total - MAX_TRACE_LINES
        ));
    }
    for d in &t.diagnostics {
        lines.push(format!("note: {}", d.message));
    }
    lines.join("\n")
}

/// Every number of a trace (values, raw values and numbers in labels, reasons and notes).
#[must_use]
pub fn trace_numbers(t: &StatTrace) -> NumberGuard {
    fn walk(s: &TraceStep, g: &mut NumberGuard) {
        g.allow_int(i64::from(s.value));
        g.allow_text(&s.label);
        for text in [
            s.raw_value.as_deref(),
            s.bonus.as_deref(),
            s.condition.as_deref(),
            s.reason.as_deref(),
            s.element_name.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            g.allow_text(text);
        }
        for i in &s.inputs {
            walk(i, g);
        }
    }
    let mut g = NumberGuard::new();
    g.allow_int(i64::from(t.base));
    g.allow_int(i64::from(t.value));
    g.allow_text(&t.stat);
    for s in &t.steps {
        walk(s, &mut g);
    }
    for d in &t.diagnostics {
        g.allow_text(&d.message);
    }
    g
}

/// The deterministic explanation (also the offline one).
#[must_use]
pub fn template_text(t: &StatTrace) -> String {
    if t.steps.is_empty() {
        return format!(
            "Nothing contributes to \"{}\"; its value is {}.",
            t.stat, t.value
        );
    }
    let mut out = format!("{} is {}.", t.stat, t.value);
    let seeds: Vec<String> = t
        .steps
        .iter()
        .filter(|s| s.included && s.kind == StepKind::Seed)
        .map(|s| format!("{} ({})", s.label, s.value))
        .collect();
    if !seeds.is_empty() {
        out.push_str(&format!(" It starts from {}.", seeds.join(", ")));
    }
    let added: Vec<String> = t
        .steps
        .iter()
        .filter(|s| s.included && s.kind != StepKind::Seed && s.value != 0)
        .map(|s| format!("{} ({})", s.label, signed(s.value)))
        .collect();
    if !added.is_empty() {
        out.push_str(&format!(" Then it adds {}.", added.join(", ")));
    }
    let skipped: Vec<String> = t
        .steps
        .iter()
        .filter(|s| !s.included)
        .map(|s| {
            format!(
                "{} ({})",
                s.label,
                s.reason.as_deref().unwrap_or("not applicable")
            )
        })
        .collect();
    if !skipped.is_empty() {
        out.push_str(&format!(" Not counted: {}.", skipped.join("; ")));
    }
    for d in &t.diagnostics {
        out.push_str(&format!(" Note: {}", d.message));
        if !d.message.ends_with('.') {
            out.push('.');
        }
    }
    out
}

/// Model answer of the explain task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplainOutput {
    /// The explanation.
    pub explanation: String,
}

/// The AI task behind [`explain_number`].
#[derive(Debug, Clone)]
pub struct ExplainNumber {
    trace: StatTrace,
}

impl ExplainNumber {
    /// A task for one trace.
    #[must_use]
    pub fn new(trace: StatTrace) -> Self {
        Self { trace }
    }
}

impl AiTask for ExplainNumber {
    type Output = ExplainOutput;
    const NAME: &'static str = "explain_number";

    fn system_prompt(&self) -> String {
        let mut s = base_system(
            "explain, in plain language, how one character statistic got its value, using only the supplied calculation trace",
        );
        s.push_str(
            "\n\nTask rules:\n\
- Write 2 to 4 short sentences for a player, not a rules lawyer.\n\
- Use ONLY facts from <trace>. Quote numbers exactly as they appear there; never compute new numbers, totals, averages or percentages.\n\
- Mention what raised or lowered the value, and mention a rule that was NOT counted only if it helps the player understand why.\n\
- If <trace> has a note, relay it plainly.\n\
- Do not give strategy advice and do not mention the trace format.",
        );
        s
    }

    fn user_prompt(&self) -> String {
        format!(
            "Explain the statistic \"{}\".\n\n{}",
            self.trace.stat,
            fence("trace", &render_trace(&self.trace))
        )
    }

    fn output_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "explanation": {"type": "string", "minLength": 1, "maxLength": MAX_EXPLANATION_CHARS}
            },
            "required": ["explanation"],
            "additionalProperties": false
        })
    }
}

/// Explain a statistic without any AI (mobile and offline): the deterministic template.
#[must_use]
pub fn explain_number_offline(engine: &Engine, state: &CharacterState, stat: &str) -> Explanation {
    template_explanation(
        engine.explain(state, stat),
        "offline mode".to_string(),
        None,
    )
}

fn template_explanation(trace: StatTrace, reason: String, usage: Option<Usage>) -> Explanation {
    Explanation {
        stat: trace.stat.clone(),
        value: trace.value,
        text: template_text(&trace),
        source: ProseSource::Template,
        fallback_reason: Some(reason),
        trace,
        usage,
    }
}

/// Explain a statistic with the model, verified against the engine's trace.
///
/// Never fails: when the model is unavailable, answers badly, or states a number that is not in
/// the trace, the deterministic template explanation is returned with `fallback_reason` set.
pub async fn explain_number(
    runner: &dyn AiRunner,
    engine: &Engine,
    state: &CharacterState,
    stat: &str,
    opts: &RunOptions<'_>,
) -> Explanation {
    let trace = engine.explain(state, stat);
    if trace.steps.is_empty() {
        return template_explanation(
            trace,
            "the engine has no data for this statistic".into(),
            None,
        );
    }
    let task = ExplainNumber::new(trace.clone());
    let opts = RunOptions {
        max_repairs: opts.max_repairs.min(1),
        sink: opts.sink,
    };
    let result = match run_task(runner, &task, &opts).await {
        Ok(r) => r,
        Err(e) => return template_explanation(trace, format!("the model call failed: {e}"), None),
    };
    let text = result.output.explanation.trim().to_string();
    let unknown = trace_numbers(&trace).unknown(&text);
    if !unknown.is_empty() {
        return template_explanation(
            trace,
            format!(
                "the model mentioned numbers that are not in the calculation: {}",
                unknown.join(", ")
            ),
            result.usage,
        );
    }
    Explanation {
        stat: trace.stat.clone(),
        value: trace.value,
        text,
        source: ProseSource::Model,
        fallback_reason: None,
        trace,
        usage: result.usage,
    }
}
