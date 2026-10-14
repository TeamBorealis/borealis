//! Batched generation: several elements of one type in ONE model call (class features, subclass
//! features across subclasses, standalone items/feats/spells, subclass roots), so an import costs a
//! small, bounded number of calls: `1 (detect) + ceil(outlined / 6) + 1 per class element +
//! sum over types of ceil(n / BATCH_SIZE)` calls, plus repair rounds.

use borealis_ai::task::prompt::fence;
use borealis_ai::task::{run_task, AiRunner, RunOptions};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::fewshot::{core_catalogue, exemplars_for};
use super::generate::{
    element_issues, guidance, parent_of, source_window, GenCtx, FULL_SOURCE_CHARS,
};
use super::normalize::normalize;
use super::steps::{ElementOut, Step};
use super::types::{ElementStatus, ImportDraft, ParentLink, PlannedElement, Role};

/// Features generated per model call.
pub(crate) const BATCH_SIZE: usize = 8;
/// Parts and options (species traits, sub-features, option lists) generated per model call.
pub(crate) const PART_BATCH_SIZE: usize = 16;
/// Sources up to this length go to every batch whole.
pub(crate) const SOURCE_CHARS: usize = 9000;

/// Why an element of a long source was generated from the whole source (its passage was not
/// found), for the import report; `None` when it had its passage or the source is short.
pub(crate) fn unlocated_warning(source: &str, p: &PlannedElement) -> Option<String> {
    let n = source.chars().count();
    let skip = p.excerpt.is_some() || p.role == Role::SubclassGate || n <= SOURCE_CHARS;
    (!skip).then(|| {
        let sent = if n > FULL_SOURCE_CHARS {
            format!("its first {FULL_SOURCE_CHARS} of {n} characters")
        } else {
            format!("all {n} characters")
        };
        format!(
            "\"{}\" was not found in the source; it was generated from {sent}",
            p.name
        )
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BatchOut {
    pub elements: Vec<ElementOut>,
}

fn batch_schema() -> Value {
    json!({"type":"object","required":["elements"],"properties":{"elements":{"type":"array",
        "items":{"type":"object","required":["element"],"properties":{
          "element":{"type":"object","required":["id","name","type"]},
          "notes":{"type":"array","items":{"type":"string"}}}}}}})
}

/// Generation phase of a role: options before the parts and features offering them, parts
/// before what grants them, features before standalone elements before subclass roots, so
/// references resolve. None when the role is generated on its own.
fn phase(role: Role) -> Option<u8> {
    match role {
        Role::Option => Some(0),
        Role::Part => Some(1),
        Role::ClassFeature | Role::SubclassFeature => Some(2),
        Role::Standalone | Role::Text => Some(3),
        Role::Subclass => Some(4),
        Role::Class | Role::SubclassGate => None,
    }
}

/// Phases batched even when alone (features, parts, options).
const ALWAYS_BATCHED: u8 = 2;

/// Elements per call of a phase: parts and options are short, so more fit in one answer.
fn batch_size(phase: u8) -> usize {
    if phase < 2 {
        PART_BATCH_SIZE
    } else {
        BATCH_SIZE
    }
}

/// Pending elements grouped into batches of one type: options, parts and features always
/// (class features per class, the rest across owners), standalone elements and subclass roots
/// whenever there are at least two of a type (a lone one keeps the single-element call). A
/// located text-only element is built from its passage without the model.
pub(crate) fn plan_batches(draft: &ImportDraft) -> Vec<Vec<PlannedElement>> {
    type Key = (u8, Option<String>, String);
    let mut groups: Vec<(Key, Vec<PlannedElement>)> = Vec::new();
    for p in &draft.plan.elements {
        let pending = draft
            .element(&p.id)
            .is_some_and(|e| e.status == ElementStatus::Pending);
        let built = p.role == Role::Text && p.excerpt.is_some();
        let Some(ph) = phase(p.role).filter(|_| pending && !built) else {
            continue;
        };
        let owner = (p.role == Role::ClassFeature)
            .then(|| p.owner.clone())
            .flatten();
        let key = (ph, owner, p.ty.clone());
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => v.push(p.clone()),
            None => groups.push((key, vec![p.clone()])),
        }
    }
    groups.sort_by_key(|((ph, _, _), _)| *ph);
    // In a long source a batch either sends the located passages of all its elements or, for
    // elements whose passage was not found, the whole source: never both in one batch.
    let long = draft.source_text.chars().count() > SOURCE_CHARS;
    groups
        .into_iter()
        .filter(|((ph, _, _), v)| *ph <= ALWAYS_BATCHED || v.len() > 1)
        .flat_map(|((ph, _, _), v)| {
            let (located, unlocated): (Vec<_>, Vec<_>) =
                v.into_iter().partition(|p| !long || p.excerpt.is_some());
            [located, unlocated]
                .into_iter()
                .flat_map(|part| {
                    part.chunks(batch_size(ph))
                        .map(<[_]>::to_vec)
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The source text a batch needs: the whole source when it fits, else the passages located for
/// its elements (in source order), else (no passage located) the whole source up to
/// [`FULL_SOURCE_CHARS`] (reported, see [`unlocated_warning`]).
fn batch_source(draft: &ImportDraft, batch: &[PlannedElement]) -> String {
    let all = &draft.source_text;
    if all.chars().count() <= SOURCE_CHARS {
        return all.clone();
    }
    if batch.iter().any(|p| p.excerpt.is_none()) {
        return all.chars().take(FULL_SOURCE_CHARS).collect();
    }
    let mut parts: Vec<(usize, String)> = batch
        .iter()
        .filter_map(|p| {
            p.excerpt
                .as_ref()
                .map(|e| (e.start_line, source_window(all, p)))
        })
        .collect();
    parts.sort();
    parts.dedup();
    let joined: Vec<String> = parts.into_iter().map(|(_, t)| t).collect();
    joined.join("\n...\n").chars().take(SOURCE_CHARS).collect()
}

fn prompt(g: &GenCtx, batch: &[PlannedElement], prior: &str) -> String {
    let ty = &batch[0].ty;
    let prefer: Vec<&str> = g.parent.iter().map(|p| p.class_name.as_str()).collect();
    let shots = exemplars_for(g.store, g.draft.edition, ty, false, &prefer);
    let shots = serde_json::to_string_pretty(&shots).unwrap_or_default();
    let list: String = batch
        .iter()
        .map(|p| {
            format!(
                "- id \"{}\": \"{}\"{} - {}\n",
                p.id,
                p.name,
                p.level.map(|l| format!(" (level {l})")).unwrap_or_default(),
                p.brief
            )
        })
        .collect();
    let src = batch_source(g.draft, batch);
    let each = if matches!(batch[0].role, Role::ClassFeature | Role::SubclassFeature) {
        "Each: `description` in HTML paragraphs restating the source in your own words, and `rules` that implement it \
(resources, grants, selects, stat changes); a feature with no mechanics may have no rules. Reuse existing core \
ids. Spellcasting, slots, cantrips and progression tables are generated for you: describe them in prose only."
    } else {
        guidance(&batch[0], g.draft.edition)
    };
    format!(
        "Write ALL of these {n} elements of type {ty} (edition {ed:?}) in ONE answer, each with EXACTLY its id:\n{list}\n\
{each}\n\n\
Real examples of the same kind (copy the SHAPE, not the content):\n{shots}\n\n\
Existing core ids you must reuse instead of inventing new ones:\n{cat}\n\n{kinds}\n\n\
Answer {{\"elements\": [{{\"element\": {{...}}, \"notes\": [...]}}, ...]}}, one entry per id above; put doubts in notes.{prior}\n\n{src}",
        n = batch.len(),
        ed = g.draft.edition,
        cat = core_catalogue(g.store),
        kinds = super::rules::catalogue(),
        src = fence("user_input", &src),
    )
}

/// Generate one batch. Returns the normalized elements with their notes and the attempts used.
pub(crate) async fn run_batch(
    runner: &dyn AiRunner,
    g: &GenCtx<'_>,
    batch: &[PlannedElement],
    opts: &RunOptions<'_>,
) -> Result<(Vec<(Value, Vec<String>)>, u32), String> {
    let ids: Vec<String> = batch.iter().map(|p| p.id.clone()).collect();
    let siblings: Vec<Value> = g
        .draft
        .elements
        .iter()
        .filter(|e| !ids.contains(&e.id) && !e.json.is_null() && e.status != ElementStatus::Failed)
        .map(|e| e.json.clone())
        .collect();
    let (ctx, plan, planned) = (g.ctx.clone(), g.draft.plan.clone(), batch.to_vec());
    let parents: Vec<Option<ParentLink>> = batch.iter().map(|p| parent_of(g.draft, p)).collect();
    let source_name = g.draft.source_name.clone();
    let check_parents = parents.clone();
    let step = Step::<BatchOut>::new(
        "import_generate",
        "write several content elements for the Borealis engine",
        "Output {\"elements\": [{\"element\": {...}, \"notes\": [...]}]} only, one per requested id.",
        prompt(g, batch, ""),
        batch_schema(),
    )
    .with_check(move |out: &BatchOut| {
        let mut issues = Vec::new();
        for (p, parent) in planned.iter().zip(&check_parents) {
            let Some(o) = out
                .elements
                .iter()
                .find(|o| o.element.get("id").and_then(Value::as_str) == Some(p.id.as_str()))
                .or_else(|| {
                    out.elements.iter().find(|o| {
                        o.element.get("name").and_then(Value::as_str) == Some(p.name.as_str())
                    })
                })
            else {
                issues.push(format!("missing element \"{}\" ({})", p.name, p.id));
                continue;
            };
            let mut el = o.element.clone();
            normalize(&mut el, p, &plan, parent.as_ref(), &source_name);
            issues.extend(
                element_issues(&ctx, &siblings, &el, p, &plan)
                    .into_iter()
                    .map(|i| format!("\"{}\": {i}", p.name)),
            );
        }
        issues
    });
    let res = run_task(runner, &step, opts)
        .await
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (p, parent) in batch.iter().zip(&parents) {
        let found = res
            .output
            .elements
            .iter()
            .find(|o| o.element.get("id").and_then(Value::as_str) == Some(p.id.as_str()))
            .or_else(|| {
                res.output.elements.iter().find(|o| {
                    o.element.get("name").and_then(Value::as_str) == Some(p.name.as_str())
                })
            })
            .ok_or_else(|| format!("the model omitted \"{}\"", p.name))?;
        let mut el = found.element.clone();
        normalize(
            &mut el,
            p,
            &g.draft.plan,
            parent.as_ref(),
            &g.draft.source_name,
        );
        out.push((el, found.notes.clone()));
    }
    Ok((out, res.attempts))
}

/// The parent link shared by a batch.
pub(crate) fn batch_parent(
    draft: &ImportDraft,
    batch: &[PlannedElement],
) -> Option<super::types::ParentLink> {
    parent_of(draft, &batch[0])
}
