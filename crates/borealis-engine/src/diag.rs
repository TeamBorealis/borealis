//! Diagnostics produced while evaluating a character.

use serde::{Deserialize, Serialize};

/// How serious a diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational (including legacy divergence notes).
    Info,
    /// Something is probably wrong but evaluation continued.
    Warning,
    /// Content or state error.
    Error,
}

/// Diagnostic codes with a stable string form. `legacy.*` codes mark places where the result
/// intentionally differs from what a legacy file may show.
pub mod code {
    pub const LEGACY_STAT_CHAIN_DEPTH: &str = "legacy.stat_chain_depth";
    pub const LEGACY_NONPOSITIVE_MERGE: &str = "legacy.nonpositive_merge";
    pub const LEGACY_BONUS_FLOOR: &str = "legacy.bonus_floor";
    pub const LEGACY_PREPARE_MIN_ONE: &str = "legacy.prepare_min_one";
    pub const LEGACY_INITIATIVE_MISC: &str = "legacy.initiative_misc";
    pub const STAT_CYCLE: &str = "stat.cycle";
    pub const STAT_UNRESOLVED: &str = "stat.unresolved_reference";
    pub const CONTENT_MISSING_ELEMENT: &str = "content.missing_element";
    pub const CONTENT_BAD_EXPRESSION: &str = "content.bad_expression";
    /// A custom rule kind no engine reader handles (`custom_rules::KNOWN_KINDS`).
    pub const CONTENT_UNKNOWN_RULE_KIND: &str = "content.unknown_rule_kind";
    /// A field of a known custom rule kind that its reader ignores.
    pub const CONTENT_UNKNOWN_RULE_FIELD: &str = "content.unknown_rule_field";
    pub const SELECTION_INVALID: &str = "selection.invalid";
    pub const SELECTION_MISSING_CONTENT: &str = "selection.missing_content";
    pub const ATTUNEMENT_OVER_LIMIT: &str = "attunement.over_limit";
    pub const EVAL_NOT_CONVERGED: &str = "eval.not_converged";
    pub const MULTICLASS_NO_SLOT_TABLE: &str = "multiclass.no_slot_table";
}

/// One diagnostic message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Stable machine-readable code (see [`code`]).
    pub code: String,
    /// Severity.
    pub severity: Severity,
    /// Human readable message.
    pub message: String,
    /// Element the diagnostic concerns, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<String>,
}

impl Diagnostic {
    /// Build a diagnostic.
    pub fn new(code: &str, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            severity,
            message: message.into(),
            element: None,
        }
    }

    /// Attach the element id.
    #[must_use]
    pub fn at(mut self, element: impl Into<String>) -> Self {
        self.element = Some(element.into());
        self
    }
}

/// Collects diagnostics, dropping exact duplicates.
#[derive(Debug, Default, Clone)]
pub struct DiagSink {
    items: Vec<Diagnostic>,
}

impl DiagSink {
    /// Add a diagnostic unless an identical one exists.
    pub fn push(&mut self, d: Diagnostic) {
        if !self.items.contains(&d) {
            self.items.push(d);
        }
    }

    /// Consume into a list.
    #[must_use]
    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.items
    }

    /// Borrow the list.
    #[must_use]
    pub fn items(&self) -> &[Diagnostic] {
        &self.items
    }
}
