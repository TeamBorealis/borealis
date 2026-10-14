//! "Import anything": detect, plan, generate, validate, review, revise and commit.
//!
//! Every stage returns plain serializable data ([`Detection`], [`ImportPlan`], [`ImportDraft`]) so a
//! UI can show progress and resume. The model writes content; ids, levels, grants and the subclass
//! gate come from the plan, and every element must load and survive an engine sweep (a test
//! character takes the class and levels 1 to 20) before it is offered for review.

mod batch;
pub mod caster;
mod fewshot;
mod finalize;
mod generate;
mod items;
mod normalize;
mod plan;
mod rules;
mod steps;
mod sweep;
mod types;

use borealis_ai::task::{run_task, RunOptions};
use borealis_content::ContentStoreBuilder;

use crate::error::AssistError;
use crate::homebrew::PackContext;

pub use finalize::{finalize_draft, Finalized};
pub use generate::MAX_REPAIR_ROUNDS;
pub use steps::VisionRunner;
pub use sweep::{engine_sweep, SweepIssue};
pub use types::*;

use generate::{
    emit, fill_pending, parent_of, pending, run_element, validate_and_repair, GenCtx, Progress,
};
use steps::{
    detect_step, outline_step, outlines_step, target_schema, Attaching, DetectOut, Step, TargetOut,
};

/// Classes and subclasses outlined per model call.
const OUTLINE_BATCH: usize = 6;

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
    let step: Step<DetectOut> = detect_step(text, !attachments.is_empty(), edition_hint);
    let adapter = Attaching {
        inner: runner,
        attachments,
    };
    let out = run_task(&adapter, &step, opts).await?.output;
    let text = if attachments.is_empty() {
        text.to_string()
    } else {
        out.transcript.clone()
    };
    if text.trim().is_empty() {
        return Err(AssistError::Extraction(
            "the model could not read any text in the attachments".into(),
        ));
    }
    let (items, mixed): (Vec<DetectedItem>, Vec<DetectedItem>) = out
        .items
        .into_iter()
        .partition(|i| i.kind != ImportKind::Mixed);
    let other: Vec<OtherItem> = out
        .other
        .into_iter()
        .chain(mixed.into_iter().map(|i| OtherItem {
            name: i.name,
            what: "mixed".into(),
            note: "listed with no single kind".into(),
        }))
        .collect();
    if items.is_empty() {
        return Err(AssistError::Extraction(
            "nothing recognisable was found in the source".into(),
        ));
    }
    let same = items.iter().all(|i| i.kind == items[0].kind);
    let detection = Detection {
        kind: if same {
            items[0].kind
        } else {
            ImportKind::Mixed
        },
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
    // possible: one for a single class or subclass, else OUTLINE_BATCH per call.
    let mut wanted: Vec<(usize, DetectedItem, Option<Vec<u32>>)> = Vec::new();
    let classes: Vec<String> = detection
        .items
        .iter()
        .filter(|i| i.kind == ImportKind::Class)
        .map(|i| i.name.clone())
        .collect();
    for item in &detection.items {
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
        }
        plan.groups.push(group);
    }
    if let [(gi, item, levels)] = wanted.as_slice() {
        let step = outline_step(item, detection.edition, source_text, levels.as_deref());
        plan.groups[*gi].outline = Some(run_task(&adapter, &step, opts).await?.output.outline);
    } else {
        for chunk in wanted.chunks(OUTLINE_BATCH) {
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
    plan::rebuild(&mut plan, &store, id_prefix, source_text);
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
    emit(progress, Stage::Detect, 0, 1, "reading the source");
    let (detection, text) = detect_source(runner, &req.source, req.edition_hint, opts).await?;
    emit(progress, Stage::Plan, 0, detection.items.len(), "planning");
    let plan = plan_import(runner, base, &detection, &text, &req.id_prefix, opts).await?;
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
