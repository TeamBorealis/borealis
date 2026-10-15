//! Long-running stages of "Import anything": start, continue after a parent choice, and revise.
//! Each has a quick `begin` step (validate, claim the draft) and an async `run` step that streams
//! `borealis://import-event` events and ends with `ready` or `error`.

use crate::ai_service::AiService;
use crate::error::{ApiError, ApiResult, ErrorCode};
use crate::import_drafts::StoredDraft;
use crate::import_dto::{ImportEvent, ImportHintDto, ImportSink};
use crate::import_notes::NotesRunner;
use crate::import_sources::{prepare_sources, ImportSourceDto, PickedFiles, PreparedSource};
use crate::picked_paths::PickPurpose;
use crate::state::AppState;
use borealis_ai::task::RunOptions;
use borealis_assist::import::anything::{
    point_element_at, regenerate_element, ImportProgress, Role, Stage,
};
use borealis_assist::{
    choose_parent, continue_draft, import_anything, revise_draft, AnythingRequest, ImportDraft,
    ImportStep, VisionRunner,
};
use std::sync::{Arc, Mutex};

/// Repair rounds the pipeline may spend per element.
const MAX_REPAIRS: u32 = 2;
/// Longest revise instruction.
const MAX_INSTRUCTION_CHARS: usize = 2000;
/// Prefix of ids until the user picks one.
const DEFAULT_PREFIX: &str = "imp";

/// Everything `run_import_start` needs.
#[derive(Debug, Clone)]
pub struct StartJob {
    pub draft_id: String,
    pub prepared: PreparedSource,
    pub hint: ImportHintDto,
}

/// Maps pipeline progress to monotonic wire events.
pub struct Emitter {
    id: String,
    sink: Arc<dyn ImportSink>,
    last: Mutex<f64>,
}

impl Emitter {
    #[must_use]
    pub fn new(id: &str, sink: Arc<dyn ImportSink>) -> Self {
        Self {
            id: id.to_string(),
            sink,
            last: Mutex::new(0.0),
        }
    }

    fn send(&self, stage: &'static str, progress: f64, message: &str) {
        let progress = {
            let mut last = self
                .last
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *last = last.max(progress.clamp(0.0, 1.0));
            *last
        };
        self.sink.import_event(&ImportEvent {
            draft_id: self.id.clone(),
            stage,
            progress,
            message: message.to_string(),
            element: None,
        });
    }

    fn on_progress(&self, p: &ImportProgress) {
        #[allow(clippy::cast_precision_loss)]
        let frac = if p.total == 0 {
            0.0
        } else {
            (p.done as f64 / p.total as f64).clamp(0.0, 1.0)
        };
        let (stage, lo, hi) = match p.stage {
            Stage::Detect => ("detect", 0.02, 0.12),
            Stage::Plan => ("plan", 0.12, 0.25),
            Stage::Generate => ("generate", 0.25, 0.7),
            Stage::Validate => ("validate", 0.7, 0.98),
            Stage::Revise => ("repair", 0.1, 0.9),
        };
        self.send(stage, lo + (hi - lo) * frac, &p.message);
    }
}

/// An assistant error as the API error the UI shows.
pub(crate) fn assist_err(e: &borealis_assist::AssistError) -> ApiError {
    match e {
        borealis_assist::AssistError::Ai(
            ai @ borealis_ai::AiError::UntrustedInputNeedsConsent { .. },
        ) => ApiError::from_ai(ai),
        borealis_assist::AssistError::Ai(_) => {
            ApiError::new(ErrorCode::AiUnavailable, e.to_string())
        }
        other => ApiError::invalid(other.to_string()),
    }
}

fn suggested_name(d: &ImportDraft) -> String {
    match d.detection.items.as_slice() {
        [one] => one.name.clone(),
        _ => "Imported content".to_string(),
    }
}

impl AppState {
    /// Resolve every `File` token (purpose `import-file`) to the path the user picked.
    fn resolve_import_files(&self, sources: &[ImportSourceDto]) -> ApiResult<PickedFiles> {
        let mut out = PickedFiles::new();
        for s in sources {
            if let ImportSourceDto::File { token } = s {
                let path = self.picked.resolve(token, PickPurpose::ImportFile)?;
                out.insert(token.clone(), path);
            }
        }
        Ok(out)
    }

    /// Validate the sources, make sure a provider exists, and create the draft record.
    ///
    /// # Errors
    /// `invalid_argument` for bad or oversized sources, `ai_unavailable` without a provider.
    pub async fn import_start(
        &self,
        ai: &AiService,
        sources: Vec<ImportSourceDto>,
        hint: Option<ImportHintDto>,
        allow_untrusted_with_codex: bool,
    ) -> ApiResult<StartJob> {
        let picked = self.resolve_import_files(&sources)?;
        let prepared =
            tauri::async_runtime::spawn_blocking(move || prepare_sources(&sources, &picked))
                .await
                .map_err(ApiError::internal)??;
        let draft_id = format!("imp_{}", uuid::Uuid::new_v4().simple());
        ai.import_authorize(self, &draft_id, allow_untrusted_with_codex)
            .await?;
        self.drafts
            .put(&self.paths, &draft_id, StoredDraft::started())?;
        self.drafts.claim(&draft_id)?;
        Ok(StartJob {
            draft_id,
            prepared,
            hint: hint.unwrap_or_default(),
        })
    }

    /// Run detect, plan, generate and validate; always ends with a `ready` or `error` event.
    pub async fn run_import_start(&self, ai: &AiService, job: StartJob, sink: Arc<dyn ImportSink>) {
        let em = Emitter::new(&job.draft_id, sink);
        let result = self.start_inner(ai, &job, &em).await;
        self.finish(&job.draft_id, &em, result, None);
    }

    async fn start_inner(
        &self,
        ai: &AiService,
        job: &StartJob,
        em: &Emitter,
    ) -> ApiResult<ImportDraft> {
        let runner = ai.import_runner(self, &job.draft_id).await?;
        let base = self.hub()?.builder.clone();
        let short: String = job.draft_id.chars().rev().take(6).collect();
        let req = AnythingRequest {
            source: job.prepared.source.clone(),
            id_prefix: DEFAULT_PREFIX.to_string(),
            source_name: job.prepared.source_name.clone(),
            pack_id: format!("import-{short}"),
            pack_name: "Imported content".to_string(),
            edition_hint: job.hint.edition(),
        };
        let opts = RunOptions {
            max_repairs: MAX_REPAIRS,
            sink: None,
        };
        let cb = |p: ImportProgress| em.on_progress(&p);
        let notes = job.prepared.notes.as_deref().unwrap_or_default();
        let noted = NotesRunner::new(runner.as_ref(), notes);
        let runner: &dyn VisionRunner = &noted;
        let step = import_anything(runner, &base, &req, &opts, Some(&cb))
            .await
            .map_err(|e| assist_err(&e))?;
        let step = match (step, job.hint.parent_class_id.as_deref()) {
            (ImportStep::NeedsChoice(d), Some(class_id)) => {
                let group = d.plan.open_choices().first().map(|c| c.group.clone());
                match group {
                    Some(g) => {
                        let next =
                            choose_parent(&base, &d, &g, class_id).map_err(|e| assist_err(&e))?;
                        continue_draft(runner, &base, next, &opts, Some(&cb))
                            .await
                            .map_err(|e| assist_err(&e))?
                    }
                    None => ImportStep::NeedsChoice(d),
                }
            }
            (other, _) => other,
        };
        let mut draft = step.draft().clone();
        draft.pack_name = suggested_name(&draft);
        Ok(draft)
    }

    /// Pick the parent class of a subclass; the elements are then generated by
    /// [`AppState::run_import_continue`].
    ///
    /// # Errors
    /// `not_found`, `requirement_failed` (busy or nothing to choose), `invalid_argument`.
    pub fn import_choose_parent(&self, id: &str, class_id: &str) -> ApiResult<ImportDraft> {
        let stored = self.drafts.get(&self.paths, id)?;
        let draft = stored
            .draft
            .ok_or_else(|| ApiError::requirement("This import has no draft yet."))?;
        let group = draft
            .plan
            .open_choices()
            .first()
            .map(|c| c.group.clone())
            .ok_or_else(|| ApiError::requirement("There is no parent class to choose."))?;
        self.drafts.claim(id)?;
        let base = self.hub()?.builder.clone();
        match choose_parent(&base, &draft, &group, class_id) {
            Ok(next) => {
                let stored = StoredDraft {
                    stage: "generate".into(),
                    draft: Some(next.clone()),
                    ..StoredDraft::started()
                };
                if let Err(e) = self.drafts.put(&self.paths, id, stored) {
                    self.drafts.release(id);
                    return Err(e);
                }
                Ok(next)
            }
            Err(e) => {
                self.drafts.release(id);
                Err(assist_err(&e))
            }
        }
    }

    /// Generate and validate the elements of a draft after a parent was chosen.
    pub async fn run_import_continue(&self, ai: &AiService, id: &str, sink: Arc<dyn ImportSink>) {
        let em = Emitter::new(id, sink);
        let previous = self.drafts.get(&self.paths, id).ok();
        let result = async {
            let draft = previous
                .as_ref()
                .and_then(|p| p.draft.clone())
                .ok_or_else(|| ApiError::not_found("That import no longer exists."))?;
            let runner = ai.import_runner(self, id).await?;
            let base = self.hub()?.builder.clone();
            let opts = RunOptions {
                max_repairs: MAX_REPAIRS,
                sink: None,
            };
            let cb = |p: ImportProgress| em.on_progress(&p);
            let step = continue_draft(runner.as_ref(), &base, draft, &opts, Some(&cb))
                .await
                .map_err(|e| assist_err(&e))?;
            Ok(step.draft().clone())
        }
        .await;
        self.finish(id, &em, result, previous);
    }

    /// Claim a draft for a revise run.
    ///
    /// # Errors
    /// `not_found`, `invalid_argument` (empty instruction), `requirement_failed` (busy, no draft).
    pub fn import_begin_revise(&self, id: &str, instruction: &str) -> ApiResult<()> {
        let instruction = instruction.trim();
        if instruction.is_empty() || instruction.chars().count() > MAX_INSTRUCTION_CHARS {
            return Err(ApiError::invalid(
                "Tell the AI what to change, in a few sentences.",
            ));
        }
        let stored = self.drafts.get(&self.paths, id)?;
        let draft = stored
            .draft
            .ok_or_else(|| ApiError::requirement("This import has no draft yet."))?;
        if !draft.plan.open_choices().is_empty() {
            return Err(ApiError::requirement("Pick the parent class first."));
        }
        self.drafts.claim(id)
    }

    /// Claim a draft to re-locate one element on lines `start..=end` (1-based) of its source.
    ///
    /// # Errors
    /// `not_found`, `invalid_argument` (bad range, unknown or generated element),
    /// `requirement_failed` (busy, no draft, open parent choice).
    pub fn import_begin_relocate(
        &self,
        id: &str,
        element_id: &str,
        start: usize,
        end: usize,
    ) -> ApiResult<()> {
        let stored = self.drafts.get(&self.paths, id)?;
        let draft = stored
            .draft
            .ok_or_else(|| ApiError::requirement("This import has no draft yet."))?;
        if !draft.plan.open_choices().is_empty() {
            return Err(ApiError::requirement("Pick the parent class first."));
        }
        relocated(&draft, element_id, start, end)?;
        self.drafts.claim(id)
    }

    /// Claim a draft to regenerate one element from its current passage.
    ///
    /// # Errors
    /// `not_found`, `invalid_argument` (unknown or generated element), `requirement_failed`
    /// (busy, no draft, open parent choice).
    pub fn import_begin_regenerate(&self, id: &str, element_id: &str) -> ApiResult<()> {
        let stored = self.drafts.get(&self.paths, id)?;
        let draft = stored
            .draft
            .ok_or_else(|| ApiError::requirement("This import has no draft yet."))?;
        if !draft.plan.open_choices().is_empty() {
            return Err(ApiError::requirement("Pick the parent class first."));
        }
        let el = draft
            .element(element_id)
            .ok_or_else(|| ApiError::not_found("That element is not in this import."))?;
        if el.role == Role::SubclassGate {
            return Err(ApiError::invalid(
                "The subclass choice is built from the class; regenerate the class instead.",
            ));
        }
        self.drafts.claim(id)
    }

    /// Re-locate one element on the lines the user selected and generate it again from them
    /// (the generation path, not a revision of its current JSON).
    pub async fn run_import_relocate(
        &self,
        ai: &AiService,
        id: &str,
        element_id: &str,
        lines: (usize, usize),
        sink: Arc<dyn ImportSink>,
    ) {
        self.regenerate_inner(ai, id, element_id, Some(lines), sink)
            .await;
    }

    /// Generate one element again from its current passage.
    pub async fn run_import_regenerate(
        &self,
        ai: &AiService,
        id: &str,
        element_id: &str,
        sink: Arc<dyn ImportSink>,
    ) {
        self.regenerate_inner(ai, id, element_id, None, sink).await;
    }

    async fn regenerate_inner(
        &self,
        ai: &AiService,
        id: &str,
        element_id: &str,
        lines: Option<(usize, usize)>,
        sink: Arc<dyn ImportSink>,
    ) {
        let em = Emitter::new(id, sink);
        let previous = self.drafts.get(&self.paths, id).ok();
        let result = async {
            let draft = previous
                .as_ref()
                .and_then(|p| p.draft.clone())
                .ok_or_else(|| ApiError::not_found("That import no longer exists."))?;
            let runner = ai.import_runner(self, id).await?;
            let base = self.hub()?.builder.clone();
            let opts = RunOptions {
                max_repairs: MAX_REPAIRS,
                sink: None,
            };
            let cb = |p: ImportProgress| em.on_progress(&p);
            regenerate_element(
                runner.as_ref(),
                &base,
                &draft,
                element_id,
                lines,
                &opts,
                Some(&cb),
            )
            .await
            .map_err(|e| assist_err(&e))
        }
        .await;
        self.finish(id, &em, result, previous);
    }

    /// Apply a revision with the model and re-validate.
    pub async fn run_import_revise(
        &self,
        ai: &AiService,
        id: &str,
        instruction: &str,
        element_id: Option<&str>,
        sink: Arc<dyn ImportSink>,
    ) {
        self.revise_inner(ai, id, instruction, element_id, sink)
            .await;
    }

    async fn revise_inner(
        &self,
        ai: &AiService,
        id: &str,
        instruction: &str,
        element_id: Option<&str>,
        sink: Arc<dyn ImportSink>,
    ) {
        let em = Emitter::new(id, sink);
        let previous = self.drafts.get(&self.paths, id).ok();
        let result = async {
            let draft = previous
                .as_ref()
                .and_then(|p| p.draft.clone())
                .ok_or_else(|| ApiError::not_found("That import no longer exists."))?;
            let runner = ai.import_runner(self, id).await?;
            let base = self.hub()?.builder.clone();
            let opts = RunOptions {
                max_repairs: MAX_REPAIRS,
                sink: None,
            };
            let cb = |p: ImportProgress| em.on_progress(&p);
            revise_draft(
                runner.as_ref(),
                &base,
                &draft,
                instruction.trim(),
                element_id,
                &opts,
                Some(&cb),
            )
            .await
            .map_err(|e| assist_err(&e))
        }
        .await;
        self.finish(id, &em, result, previous);
    }

    fn finish(
        &self,
        id: &str,
        em: &Emitter,
        result: ApiResult<ImportDraft>,
        previous: Option<StoredDraft>,
    ) {
        let outcome = match result {
            Ok(draft) => {
                let msg = if draft.plan.open_choices().is_empty() {
                    "Ready for review"
                } else {
                    "Pick the parent class"
                };
                // Review decisions survive a revise; an acceptance lapses when its element changed.
                let mut review = previous.map(|p| p.review).unwrap_or_default();
                review.prune(&draft);
                let stored = StoredDraft {
                    stage: "ready".into(),
                    draft: Some(draft),
                    error: None,
                    review,
                };
                self.drafts
                    .put(&self.paths, id, stored)
                    .map(|()| msg.to_string())
            }
            Err(e) => {
                let stored = match previous {
                    Some(mut p) if p.draft.is_some() => {
                        p.stage = "ready".into();
                        p
                    }
                    _ => StoredDraft {
                        stage: "error".into(),
                        error: Some(e.message.clone()),
                        ..StoredDraft::started()
                    },
                };
                let _ = self.drafts.put(&self.paths, id, stored);
                Err(e)
            }
        };
        self.drafts.release(id);
        match outcome {
            Ok(msg) => em.send("ready", 1.0, &msg),
            Err(e) => em.send("error", 0.0, &e.message),
        }
    }
}

/// `draft` with element `element_id` (and its plan entry) pointing at source lines
/// `start..=end` (1-based, inclusive): the range check of a relocate, before the run claims
/// the draft.
///
/// # Errors
/// `not_found` for an unknown element, `invalid_argument` for a range outside the source or the
/// generated subclass choice feature.
pub(crate) fn relocated(
    draft: &ImportDraft,
    element_id: &str,
    start: usize,
    end: usize,
) -> ApiResult<ImportDraft> {
    if draft.element(element_id).is_none() {
        return Err(ApiError::not_found("That element is not in this import."));
    }
    point_element_at(draft, element_id, start, end).map_err(|e| ApiError::invalid(e.to_string()))
}
