//! Contract DTOs for equipment requests, item rows, sheet and compendium payloads, traces.

use crate::dto::{ElementSummary, Ruleset};
use borealis_engine::{Diagnostic, Severity, StatTrace, StepKind, TraceStep};
use serde::{Deserialize, Serialize};

/// Where the UI wants an item equipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum EquipRequest {
    /// Let the engine pick.
    #[serde(rename = "auto")]
    Auto,
    /// Armor or a worn item.
    #[serde(rename = "worn")]
    Worn,
    /// Main hand.
    #[serde(rename = "primary")]
    Primary,
    /// Off hand.
    #[serde(rename = "secondary")]
    Secondary,
    /// Both hands (two-handed weapon).
    #[serde(rename = "two-handed")]
    TwoHanded,
    /// A versatile weapon held in both hands.
    #[serde(rename = "versatile")]
    Versatile,
}

/// Editable item fields.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ItemPatch {
    /// Alternative display name.
    pub display_name: Option<String>,
    /// Free notes.
    pub notes: Option<String>,
    /// Show in the inventory table.
    pub list_in_inventory: Option<bool>,
    /// Print an item card ("show card").
    pub show_card: Option<bool>,
}

/// Item search filter (the `characterId` restricts to the character's ruleset and sources).
#[derive(Debug, Clone, Default)]
pub struct ItemFilter {
    /// Category setter value, e.g. `Weapons`.
    pub category: Option<String>,
    /// Name text.
    pub query: Option<String>,
    /// Source name.
    pub source: Option<String>,
    /// Page size.
    pub limit: Option<usize>,
    /// Page start.
    pub offset: Option<usize>,
    /// Character whose ruleset and source restrictions apply.
    pub character_id: Option<String>,
}

/// A catalogue item row.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemSummary {
    #[serde(flatten)]
    /// The element row.
    pub summary: ElementSummary,
    /// Category (e.g. `Weapons`).
    pub category: String,
    /// Price in copper pieces.
    pub cost_cp: u64,
    /// Weight in pounds.
    pub weight: f64,
    /// Needs attunement.
    pub requires_attunement: bool,
}

/// Sheet export request options.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SheetOptionsDto {
    /// `pdf` (default) or `html`.
    pub format: Option<String>,
}

/// HTML preview of the sheet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetPreview {
    /// Self-contained document.
    pub html: String,
    /// Number of pages.
    pub page_count: usize,
    /// RFC 3339 time.
    pub generated_at: String,
}

/// Compendium search request.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompendiumQuery {
    /// Free text.
    #[serde(default)]
    pub query: String,
    /// Contract element types.
    #[serde(default)]
    pub types: Option<Vec<String>>,
    /// Source names.
    #[serde(default)]
    pub sources: Option<Vec<String>>,
    /// `dnd2014`, `dnd2024` or `any`.
    #[serde(default)]
    pub ruleset: Option<String>,
    /// Page size.
    #[serde(default)]
    pub limit: usize,
    /// Page start.
    #[serde(default)]
    pub offset: usize,
}

/// One facet value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Facet {
    /// Filter id.
    pub id: String,
    /// Display label.
    pub label: String,
    /// Elements matching.
    pub count: usize,
}

/// Compendium filter facets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompendiumFacets {
    /// Element types present.
    pub types: Vec<Facet>,
    /// Sources present.
    pub sources: Vec<Facet>,
}

/// A label/value fact line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ElementFact {
    /// Label.
    pub label: String,
    /// Value.
    pub value: String,
}

/// Full element for the detail pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementDetail {
    #[serde(flatten)]
    /// The list row.
    pub summary: ElementSummary,
    /// Sanitised description.
    pub description_html: String,
    /// Fact lines.
    pub facts: Vec<ElementFact>,
    /// What choosing the element grants.
    pub grants: Vec<String>,
}

/// A diagnostic touching a trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagnosticDto {
    /// Machine code.
    pub code: String,
    /// `info`, `warning` or `error`.
    pub severity: String,
    /// Message.
    pub message: String,
    /// Element concerned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element: Option<String>,
}

/// One step of a statistic trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceStepDto {
    /// Group.
    pub group: String,
    /// `seed`, `derived` or `rule`.
    pub kind: String,
    /// Label.
    pub label: String,
    /// Owning element id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_id: Option<String>,
    /// Owning element name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_name: Option<String>,
    /// Rule kind.
    pub rule_kind: String,
    /// Contributed value.
    pub value: i32,
    /// Value as written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_value: Option<String>,
    /// Stacking label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bonus: Option<String>,
    /// Conditions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Counted in the total.
    pub included: bool,
    /// Why not (or a note).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Steps of a referenced statistic.
    pub inputs: Vec<TraceStepDto>,
}

/// The provenance trace of one statistic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatTraceDto {
    /// Statistic name.
    pub stat: String,
    /// Sum of the seed steps.
    pub base: i32,
    /// Steps in evaluation order.
    pub steps: Vec<TraceStepDto>,
    /// Final value.
    pub value: i32,
    /// Diagnostics touching the trace.
    pub diagnostics: Vec<DiagnosticDto>,
}

impl From<&TraceStep> for TraceStepDto {
    fn from(s: &TraceStep) -> Self {
        Self {
            group: s.group.clone(),
            kind: match s.kind {
                StepKind::Seed => "seed",
                StepKind::Derived => "derived",
                StepKind::Rule => "rule",
            }
            .into(),
            label: s.label.clone(),
            element_id: s.element_id.clone(),
            element_name: s.element_name.clone(),
            rule_kind: s.rule_kind.clone(),
            value: s.value,
            raw_value: s.raw_value.clone(),
            bonus: s.bonus.clone(),
            condition: s.condition.clone(),
            included: s.included,
            reason: s.reason.clone(),
            inputs: s.inputs.iter().map(Self::from).collect(),
        }
    }
}

impl From<&Diagnostic> for DiagnosticDto {
    fn from(d: &Diagnostic) -> Self {
        Self {
            code: d.code.clone(),
            severity: match d.severity {
                Severity::Info => "info",
                Severity::Warning => "warning",
                Severity::Error => "error",
            }
            .into(),
            message: d.message.clone(),
            element: d.element.clone(),
        }
    }
}

impl From<&StatTrace> for StatTraceDto {
    fn from(t: &StatTrace) -> Self {
        Self {
            stat: t.stat.clone(),
            base: t.base,
            steps: t.steps.iter().map(TraceStepDto::from).collect(),
            value: t.value,
            diagnostics: t.diagnostics.iter().map(DiagnosticDto::from).collect(),
        }
    }
}

/// Does an element of `ruleset` belong in a listing for `wanted`?
#[must_use]
pub fn ruleset_matches(element: Option<Ruleset>, wanted: Ruleset) -> bool {
    element.map_or(true, |r| r.compatible_with(wanted))
}
