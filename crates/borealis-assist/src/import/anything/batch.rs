//! Batched generation of features: several class or subclass features in ONE model call, so a
//! class import costs a small, bounded number of calls. Bound: `2 (detect, outline) + 1 (class
//! element) + ceil(features / BATCH_SIZE)` calls, plus one per subclass and repair rounds.

use borealis_ai::task::prompt::fence;
use borealis_ai::task::{run_task, AiRunner, RunOptions};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::fewshot::{core_catalogue, exemplars_for};
use super::generate::{element_issues, parent_of, GenCtx};
use super::normalize::normalize;
use super::steps::{ElementOut, Step};
use super::types::{ElementStatus, ImportDraft, PlannedElement, Role};

/// Features generated per model call.
pub(crate) const BATCH_SIZE: usize = 8;
const SOURCE_CHARS: usize = 9000;

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

/// Pending feature elements grouped into batches (same owner and type, at most [`BATCH_SIZE`]).
pub(crate) fn plan_batches(draft: &ImportDraft) -> Vec<Vec<PlannedElement>> {
    type Key = (Option<String>, String);
    let mut groups: Vec<(Key, Vec<PlannedElement>)> = Vec::new();
    for p in &draft.plan.elements {
        let pending = draft
            .element(&p.id)
            .is_some_and(|e| e.status == ElementStatus::Pending);
        if !pending || !matches!(p.role, Role::ClassFeature | Role::SubclassFeature) {
            continue;
        }
        let key = (p.owner.clone(), p.ty.clone());
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => v.push(p.clone()),
            None => groups.push((key, vec![p.clone()])),
        }
    }
    groups
        .into_iter()
        .flat_map(|(_, v)| v.chunks(BATCH_SIZE).map(<[_]>::to_vec).collect::<Vec<_>>())
        .collect()
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
    let src: String = g.draft.source_text.chars().take(SOURCE_CHARS).collect();
    format!(
        "Write ALL of these {n} elements of type {ty} (edition {ed:?}) in ONE answer, each with EXACTLY its id:\n{list}\n\
Each: `description` in HTML paragraphs restating the source in your own words, and `rules` that implement it \
(resources, grants, selects, stat changes); a feature with no mechanics may have no rules. Reuse existing core \
ids. Spellcasting, slots, cantrips and progression tables are generated for you: describe them in prose only.\n\n\
Real examples of the same kind (copy the SHAPE, not the content):\n{shots}\n\n\
Existing core ids you must reuse instead of inventing new ones:\n{cat}\n\n\
Answer {{\"elements\": [{{\"element\": {{...}}, \"notes\": [...]}}, ...]}}, one entry per id above; put doubts in notes.{prior}\n\n{src}",
        n = batch.len(),
        ed = g.draft.edition,
        cat = core_catalogue(g.store),
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
    let (parent, source_name) = (g.parent.clone(), g.draft.source_name.clone());
    let step = Step::<BatchOut>::new(
        "import_generate",
        "write several content elements for the Borealis engine",
        "Output {\"elements\": [{\"element\": {...}, \"notes\": [...]}]} only, one per requested id.",
        prompt(g, batch, ""),
        batch_schema(),
    )
    .with_check(move |out: &BatchOut| {
        let mut issues = Vec::new();
        for p in &planned {
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
    for p in batch {
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
            g.parent.as_ref(),
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
