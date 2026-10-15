//! Regenerate one element: the generation path of the first import (one single-element call from
//! the element's source passage, then the same validation, repair and source checks), not the
//! revise path that edits the element's current JSON.

use borealis_ai::task::RunOptions;
use borealis_content::ContentStoreBuilder;

use super::generate::{
    emit, generate_single_once, parent_of, push_new, validate_and_repair, GenCtx, Progress,
};
use super::normalize::normalize;
use super::selfcheck;
use super::steps::{Attaching, VisionRunner};
use super::types::{ElementStatus, ImportDraft, Role, SourceExcerpt, Stage};
use super::{ctx_of, parts};
use crate::error::AssistError;

/// Longest snippet stored with a re-located passage.
const SNIPPET_CHARS: usize = 400;

/// `draft` with element `element_id` (and its plan entry) pointing at source lines
/// `start..=end` (1-based, inclusive).
///
/// # Errors
/// [`AssistError::Invalid`] for a range outside the source, an unknown element, or the
/// generated subclass choice feature.
pub fn point_element_at(
    draft: &ImportDraft,
    element_id: &str,
    start: usize,
    end: usize,
) -> Result<ImportDraft, AssistError> {
    let lines: Vec<&str> = draft.source_text.lines().collect();
    if start == 0 || start > end || end > lines.len() {
        return Err(AssistError::Invalid(
            "select a passage inside the source text".into(),
        ));
    }
    let el = draft
        .element(element_id)
        .ok_or_else(|| AssistError::Invalid(format!("no element {element_id} in the draft")))?;
    if el.role == Role::SubclassGate {
        return Err(AssistError::Invalid(
            "the subclass choice is built from the class; use the class instead".into(),
        ));
    }
    let excerpt = SourceExcerpt {
        start_line: start,
        end_line: end,
        snippet: lines[start - 1..end]
            .join("\n")
            .chars()
            .take(SNIPPET_CHARS)
            .collect(),
    };
    let mut next = draft.clone();
    for e in next.elements.iter_mut().filter(|e| e.id == element_id) {
        e.excerpt = Some(excerpt.clone());
    }
    for p in next.plan.elements.iter_mut().filter(|p| p.id == element_id) {
        p.excerpt = Some(excerpt.clone());
    }
    Ok(next)
}

/// Generate exactly one element again, from source lines `range` (1-based, inclusive; `None` =
/// its current passage). The model gets the single-element prompt with no previous answer, then
/// the element goes through the draft's validation and repair and its source checks, as in the
/// first import. Other elements are never regenerated (a repair they need is made only when the
/// sweep blames them). On a failed model call the draft is untouched and the error is returned.
///
/// # Errors
/// [`AssistError::Invalid`] for an unknown element, the generated subclass choice feature or a
/// bad range; the model's failure message when the call fails.
pub async fn regenerate_element(
    runner: &dyn VisionRunner,
    base: &ContentStoreBuilder,
    draft: &ImportDraft,
    element_id: &str,
    range: Option<(usize, usize)>,
    opts: &RunOptions<'_>,
    progress: Progress<'_>,
) -> Result<ImportDraft, AssistError> {
    let mut next = match range {
        Some((start, end)) => point_element_at(draft, element_id, start, end)?,
        None => draft.clone(),
    };
    let idx = next
        .elements
        .iter()
        .position(|e| e.id == element_id)
        .ok_or_else(|| AssistError::Invalid(format!("no element {element_id} in the draft")))?;
    if next.elements[idx].role == Role::SubclassGate {
        return Err(AssistError::Invalid(
            "the subclass choice feature is generated; regenerate the class instead".into(),
        ));
    }
    let planned = next
        .plan
        .elements
        .iter()
        .find(|p| p.id == element_id)
        .cloned()
        .ok_or_else(|| AssistError::Invalid(format!("{element_id} is not in the plan")))?;
    emit(progress, Stage::Generate, 0, 1, &planned.name);
    let adapter = Attaching {
        inner: runner,
        attachments: &[],
    };
    let ctx = ctx_of(base, next.edition);
    let (json, notes, attempts, retried) =
        if planned.role == Role::Text && planned.excerpt.is_some() {
            // Rules and lore keep the passage as it is: no model call.
            let mut el = parts::text_element(&planned, &next.plan, &next.source_text);
            normalize(&mut el, &planned, &next.plan, None, &next.source_name);
            (el, Vec::new(), 1, Vec::new())
        } else {
            let store = ctx.base.clone().build();
            let g = GenCtx {
                ctx: &ctx,
                store: &store,
                draft: &next,
                parent: parent_of(&next, &planned),
            };
            generate_single_once(&adapter, &g, &planned, opts, progress)
                .await
                .map_err(AssistError::Invalid)?
        };
    let e = &mut next.elements[idx];
    e.json = json;
    e.revisions += 1;
    e.repair_rounds = 0;
    e.balance = None;
    e.checks.clear();
    e.rule_errors.clear();
    e.normalised.clear();
    push_new(&mut e.normalised, retried);
    e.status = match (notes.is_empty(), attempts > 1) {
        (false, _) => ElementStatus::NeedsReview,
        (true, true) => ElementStatus::Repaired,
        (true, false) => ElementStatus::Ok,
    };
    e.issues = notes;
    validate_and_repair(&adapter, &ctx, &mut next, opts, progress).await;
    selfcheck::heal_one(&adapter, &ctx, &mut next, element_id, opts, progress).await;
    Ok(next)
}

#[cfg(test)]
#[path = "regenerate_tests.rs"]
mod tests;
