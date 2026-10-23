//! Wire types of the `import_*` commands (`ui/src/api/types/import.ts`) and the mapping from the
//! pipeline's [`ImportDraft`].

use crate::import_check::balance_lines;
use borealis_assist::import::anything::review::{
    import_level_notes, review_all, source_passage, ReviewCounts, ReviewLevel, ReviewOverrides,
    ReviewState,
};
use borealis_assist::import::anything::{DraftElement, ElementStatus, ImportKind, Role};
use borealis_assist::{Edition, ImportDraft};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Event name of the import stream.
pub const EVENT_IMPORT: &str = "borealis://import-event";

/// Optional hints from the UI.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportHintDto {
    /// Expected kind (informational).
    pub kind: Option<String>,
    /// `dnd2014` or `dnd2024`.
    pub edition: Option<String>,
    /// Parent class of a subclass, when the user already knows it.
    pub parent_class_id: Option<String>,
}

impl ImportHintDto {
    /// The edition hint (2024 when absent).
    #[must_use]
    pub fn edition(&self) -> Edition {
        self.edition_override().unwrap_or(Edition::Y2024)
    }

    /// The edition the user picked explicitly (none for Auto), which wins over detection.
    #[must_use]
    pub fn edition_override(&self) -> Option<Edition> {
        match self.edition.as_deref() {
            Some("dnd2014" | "2014") => Some(Edition::Y2014),
            Some("dnd2024" | "2024") => Some(Edition::Y2024),
            _ => None,
        }
    }
}

/// `import_start` result.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportStarted {
    /// Id to pass to the other commands.
    pub draft_id: String,
}

/// One element in review.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportElementDto {
    pub id: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    pub status: &'static str,
    pub source_excerpt: String,
    pub json: Value,
    pub balance_flags: Vec<String>,
    pub validation_issues: Vec<String>,
    pub review: ElementReviewDto,
    pub source_passage: Option<SourcePassageDto>,
}

/// The element's review state (see `borealis_assist::import::anything::review`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementReviewDto {
    /// `ok`, `check` or `not_imported`.
    pub state: ReviewLevel,
    pub code: String,
    pub reason: String,
    pub details: Vec<String>,
    pub text_only: bool,
    pub text_only_reason: String,
    pub text_only_override: Option<bool>,
    pub accepted: bool,
}

impl From<ReviewState> for ElementReviewDto {
    fn from(s: ReviewState) -> Self {
        Self {
            state: s.state,
            code: s.code,
            reason: s.reason,
            details: s.details,
            text_only: s.text_only,
            text_only_reason: s.text_only_reason,
            text_only_override: s.text_only_override,
            accepted: s.accepted,
        }
    }
}

/// Where an element comes from: 1-based inclusive lines of `ImportDraftDto::source_lines`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcePassageDto {
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
}

/// How many elements are in each review state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCountsDto {
    pub ok: usize,
    pub check: usize,
    pub not_imported: usize,
    pub text_only: usize,
    pub accepted: usize,
}

impl From<ReviewCounts> for ReviewCountsDto {
    fn from(c: ReviewCounts) -> Self {
        Self {
            ok: c.ok,
            check: c.check,
            not_imported: c.not_imported,
            text_only: c.text_only,
            accepted: c.accepted,
        }
    }
}

/// A failed check on the import as a whole, for a banner above the list.
#[derive(Debug, Clone, Serialize)]
pub struct ImportNoteDto {
    pub code: String,
    pub summary: String,
    pub items: Vec<String>,
}

/// A parent class the user can pick.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentCandidateDto {
    pub class_id: String,
    pub name: String,
}

/// What the model recognised.
#[derive(Debug, Clone, Serialize)]
pub struct DetectedDto {
    pub kinds: Vec<String>,
    pub edition: &'static str,
    pub names: Vec<String>,
    pub confidence: f64,
}

/// Engine sweep summary.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SweepDto {
    pub levels_passed: u32,
    pub errors: Vec<String>,
}

/// `import_get_draft` result.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDraftDto {
    pub draft_id: String,
    pub stage: String,
    pub detected: DetectedDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_choice: Option<ParentChoiceDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_class_id: Option<String>,
    pub elements: Vec<ImportElementDto>,
    pub sweep: SweepDto,
    pub suggested_pack_name: String,
    pub suggested_prefix: String,
    pub source_lines: Vec<String>,
    pub review_counts: ReviewCountsDto,
    /// Failed import-level source checks (e.g. headings that were not imported); empty when none.
    pub import_notes: Vec<ImportNoteDto>,
}

/// Open parent choice.
#[derive(Debug, Clone, Serialize)]
pub struct ParentChoiceDto {
    pub candidates: Vec<ParentCandidateDto>,
}

/// A progress event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportEvent {
    pub draft_id: String,
    pub stage: &'static str,
    pub progress: f64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element: Option<ImportElementDto>,
}

/// Receives import events (Tauri emitter in the app, a recorder in tests).
pub trait ImportSink: Send + Sync {
    /// Deliver one event.
    fn import_event(&self, event: &ImportEvent);
}

fn status_of(s: ElementStatus) -> &'static str {
    match s {
        ElementStatus::Ok => "ok",
        ElementStatus::Repaired => "repaired",
        ElementStatus::Failed => "failed",
        ElementStatus::NeedsReview | ElementStatus::Pending => "needs_review",
    }
}

fn kind_name(k: ImportKind) -> String {
    serde_json::to_value(k)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn flags_of(e: &DraftElement) -> Vec<String> {
    e.balance.as_ref().map(balance_lines).unwrap_or_default()
}

/// An element as shown in review, with its review state.
#[must_use]
pub fn element_dto(e: &DraftElement, d: &ImportDraft, review: ReviewState) -> ImportElementDto {
    let mut issues = e.issues.clone();
    if e.status == ElementStatus::Pending {
        issues.push("Not generated yet.".into());
    }
    ImportElementDto {
        id: e.id.clone(),
        ty: e.ty.clone(),
        name: e.name.clone(),
        level: e.level,
        status: status_of(e.status),
        source_excerpt: e
            .excerpt
            .as_ref()
            .map(|x| x.snippet.clone())
            .unwrap_or_default(),
        json: e.json.clone(),
        balance_flags: flags_of(e),
        validation_issues: issues,
        review: review.into(),
        source_passage: source_passage(e, d).map(|(start_line, end_line, text)| SourcePassageDto {
            start_line,
            end_line,
            text,
        }),
    }
}

/// The whole draft as shown in review, with the user's review decisions applied.
#[must_use]
pub fn draft_dto(
    id: &str,
    stage: &str,
    d: &ImportDraft,
    review: &ReviewOverrides,
) -> ImportDraftDto {
    let states = review_all(d, review);
    let counts = ReviewCounts::of(&states);
    let mut kinds: Vec<String> = Vec::new();
    for item in &d.detection.items {
        let k = kind_name(item.kind);
        if !kinds.contains(&k) {
            kinds.push(k);
        }
    }
    let choice = d
        .plan
        .open_choices()
        .into_iter()
        .next()
        .map(|c| ParentChoiceDto {
            candidates: c
                .candidates
                .iter()
                .map(|p| ParentCandidateDto {
                    class_id: p.link.class_id.clone(),
                    name: p.link.class_name.clone(),
                })
                .collect(),
        });
    let sweep_errors: Vec<String> = d
        .sweep_issues
        .iter()
        .map(|i| {
            if i.level == 0 {
                i.message.clone()
            } else {
                format!("Level {}: {}", i.level, i.message)
            }
        })
        .collect();
    let has_class = d
        .plan
        .elements
        .iter()
        .any(|p| p.role == Role::Class || p.role == Role::Subclass);
    let first_bad = d
        .sweep_issues
        .iter()
        .map(|i| i.level)
        .filter(|l| *l > 0)
        .min();
    let levels_passed = match (has_class, first_bad) {
        (false, _) => 0,
        (true, None) if d.sweep_issues.is_empty() => 20,
        (true, None) => 0,
        (true, Some(l)) => l.saturating_sub(1),
    };
    ImportDraftDto {
        draft_id: id.to_string(),
        stage: stage.to_string(),
        detected: DetectedDto {
            kinds,
            edition: match d.edition {
                Edition::Y2014 => "dnd2014",
                Edition::Y2024 => "dnd2024",
            },
            names: d.detection.items.iter().map(|i| i.name.clone()).collect(),
            confidence: d.detection.confidence,
        },
        parent_choice: choice,
        parent_class_id: d
            .plan
            .groups
            .iter()
            .find_map(|g| g.parent.as_ref().map(|p| p.class_id.clone())),
        elements: d
            .elements
            .iter()
            .zip(states)
            .map(|(e, s)| element_dto(e, d, s))
            .collect(),
        sweep: SweepDto {
            levels_passed,
            errors: sweep_errors,
        },
        suggested_pack_name: d.pack_name.clone(),
        suggested_prefix: d.id_prefix.clone(),
        source_lines: d.source_text.lines().map(str::to_string).collect(),
        review_counts: counts.into(),
        import_notes: import_level_notes(d)
            .into_iter()
            .map(|n| ImportNoteDto {
                code: n.code,
                summary: n.summary,
                items: n.items,
            })
            .collect(),
    }
}

#[cfg(test)]
mod edition_override_tests {
    use super::*;

    fn hint(edition: Option<&str>) -> ImportHintDto {
        ImportHintDto {
            edition: edition.map(str::to_string),
            ..ImportHintDto::default()
        }
    }

    #[test]
    fn auto_is_no_override_and_an_explicit_edition_is_one() {
        assert_eq!(hint(None).edition_override(), None);
        assert_eq!(hint(Some("auto")).edition_override(), None);
        assert_eq!(
            hint(Some("dnd2014")).edition_override(),
            Some(Edition::Y2014)
        );
        assert_eq!(
            hint(Some("dnd2024")).edition_override(),
            Some(Edition::Y2024)
        );
        assert_eq!(hint(None).edition(), Edition::Y2024);
        assert_eq!(hint(Some("dnd2014")).edition(), Edition::Y2014);
    }
}
