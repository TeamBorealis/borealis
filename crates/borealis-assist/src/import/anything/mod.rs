//! "Import anything": detect, plan, generate, validate, review, revise and commit.
//!
//! Every stage returns plain serializable data ([`Detection`], [`ImportPlan`], [`ImportDraft`]) so a
//! UI can show progress and resume. The model writes content; ids, levels, grants and the subclass
//! gate come from the plan, and every element must load and survive an engine sweep (a test
//! character takes the class and levels 1 to 20) before it is offered for review.

mod arcanum;
mod batch;
pub mod caster;
mod choices;
mod contents;
mod derived;
mod fallback;
mod feature_spells;
mod fewshot;
mod fidelity;
mod finalize;
mod formula;
mod furniture;
mod generate;
mod ingest;
mod intake;
mod items;
mod lead_in;
mod leveled;
mod lists;
mod locate;
mod mechanics;
mod normalize;
mod option_rows;
mod parts;
mod passage;
mod plan;
#[cfg(test)]
mod plan_shape_tests;
mod pools;
mod prepared;
mod prereq;
mod regenerate;
#[cfg(test)]
mod replay_tests;
mod report_cover;
pub mod review;
mod rule_terms;
mod rules;
mod selfcheck;
mod source;
mod steps;
#[cfg(test)]
mod structure_tests;
mod sweep;
mod targets;
mod text_fixes;
mod types;
mod units;
mod variants;

use borealis_ai::task::{run_task, RunOptions};
use borealis_content::ContentStoreBuilder;

use crate::error::AssistError;
use crate::homebrew::PackContext;

pub use finalize::{finalize_draft, Finalized};
pub use generate::MAX_REPAIR_ROUNDS;
pub use regenerate::{point_element_at, regenerate_element};
pub use steps::VisionRunner;
pub use sweep::{engine_sweep, SweepIssue};
pub use types::*;

use generate::{
    emit, fill_pending, parent_of, pending, run_element, validate_and_repair, GenCtx, Progress,
};
use steps::{outline_step, outlines_step, target_schema, Attaching, Step, TargetOut};

/// Classes and subclasses outlined per model call.
const OUTLINE_BATCH: usize = 6;
/// Species, subraces and backgrounds (short outlines: their named traits) per model call.
const PARTS_OUTLINE_BATCH: usize = 12;

/// Request for [`import_anything`].
#[derive(Debug, Clone)]
pub struct AnythingRequest {
    /// The source.
    pub source: ImportSource,
    /// User prefix for ids (`ID_<PREFIX>_<TYPE>_<NAME>`).
    pub id_prefix: String,
    /// `source` stamped on every element.
    pub source_name: String,
    /// Pack id.
    pub pack_id: String,
    /// Pack display name.
    pub pack_name: String,
    /// Edition to assume when the source does not say.
    pub edition_hint: Edition,
}

/// Where the pipeline stands.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportStep {
    /// A parent class is ambiguous: call [`choose_parent`] then [`continue_draft`].
    NeedsChoice(Box<ImportDraft>),
    /// The reviewable draft.
    Draft(Box<ImportDraft>),
}

impl ImportStep {
    /// The draft either way.
    pub fn draft(&self) -> &ImportDraft {
        match self {
            Self::NeedsChoice(d) | Self::Draft(d) => d,
        }
    }
}

fn ctx_of(base: &ContentStoreBuilder, edition: Edition) -> PackContext {
    PackContext {
        base: base.clone(),
        ruleset: edition.ruleset(),
    }
}

fn source_parts(source: &ImportSource) -> (&str, &'static str, &[borealis_ai::Attachment]) {
    match source {
        ImportSource::Text(t) => (t, "text", &[]),
        ImportSource::PdfText(t) => (t, "pdf_text", &[]),
        ImportSource::Attachments(a) => ("", "attachments", a),
    }
}

/// Stage 1: classify the source. Returns the detection and the source text (the model's
/// transcript for attachments).
///
/// # Errors
/// [`AssistError::Ai`] when the model fails, [`AssistError::Extraction`] for empty sources.
pub async fn detect_source(
    runner: &dyn VisionRunner,
    source: &ImportSource,
    edition_hint: Edition,
    opts: &RunOptions<'_>,
) -> Result<(Detection, String), AssistError> {
    let (text, _, attachments) = source_parts(source);
    if attachments.is_empty() && text.trim().is_empty() {
        return Err(AssistError::Extraction("the source is empty".into()));
    }
    // Attachments are read into text first (text layers, else verbatim transcription part by
    // part), so detection and every later stage see the whole source, never a summary.
    let (text, reading) = if attachments.is_empty() {
        (text.to_string(), Vec::new())
    } else {
        let read = ingest::read_attachments(runner, attachments, opts).await?;
        (read.text, read.notes)
    };
    let mut out = ingest::detect_text(runner, &text, edition_hint, opts).await?;
    out.notes.extend(reading);
    let (items, mixed): (Vec<DetectedItem>, Vec<DetectedItem>) = out
        .items
        .into_iter()
        .partition(|i| i.kind != ImportKind::Mixed);
    let items = variants::with_bases(variants::split(items, out.edition));
    let other: Vec<OtherItem> = out
        .other
        .into_iter()
        .chain(mixed.into_iter().map(|i| OtherItem {
            name: i.name,
            what: "mixed".into(),
            note: "listed with no single kind".into(),
        }))
        .collect();
    // A source with no element kinds is still an import: its rules and lore become text
    // entries, and the report says when nothing mechanical was found.
    let detection = Detection {
        kind: overall_kind(&items),
        edition: out.edition,
        confidence: out.confidence.clamp(0.0, 1.0),
        items,
        notes: out.notes,
        other,
    };
    Ok((detection, text))
}

/// Stage 2: outline classes and subclasses, resolve subclass parents in the library and list the
/// elements to generate.
///
/// # Errors
/// [`AssistError::Ai`] when an outline call fails.
pub async fn plan_import(
    runner: &dyn VisionRunner,
    base: &ContentStoreBuilder,
    detection: &Detection,
    source_text: &str,
    id_prefix: &str,
    opts: &RunOptions<'_>,
) -> Result<ImportPlan, AssistError> {
    let store = base.clone().build();
    let adapter = Attaching {
        inner: runner,
        attachments: &[],
    };
    let mut plan = ImportPlan {
        edition: detection.edition,
        groups: Vec::new(),
        elements: Vec::new(),
        notes: Vec::new(),
    };
    // Parents first (they decide the subclass levels), then the outlines in as few calls as
    // possible: one for a single item, else OUTLINE_BATCH classes and subclasses or
    // PARTS_OUTLINE_BATCH species, subraces and backgrounds per call.
    let mut wanted: Vec<(usize, DetectedItem, Option<Vec<u32>>)> = Vec::new();
    let mut parted: Vec<(usize, DetectedItem, Option<Vec<u32>>)> = Vec::new();
    let names_of = |kind: ImportKind| -> Vec<String> {
        detection
            .items
            .iter()
            .filter(|i| i.kind == kind)
            .map(|i| i.name.clone())
            .collect()
    };
    let classes = names_of(ImportKind::Class);
    let species = names_of(ImportKind::Species);
    // An item the source only mentions that the library has is referenced, not re-created.
    let references =
        intake::library_references(&store, detection.edition, &detection.items, source_text);
    let referenced = |item: &DetectedItem| {
        item.kind == ImportKind::Item && references.iter().any(|(n, _)| n == &item.name)
    };
    for item in detection.items.iter().filter(|i| !referenced(i)) {
        let mut group = plan::standalone(item.kind, &item.name, item.item_type);
        if matches!(item.kind, ImportKind::Class | ImportKind::Subclass) {
            let mut levels: Option<Vec<u32>> = None;
            let own = (item.kind == ImportKind::Subclass)
                .then(|| plan::in_source_class(&classes, item.parent_hint.as_deref()))
                .flatten();
            if let Some(class) = own {
                // A subclass of a class from this source: linked to the planned class.
                group.parent = Some(plan::in_source_link(class));
            } else if item.kind == ImportKind::Subclass {
                match plan::resolve_parent(
                    &store,
                    detection.edition,
                    item.parent_hint.as_deref(),
                    &item.name,
                ) {
                    Ok(link) => {
                        levels = Some(link.subclass_levels.clone());
                        group.parent = Some(link);
                    }
                    Err(choice) => group.choice = Some(choice),
                }
            }
            wanted.push((plan.groups.len(), item.clone(), levels));
        } else if matches!(
            item.kind,
            ImportKind::Species | ImportKind::Subrace | ImportKind::Background
        ) {
            if item.kind == ImportKind::Subrace {
                group.parent = plan::subrace_parent(
                    &store,
                    detection.edition,
                    &species,
                    item.parent_hint.as_deref(),
                );
            }
            parted.push((plan.groups.len(), item.clone(), None));
        }
        plan.groups.push(group);
    }
    add_text_groups(&mut plan.groups, detection, source_text);
    let all: Vec<_> = wanted.iter().chain(&parted).cloned().collect();
    if let [(gi, item, levels)] = all.as_slice() {
        let step = outline_step(item, detection.edition, source_text, levels.as_deref());
        plan.groups[*gi].outline = Some(run_task(&adapter, &step, opts).await?.output.outline);
    } else {
        let chunks = wanted
            .chunks(OUTLINE_BATCH)
            .chain(parted.chunks(PARTS_OUTLINE_BATCH));
        for chunk in chunks {
            let items: Vec<(DetectedItem, Option<Vec<u32>>)> = chunk
                .iter()
                .map(|(_, i, l)| (i.clone(), l.clone()))
                .collect();
            let step = outlines_step(&items, detection.edition, source_text);
            let out = run_task(&adapter, &step, opts).await?.output;
            for (k, (gi, item, _)) in chunk.iter().enumerate() {
                plan.groups[*gi].outline = out.take(&item.name, k);
            }
        }
    }
    let tables = intake::option_tables(&detection.other, source_text);
    let mut intake_notes = intake::attach_option_tables(&mut plan.groups, &tables);
    plan::rebuild(&mut plan, &store, id_prefix, source_text);
    // Rules prose introducing a list of planned entries is kept as an entry of its own.
    let leads = lead_in::lead_ins(&plan, source_text);
    if !leads.is_empty() {
        plan.groups.extend(
            leads
                .iter()
                .map(|t| plan::text_group(t, "rules introducing the entries below", "")),
        );
        plan::rebuild(&mut plan, &store, id_prefix, source_text);
    }
    intake_notes.extend(
        references
            .iter()
            .map(|(name, id)| intake::library_note(name, id)),
    );
    plan.notes.extend(intake_notes);
    plan.notes.extend(plan::narrative_notes(&detection.other));
    plan.notes
        .extend(plan::contents_notes(&detection.other, source_text));
    let own = plan::planned_class_links(&plan);
    for choice in plan.groups.iter_mut().filter_map(|g| g.choice.as_mut()) {
        choice
            .candidates
            .extend(own.iter().map(|link| ParentCandidate {
                link: link.clone(),
                score: 0,
            }));
    }
    Ok(plan)
}

/// Rules and lore the detection could not type keep their text as text-only groups; narrative
/// sections (see [`intake::is_narrative`]) and contents pages are reported, never imported as
/// rules.
fn add_text_groups(groups: &mut Vec<PlanGroup>, detection: &Detection, source: &str) {
    let texts: Vec<PlanGroup> = detection
        .other
        .iter()
        .filter(|o| !intake::is_narrative(&o.what))
        .filter(|o| !contents::is_contents(source, &o.name))
        .filter(|o| !detection.items.iter().any(|i| same_name(&i.name, &o.name)))
        .map(|o| plan::text_group(&o.name, &o.what, &o.note))
        .collect();
    for g in texts {
        if !groups.iter().any(|x| same_name(&x.name, &g.name)) {
            groups.push(g);
        }
    }
}

/// The kind of the whole source: the single kind of its items, `Mixed` for several, `Rules`
/// when the detection found no element kind at all.
fn overall_kind(items: &[DetectedItem]) -> ImportKind {
    match items.first() {
        None => ImportKind::Rules,
        Some(first) if items.iter().all(|i| i.kind == first.kind) => first.kind,
        Some(_) => ImportKind::Mixed,
    }
}

fn same_name(a: &str, b: &str) -> bool {
    let n = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    n(a) == n(b)
}

/// Resolve an open parent choice: the user picked `class_id` for the subclass `group`.
///
/// # Errors
/// [`AssistError::Invalid`] for an unknown group or a class that has no subclass choice.
pub fn choose_parent(
    base: &ContentStoreBuilder,
    draft: &ImportDraft,
    group: &str,
    class_id: &str,
) -> Result<ImportDraft, AssistError> {
    let store = base.clone().build();
    let link = plan::link_for(&store, draft.edition, class_id)
        .or_else(|| {
            plan::planned_class_links(&draft.plan)
                .into_iter()
                .find(|l| l.class_id == class_id)
        })
        .ok_or_else(|| {
            AssistError::Invalid(format!("{class_id} is not a class with a subclass choice"))
        })?;
    let mut next = draft.clone();
    let g = next
        .plan
        .groups
        .iter_mut()
        .find(|g| g.name == group && g.kind == ImportKind::Subclass)
        .ok_or_else(|| AssistError::Invalid(format!("no subclass named {group} in the plan")))?;
    g.parent = Some(link);
    g.choice = None;
    plan::rebuild(&mut next.plan, &store, &next.id_prefix, &next.source_text);
    next.elements = pending(&next.plan);
    Ok(next)
}

/// Stages 3 and 4 for a draft: generate the pending elements, then validate and repair.
///
/// # Errors
/// Never fails on element problems (they are statuses); the `Result` is for future stages.
pub async fn continue_draft(
    runner: &dyn VisionRunner,
    base: &ContentStoreBuilder,
    draft: ImportDraft,
    opts: &RunOptions<'_>,
    progress: Progress<'_>,
) -> Result<ImportStep, AssistError> {
    let mut draft = draft;
    // A usage limit or lost sign-in stops every call and fails the import (no partial draft).
    let guard = batch::AccountGuard::new(runner);
    let runner: &dyn VisionRunner = &guard;
    if !draft.plan.open_choices().is_empty() {
        return Ok(ImportStep::NeedsChoice(Box::new(draft)));
    }
    if draft.elements.is_empty() {
        draft.elements = pending(&draft.plan);
    }
    let ctx = ctx_of(base, draft.edition);
    let adapter = Attaching {
        inner: runner,
        attachments: &[],
    };
    fill_pending(&adapter, &ctx, &mut draft, opts, progress).await;
    validate_and_repair(&adapter, &ctx, &mut draft, opts, progress).await;
    selfcheck::heal(&adapter, &ctx, &mut draft, opts, progress).await;
    guard.check()?;
    Ok(ImportStep::Draft(Box::new(draft)))
}

/// The whole pipeline: detect, plan, generate, validate. Stops with
/// [`ImportStep::NeedsChoice`] when a subclass's parent class is ambiguous.
///
/// # Errors
/// [`AssistError`] from the detect and plan stages.
pub async fn import_anything(
    runner: &dyn VisionRunner,
    base: &ContentStoreBuilder,
    req: &AnythingRequest,
    opts: &RunOptions<'_>,
    progress: Progress<'_>,
) -> Result<ImportStep, AssistError> {
    let guard = batch::AccountGuard::new(runner);
    let runner: &dyn VisionRunner = &guard;
    emit(progress, Stage::Detect, 0, 1, "reading the source");
    let (detection, text) = detect_source(runner, &req.source, req.edition_hint, opts).await?;
    emit(progress, Stage::Plan, 0, detection.items.len(), "planning");
    let plan = plan_import(runner, base, &detection, &text, &req.id_prefix, opts).await?;
    guard.check()?;
    let draft = ImportDraft {
        edition: detection.edition,
        id_prefix: req.id_prefix.clone(),
        source_name: req.source_name.clone(),
        pack_id: req.pack_id.clone(),
        pack_name: req.pack_name.clone(),
        source_kind: source_parts(&req.source).1.to_string(),
        source_text: text,
        detection,
        elements: pending(&plan),
        plan,
        sweep_issues: Vec::new(),
        source_checks: Vec::new(),
    };
    continue_draft(runner, base, draft, opts, progress).await
}

/// Stage 6: change one element as the user says, then re-validate the whole draft. With no
/// `element_id` the model first picks the element the instruction is about.
///
/// # Errors
/// [`AssistError::Invalid`] for an unknown or deterministic element, [`AssistError::Ai`] when the
/// model fails.
pub async fn revise_draft(
    runner: &dyn VisionRunner,
    base: &ContentStoreBuilder,
    draft: &ImportDraft,
    instruction: &str,
    element_id: Option<&str>,
    opts: &RunOptions<'_>,
    progress: Progress<'_>,
) -> Result<ImportDraft, AssistError> {
    let adapter = Attaching {
        inner: runner,
        attachments: &[],
    };
    let id = match element_id {
        Some(id) => id.to_string(),
        None => pick_target(&adapter, draft, instruction, opts).await?,
    };
    let mut next = draft.clone();
    let idx = next
        .elements
        .iter()
        .position(|e| e.id == id)
        .ok_or_else(|| AssistError::Invalid(format!("no element {id} in the draft")))?;
    if next.elements[idx].role == Role::SubclassGate {
        return Err(AssistError::Invalid(
            "the subclass choice feature is generated; revise the class instead".into(),
        ));
    }
    let planned = next
        .plan
        .elements
        .iter()
        .find(|p| p.id == id)
        .cloned()
        .ok_or_else(|| AssistError::Invalid(format!("{id} is not in the plan")))?;
    emit(progress, Stage::Revise, 0, 1, &planned.name);
    let ctx = ctx_of(base, next.edition);
    let store = ctx.base.clone().build();
    let current = next.elements[idx].json.clone();
    let parent = parent_of(&next, &planned);
    let (el, notes, _) = {
        let g = GenCtx {
            ctx: &ctx,
            store: &store,
            draft: &next,
            parent,
        };
        run_element(
            &adapter,
            &g,
            &planned,
            "import_revise",
            Some((&current, &[])),
            Some(instruction),
            opts,
        )
        .await
        .map_err(AssistError::Invalid)?
    };
    let e = &mut next.elements[idx];
    e.json = el;
    e.revisions += 1;
    e.status = if notes.is_empty() {
        ElementStatus::Ok
    } else {
        ElementStatus::NeedsReview
    };
    e.issues = notes;
    validate_and_repair(&adapter, &ctx, &mut next, opts, progress).await;
    selfcheck::apply(&ctx.base, &mut next);
    Ok(next)
}

async fn pick_target(
    adapter: &Attaching<'_>,
    draft: &ImportDraft,
    instruction: &str,
    opts: &RunOptions<'_>,
) -> Result<String, AssistError> {
    let list: Vec<String> = draft
        .elements
        .iter()
        .filter(|e| e.role != Role::SubclassGate)
        .map(|e| format!("{} | {} | {} | level {:?}", e.id, e.ty, e.name, e.level))
        .collect();
    let user = format!(
        "Which element does this instruction change? Answer with its id from the list.\n\n{}\n\n{}",
        borealis_ai::task::prompt::fence("user_input", instruction),
        borealis_ai::task::prompt::fence("context", &list.join("\n"))
    );
    let valid: Vec<String> = draft.elements.iter().map(|e| e.id.clone()).collect();
    let step: Step<TargetOut> = Step::new(
        "import_target",
        "choose which imported element a user instruction is about",
        "Output {\"element_id\": \"...\"} only.",
        user,
        target_schema(),
    )
    .with_check(move |t: &TargetOut| {
        if valid.contains(&t.element_id) {
            Vec::new()
        } else {
            vec![format!(
                "{} is not an element id from the list",
                t.element_id
            )]
        }
    });
    Ok(run_task(adapter, &step, opts).await?.output.element_id)
}
