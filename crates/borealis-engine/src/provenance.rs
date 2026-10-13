//! Stat provenance: a deterministic trace of how each statistic got its value.
//!
//! A [`StatTrace`] lists the base value, then every contributing or rejected rule in evaluation
//! order, then the final value. The invariant is `value == sum(step.value for included top-level steps)`.
//! Prose ("explain this number") is layered on top of this trace; the trace itself never changes
//! a result.

use crate::diag::Diagnostic;
use serde::{Deserialize, Serialize};

/// A statistic group name (lower-case, e.g. `ac`, `strength:score`).
pub type StatName = String;

/// Options for one evaluation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EvaluateOptions {
    /// Record [`crate::snapshot::CharacterSnapshot::provenance`] for every statistic.
    pub provenance: bool,
}

/// What a trace step is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    /// An engine-provided starting value (level, base ability score, proficiency table).
    Seed,
    /// A value the engine derives itself (modifier table, score override or cap).
    Derived,
    /// A content stat rule that was considered.
    Rule,
}

/// One entry of a trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceStep {
    /// The group the step belongs to (a merged group such as `ac:misc` keeps its own name).
    pub group: StatName,
    /// Kind of step.
    pub kind: StepKind,
    /// Display label (rule `alt` or element name).
    pub label: String,
    /// Owning element id (rules only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element_id: Option<String>,
    /// Owning element name (rules only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element_name: Option<String>,
    /// Rule kind: `stat`, `seed` or `derived`.
    pub rule_kind: String,
    /// The value the step contributes when included (after any per-rule maximum).
    pub value: i32,
    /// The value as written in the rule (number or referenced group).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_value: Option<String>,
    /// Stacking label (`bonus`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bonus: Option<String>,
    /// Conditions on the rule: level, requirements, equipped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Does `value` count towards the final value?
    pub included: bool,
    /// Why the step was excluded, or a note on an included one (capped by a maximum).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// When the value is a reference to another statistic: that statistic's steps (they explain
    /// `value` but do not add to the parent's sum).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<TraceStep>,
}

/// The full trace of one statistic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatTrace {
    /// The statistic.
    pub stat: StatName,
    /// Sum of the included seed steps (base ability score, level, proficiency table).
    pub base: i32,
    /// Seed steps, then rules in evaluation order (merged groups follow).
    pub steps: Vec<TraceStep>,
    /// Final value.
    pub value: i32,
    /// Diagnostics (legacy divergences, unresolved references) touching this trace.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
}

impl StatTrace {
    /// Assemble a trace from steps and the diagnostics of an evaluation.
    #[must_use]
    pub fn build(stat: &str, steps: Vec<TraceStep>, diags: &[Diagnostic]) -> Self {
        let steps = prune_steps(steps, true);
        let value = steps.iter().filter(|s| s.included).map(|s| s.value).sum();
        let base = steps
            .iter()
            .filter(|s| s.included && s.kind == StepKind::Seed)
            .map(|s| s.value)
            .sum();
        let mut groups: Vec<&str> = vec![stat];
        for s in &steps {
            if !groups.contains(&s.group.as_str()) {
                groups.push(&s.group);
            }
        }
        let diagnostics = diags
            .iter()
            .filter(|d| d.element.as_deref().is_some_and(|e| groups.contains(&e)))
            .cloned()
            .collect();
        Self {
            stat: stat.to_string(),
            base,
            steps,
            value,
            diagnostics,
        }
    }
}

/// Drop explanation noise without changing any total: included rule steps worth 0 that explain
/// nothing, and (inside a referenced statistic's inputs) alternatives that did not apply. Top-level
/// skipped rules stay, since they show which other formulas were considered.
fn prune_steps(steps: Vec<TraceStep>, top: bool) -> Vec<TraceStep> {
    steps
        .into_iter()
        .filter_map(|mut s| {
            s.inputs = prune_steps(s.inputs, false);
            let empty_zero = s.included && s.value == 0 && s.reason.is_none();
            let nested_skip = !s.included && !top;
            let noise =
                s.kind == StepKind::Rule && s.inputs.is_empty() && (empty_zero || nested_skip);
            (!noise).then_some(s)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(label: &str, value: i32, included: bool, inputs: Vec<TraceStep>) -> TraceStep {
        TraceStep {
            group: "ac".into(),
            kind: StepKind::Rule,
            label: label.into(),
            element_id: None,
            element_name: None,
            rule_kind: "stat".into(),
            value,
            raw_value: None,
            bonus: None,
            condition: None,
            included,
            reason: None,
            inputs,
        }
    }

    #[test]
    fn prunes_empty_zero_rows_and_nested_skips_but_keeps_top_level_skips() {
        let armored = rule(
            "Armored",
            16,
            true,
            vec![
                rule("Chain Mail", 16, true, vec![]),
                rule(
                    "Base",
                    0,
                    true,
                    vec![
                        rule("Base", 0, true, vec![]),
                        rule("Light", 0, false, vec![]),
                        TraceStep {
                            reason: Some("equipped condition is false".into()),
                            ..rule("Medium", 0, false, vec![])
                        },
                    ],
                ),
            ],
        );
        let steps = vec![
            armored,
            rule("Unarmored", 10, false, vec![]),
            rule("Shield", 2, true, vec![]),
        ];
        let t = StatTrace::build("ac", steps, &[]);
        assert_eq!(t.value, 18);
        let labels: Vec<_> = t.steps.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["Armored", "Unarmored", "Shield"]);
        assert_eq!(t.steps[0].inputs.len(), 1);
        assert_eq!(t.steps[0].inputs[0].label, "Chain Mail");
    }
}
