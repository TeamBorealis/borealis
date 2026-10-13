//! Wire types of the `import_*` commands (`ui/src/api/types/import.ts`) and the mapping from the
//! pipeline's [`ImportDraft`].

use crate::import_check::balance_lines;
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
        match self.edition.as_deref() {
            Some("dnd2014" | "2014") => Edition::Y2014,
            _ => Edition::Y2024,
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

/// An element as shown in review.
#[must_use]
pub fn element_dto(e: &DraftElement) -> ImportElementDto {
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
    }
}

/// The whole draft as shown in review.
#[must_use]
pub fn draft_dto(id: &str, stage: &str, d: &ImportDraft) -> ImportDraftDto {
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
        elements: d.elements.iter().map(element_dto).collect(),
        sweep: SweepDto {
            levels_passed,
            errors: sweep_errors,
        },
        suggested_pack_name: d.pack_name.clone(),
        suggested_prefix: d.id_prefix.clone(),
    }
}
