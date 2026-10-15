//! Stage 4: the engine sweep, the rule catalogue and targeted repairs of an element, then the
//! balance flags.

use std::collections::BTreeSet;

use borealis_ai::task::{AiRunner, RunOptions};
use borealis_content::{ContentStore, Element};

use super::super::sweep::{engine_sweep, SweepIssue};
use super::super::types::{ElementStatus, ImportDraft, Role, Stage};
use super::{emit, parent_of, run_element, GenCtx, Progress, MAX_REPAIR_ROUNDS};
use crate::balance::{balance_report, Baselines};
use crate::homebrew::PackContext;

/// Stage 4: engine sweep with up to [`MAX_REPAIR_ROUNDS`] targeted repairs per element, then the
/// balance flags. Unresolved sweep problems mark their element `Failed`.
pub(crate) async fn validate_and_repair(
    runner: &dyn AiRunner,
    ctx: &PackContext,
    draft: &mut ImportDraft,
    opts: &RunOptions<'_>,
    progress: Progress<'_>,
) {
    validate_rounds(runner, ctx, draft, opts, progress, MAX_REPAIR_ROUNDS).await;
}

/// [`validate_and_repair`] with at most `max_rounds` repair rounds; with 0 it makes no model
/// call (a re-validation after the source checks' repairs) and keeps the repair counts.
pub(crate) async fn validate_rounds(
    runner: &dyn AiRunner,
    ctx: &PackContext,
    draft: &mut ImportDraft,
    opts: &RunOptions<'_>,
    progress: Progress<'_>,
    max_rounds: u32,
) {
    let store = ctx.base.clone().build();
    for e in &mut draft.elements {
        if e.status == ElementStatus::Failed && !e.json.is_null() {
            e.status = ElementStatus::Repaired;
            e.issues.clear();
        }
        if max_rounds > 0 {
            e.repair_rounds = 0;
        }
    }
    keep_printed(draft);
    let mut remaining = Vec::new();
    let mut remaining_rules = Vec::new();
    for round in 0..=max_rounds {
        tidy_rules(draft);
        let issues = engine_sweep(ctx, &draft.plan, &draft.elements);
        let mut rule_issues = rule_sweep(draft);
        if round == 0 {
            // Mechanics left in the text get one repair, never more.
            rule_issues.extend(super::super::mechanics::sweep(draft));
            // So do description sentences the source does not have.
            rule_issues.extend(super::super::fidelity::sweep(draft));
        }
        emit(
            progress,
            Stage::Validate,
            round as usize,
            max_rounds as usize + 1,
            &format!(
                "{} sweep issue(s), {} rule issue(s)",
                issues.len(),
                rule_issues.len()
            ),
        );
        let clean = issues.is_empty() && rule_issues.is_empty();
        if clean || round == max_rounds {
            remaining = issues;
            remaining_rules = rule_issues;
            break;
        }
        let all: Vec<SweepIssue> = issues.iter().chain(&rule_issues).cloned().collect();
        let blamed = blamed_elements(draft, &all);
        let mut repaired_any = false;
        for (id, msgs) in blamed {
            let Some(idx) = draft.elements.iter().position(|e| e.id == id) else {
                continue;
            };
            let e = &draft.elements[idx];
            if e.json.is_null()
                || e.repair_rounds >= MAX_REPAIR_ROUNDS
                || e.role == Role::SubclassGate
            {
                continue;
            }
            let Some(planned) = draft.plan.elements.iter().find(|p| p.id == id).cloned() else {
                continue;
            };
            let parent = parent_of(draft, &planned);
            let prev = e.json.clone();
            let result = {
                let g = GenCtx {
                    ctx,
                    store: &store,
                    draft,
                    parent,
                };
                run_element(
                    runner,
                    &g,
                    &planned,
                    "import_repair",
                    Some((&prev, &msgs)),
                    None,
                    opts,
                )
                .await
            };
            let e = &mut draft.elements[idx];
            e.repair_rounds += 1;
            repaired_any = true;
            if let Ok((el, notes, _)) = result {
                e.json = el;
                e.status = ElementStatus::Repaired;
                e.issues = notes;
            }
        }
        if !repaired_any {
            remaining = issues;
            remaining_rules = rule_issues;
            break;
        }
    }
    strip_unrepaired_rules(draft, &remaining_rules);
    mark_unresolved(draft, &remaining);
    draft.sweep_issues = remaining;
    super::super::mechanics::note_leftovers(draft);
    keep_printed(draft);
    super::super::fidelity::note_leftovers(draft);
    attach_balance(ctx, &store, draft);
}

/// Deterministic description fixes: a description that only rewords its element's own printed
/// passage becomes the passage, a class or subclass root gets its unplanned prose
/// sub-sections, and a table row loses sentences the source does not print.
pub(crate) fn keep_printed(draft: &mut ImportDraft) {
    super::super::passage::restore(draft);
    super::super::passage::add_sub_sections(draft);
    super::super::passage::drop_unprinted_row_text(draft);
}

/// Deterministic rule fixes on every generated element (recorded on the element): unknown
/// fields of complete rules removed, resources and references normalised.
fn tidy_rules(draft: &mut ImportDraft) {
    for e in draft.elements.iter_mut().filter(|e| !e.json.is_null()) {
        let mut notes = super::super::rules::strip_unknown_fields(&mut e.json);
        notes.extend(super::super::rules::normalise_resources(&mut e.json));
        push_new(&mut e.normalised, notes);
    }
    let known: Vec<(String, String)> = draft
        .elements
        .iter()
        .flat_map(|e| super::super::rules::resources_of(&e.json))
        .collect();
    for e in draft.elements.iter_mut().filter(|e| !e.json.is_null()) {
        let notes = super::super::rules::normalise_references(&mut e.json, &known);
        push_new(&mut e.normalised, notes);
    }
}

pub(crate) fn push_new(into: &mut Vec<String>, items: Vec<String>) {
    for i in items {
        if !into.contains(&i) {
            into.push(i);
        }
    }
}

/// Rule-catalogue problems of every generated element, as repairable issues.
fn rule_sweep(draft: &ImportDraft) -> Vec<SweepIssue> {
    draft
        .elements
        .iter()
        .filter(|e| !e.json.is_null() && e.status != ElementStatus::Failed)
        .flat_map(|e| {
            super::super::rules::rule_problems(&e.json)
                .into_iter()
                .map(|message| SweepIssue {
                    level: 0,
                    element_id: Some(e.id.clone()),
                    message,
                })
        })
        .collect()
}

/// Last resort after the repair rounds: drop what the engine would ignore, and say so on the
/// element (it needs review; the import report lists it as an error).
fn strip_unrepaired_rules(draft: &mut ImportDraft, remaining: &[SweepIssue]) {
    for e in &mut draft.elements {
        if !remaining
            .iter()
            .any(|i| i.element_id.as_deref() == Some(&e.id))
        {
            continue;
        }
        let removed = super::super::rules::strip_invalid(&mut e.json);
        if removed.is_empty() {
            continue;
        }
        if matches!(e.status, ElementStatus::Ok | ElementStatus::Repaired) {
            e.status = ElementStatus::NeedsReview;
        }
        push_new(&mut e.issues, removed.clone());
        push_new(&mut e.rule_errors, removed);
    }
}

fn blamed_elements(draft: &ImportDraft, issues: &[SweepIssue]) -> Vec<(String, Vec<String>)> {
    let mut order: Vec<String> = Vec::new();
    let mut map: std::collections::BTreeMap<String, BTreeSet<String>> =
        std::collections::BTreeMap::new();
    for i in issues {
        let Some(id) = i
            .element_id
            .clone()
            .filter(|id| draft.element(id).is_some())
        else {
            continue;
        };
        let id = match draft.element(&id) {
            Some(e) if e.role == Role::SubclassGate => draft
                .plan
                .elements
                .iter()
                .find(|p| p.id == id)
                .and_then(|p| p.owner.clone())
                .unwrap_or(id),
            _ => id,
        };
        if !order.contains(&id) {
            order.push(id.clone());
        }
        let msg = if i.level > 0 {
            format!("level {}: {}", i.level, i.message)
        } else {
            i.message.clone()
        };
        map.entry(id).or_default().insert(msg);
    }
    order
        .into_iter()
        .map(|id| {
            let m = map.remove(&id).unwrap_or_default().into_iter().collect();
            (id, m)
        })
        .collect()
}

fn mark_unresolved(draft: &mut ImportDraft, remaining: &[super::super::sweep::SweepIssue]) {
    for i in remaining {
        let Some(id) = &i.element_id else { continue };
        if let Some(e) = draft.elements.iter_mut().find(|e| &e.id == id) {
            e.status = ElementStatus::Failed;
            let msg = if i.level > 0 {
                format!("level {}: {}", i.level, i.message)
            } else {
                i.message.clone()
            };
            if !e.issues.contains(&msg) {
                e.issues.push(msg);
            }
        }
    }
}

fn attach_balance(ctx: &PackContext, store: &ContentStore, draft: &mut ImportDraft) {
    let baselines = Baselines::from_store(store, ctx.ruleset);
    for e in &mut draft.elements {
        e.balance = None;
        if e.json.is_null()
            || matches!(
                e.role,
                Role::ClassFeature
                    | Role::SubclassFeature
                    | Role::SubclassGate
                    | Role::Part
                    | Role::Option
                    | Role::Text
            )
        {
            continue;
        }
        if let Ok(el) = serde_json::from_value::<Element>(e.json.clone()) {
            let report = balance_report(&baselines, &el);
            if report
                .flags
                .iter()
                .any(|f| f.severity != crate::balance::Severity::Ok)
            {
                e.balance = Some(report);
            }
        }
    }
}
