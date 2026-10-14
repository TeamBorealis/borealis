//! Batched generation: several elements of one type in ONE model call (class features, subclass
//! features across subclasses, standalone items/feats/spells, subclass roots), so an import costs a
//! small, bounded number of calls: `1 (detect) + ceil(outlined / 6) + 1 per class element +
//! sum over types of ceil(n / BATCH_SIZE)` calls, plus repair rounds.

use borealis_ai::task::prompt::fence;
use borealis_ai::task::{run_task, AiRunner, RunOptions};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::fewshot::{core_catalogue, exemplars_for};
use super::generate::{element_issues, guidance, parent_of, GenCtx};
use super::normalize::normalize;
use super::steps::{ElementOut, Step};
use super::types::{ElementStatus, ImportDraft, ParentLink, PlannedElement, Role};

/// Features generated per model call.
pub(crate) const BATCH_SIZE: usize = 8;
/// Parts and options (species traits, sub-features, option lists) generated per model call.
pub(crate) const PART_BATCH_SIZE: usize = 16;
/// Sources up to this length go to every batch whole.
pub(crate) const SOURCE_CHARS: usize = 9000;
/// Most source characters one batch carries: a batch whose elements' sections are longer is
/// split (a single longer section goes alone, whole). Sections are never cut.
pub(crate) const BATCH_SOURCE_CHARS: usize = 40_000;

/// Why an element was generated without a section of its own (the source only names it in
/// contents, index or list lines, or not at all), for the import report; `None` when it had its
/// section. Reported for short sources too: never a silent fallback.
pub(crate) fn unlocated_warning(source: &str, p: &PlannedElement) -> Option<String> {
    let skip = p.excerpt.is_some() || p.role == Role::SubclassGate;
    (!skip).then(|| super::source::unlocated_message(source, p))
}

/// The source one element of a batch is generated from: its whole section, else the passages
/// mentioning it (see [`super::source::unlocated_source`]).
fn element_source(source: &str, p: &PlannedElement) -> String {
    super::source::section_text(source, p)
        .unwrap_or_else(|| super::source::unlocated_source(source, &p.name).0)
}

/// The element has no section and is generated from the whole source.
fn needs_whole(source: &str, p: &PlannedElement) -> bool {
    p.excerpt.is_none()
        && matches!(
            super::source::unlocated_source(source, &p.name).1,
            super::source::Unlocated::Whole(_)
        )
}

/// `part` cut into batches of at most `size` elements whose sources together stay within
/// [`BATCH_SOURCE_CHARS`] (a short source is sent whole once, so only the count matters).
/// Elements generated from the whole source come first and share one copy of it per batch (a
/// copy longer than the limit goes with them alone), so they cost no extra calls.
fn split_by_size(source: &str, part: &[PlannedElement], size: usize) -> Vec<Vec<PlannedElement>> {
    if source.chars().count() <= SOURCE_CHARS {
        return part.chunks(size).map(<[PlannedElement]>::to_vec).collect();
    }
    let (whole, own): (Vec<&PlannedElement>, Vec<&PlannedElement>) =
        part.iter().partition(|p| needs_whole(source, p));
    let whole_chars = source.chars().count();
    let mut out: Vec<Vec<PlannedElement>> = Vec::new();
    let (mut cur, mut chars, mut shared): (Vec<PlannedElement>, usize, bool) =
        (Vec::new(), 0, false);
    for (p, is_whole) in whole
        .into_iter()
        .map(|p| (p, true))
        .chain(own.into_iter().map(|p| (p, false)))
    {
        let n = match (is_whole, shared) {
            (true, true) => 0,
            (true, false) => whole_chars,
            (false, _) => element_source(source, p).chars().count(),
        };
        // A whole copy over the limit is shared by whole-source elements only.
        let room = if shared && whole_chars > BATCH_SOURCE_CHARS {
            is_whole
        } else {
            chars + n <= BATCH_SOURCE_CHARS
        };
        let fits = cur.len() < size && (cur.is_empty() || room);
        if !fits {
            out.push(std::mem::take(&mut cur));
            (chars, shared) = (0, false);
        }
        let n = if is_whole && !shared { whole_chars } else { n };
        chars += n;
        shared |= is_whole;
        cur.push(p.clone());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
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
    // Every element brings its own source (section or mentions); batches are cut by count
    // and by the size of those sources, so no element's source is cut off.
    let source = &draft.source_text;
    groups
        .into_iter()
        .filter(|((ph, _, _), v)| *ph <= ALWAYS_BATCHED || v.len() > 1)
        .flat_map(|((ph, _, _), v)| split_by_size(source, &v, batch_size(ph)))
        .collect()
}

/// The source text a batch needs: the whole source when it is short, else each element's own
/// source (its section, or the passages mentioning it), labelled with its id, in source order;
/// the elements generated from the whole source share one labelled copy of it. Nothing is
/// truncated: [`plan_batches`] keeps a batch's sources within [`BATCH_SOURCE_CHARS`].
fn batch_source(draft: &ImportDraft, batch: &[PlannedElement]) -> String {
    let all = &draft.source_text;
    if all.chars().count() <= SOURCE_CHARS {
        return all.clone();
    }
    let (whole, own): (Vec<&PlannedElement>, Vec<&PlannedElement>) =
        batch.iter().partition(|p| needs_whole(all, p));
    let mut parts: Vec<(usize, &PlannedElement)> = own
        .into_iter()
        .map(|p| (p.excerpt.as_ref().map_or(usize::MAX, |e| e.start_line), p))
        .collect();
    parts.sort_by_key(|(line, _)| *line);
    let mut out: Vec<String> = Vec::new();
    if !whole.is_empty() {
        let names: Vec<String> = whole
            .iter()
            .map(|p| format!("\"{}\" (id {})", p.name, p.id))
            .collect();
        out.push(format!(
            "=== Source for {} (no section of its own: the whole source) ===\n{all}",
            names.join(", ")
        ));
    }
    out.extend(parts.into_iter().map(|(_, p)| {
        format!(
            "=== Source for \"{}\" (id {}) ===\n{}",
            p.name,
            p.id,
            element_source(all, p)
        )
    }));
    out.join("\n\n")
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
ids. The class's own spell slots, cantrip and spell choices and preparation are generated for you: describe them in \
prose only. Spells a feature grants (a spell table) and counters from class table columns are added from the outline; \
every other mechanic of the feature is yours to write."
    } else {
        guidance(&batch[0], g.draft.edition)
    };
    format!(
        "Write ALL of these {n} elements of type {ty} (edition {ed:?}) in ONE answer, each with EXACTLY its id:\n{list}\n\
{each}\n\n\
Real examples of the same kind (copy the SHAPE, not the content):\n{shots}\n\n\
Existing core ids you must reuse instead of inventing new ones:\n{cat}\n\n{kinds}\n\n\
Answer {{\"elements\": [{{\"element\": {{...}}, \"notes\": [...]}}, ...]}}, one entry per id above; put doubts in notes.{pools}{prior}\n\n{src}",
        n = batch.len(),
        pools = super::pools::pools_note(g.draft, batch),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::anything::types::SourceExcerpt;

    fn feat(name: &str, line: Option<usize>) -> PlannedElement {
        PlannedElement {
            id: format!("ID_T_{}", name.to_uppercase().replace(' ', "_")),
            ty: "Feat".into(),
            name: name.into(),
            role: Role::Standalone,
            level: None,
            owner: None,
            brief: String::new(),
            excerpt: line.map(|l| SourceExcerpt {
                start_line: l,
                end_line: l,
                snippet: String::new(),
            }),
            parent: None,
            picks: None,
            prerequisite_level: None,
            spells: Vec::new(),
        }
    }

    #[test]
    fn elements_without_a_section_share_one_whole_copy_per_batch() {
        // A source over the batch limit: each element with no section needs all of it.
        let mut src = String::new();
        for i in 0..1500 {
            src.push_str(&format!("Line {i} of a long almanac of roads.\n"));
        }
        assert!(src.chars().count() > BATCH_SOURCE_CHARS);
        let mut part: Vec<PlannedElement> = (0..10)
            .map(|i| feat(&format!("Lost Feat {i}"), None))
            .collect();
        part.extend((0..3).map(|i| feat(&format!("Found Feat {i}"), Some(10 + i))));
        let batches = split_by_size(&src, &part, BATCH_SIZE);
        // 10 whole-source elements in two batches of at most 8, the located three apart.
        assert_eq!(
            batches.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![8, 2, 3]
        );
        let draft_src = |batch: &[PlannedElement]| {
            let mut d = ImportDraft {
                source_text: src.clone(),
                ..serde_json::from_value(json!({"edition": "2024", "id_prefix": "t",
                    "source_name": "T", "pack_id": "t", "pack_name": "T", "source_kind": "text",
                    "source_text": "", "detection": {"kind": "feat", "edition": "2024",
                    "confidence": 1.0, "items": []}, "plan": {"edition": "2024", "groups": [],
                    "elements": [], "notes": []}, "elements": [], "sweep_issues": []}))
                .unwrap()
            };
            d.plan.elements = batch.to_vec();
            batch_source(&d, batch)
        };
        let whole = draft_src(&batches[0]);
        assert_eq!(whole.matches("Line 1499 of a long").count(), 1);
        assert!(whole.contains("\"Lost Feat 7\""));
        let own = draft_src(&batches[2]);
        assert!(own.contains("=== Source for \"Found Feat 1\"") && !own.contains("Line 1499"));
    }
}
