//! Failed generation calls are retried before any element is marked failed. A call that timed
//! out or produced too much output is retried as two smaller batches; a transport failure (CLI
//! error, no result, I/O) is retried once as it was and then split; an answer that still failed
//! validation after its repairs (an empty or partial answer) is split. A batch of one element
//! goes to the single-element path, which retries a transient failure once. A refused call (the
//! import's call budget, a cancel, a missing CLI) is never retried, and after
//! [`MAX_FAILURES_IN_A_ROW`] calls in a row failed on time or transport, nothing is retried any
//! more (the model or its connection is down): every planned call is still made once, as
//! without retries, and the queued retries fail without a call.

use borealis_ai::task::{AiRunner, RunOptions};
use borealis_ai::AiError;
use borealis_content::ContentStore;
use serde_json::Value;

use super::super::types::{ElementStatus, ImportDraft, PlannedElement, Stage};
use super::{emit, push_new, run_element_call, GenCtx, Progress};
use crate::homebrew::PackContext;

/// Failed calls in a row after which no call is retried.
pub(crate) const MAX_FAILURES_IN_A_ROW: u32 = 4;

/// What follows a failed call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Next {
    /// Make the same call again.
    Again,
    /// Retry its elements as two smaller batches.
    Split,
    /// Generate its (single) element on the single-element path.
    Alone,
    /// Mark its elements failed.
    GiveUp,
}

/// Retry bookkeeping of one import's generation stage.
#[derive(Debug, Default)]
pub(crate) struct Retries {
    in_row: u32,
    /// Calls retried (again, split or alone), for the progress log.
    pub retried: u32,
}

impl Retries {
    /// A call answered: the run of failures ends.
    pub(crate) fn succeeded(&mut self) {
        self.in_row = 0;
    }

    /// What follows `err` on a batch call of `size` elements (`again`: this same call was
    /// already retried once).
    pub(crate) fn after_batch(&mut self, err: &AiError, size: usize, again: bool) -> Next {
        let next = match (kind(err), size) {
            (Kind::Refused, _) => Next::GiveUp,
            (_, 0 | 1) => Next::Alone,
            (Kind::Transient, _) if !again => Next::Again,
            _ => Next::Split,
        };
        self.count(next, err)
    }

    /// What follows `err` on a single-element call (`again`: it was already retried once).
    pub(crate) fn after_single(&mut self, err: &AiError, again: bool) -> Next {
        let next = match kind(err) {
            Kind::Transient | Kind::TooBig if !again => Next::Again,
            _ => Next::GiveUp,
        };
        self.count(next, err)
    }

    /// No call is retried any more: [`MAX_FAILURES_IN_A_ROW`] calls failed in a row.
    pub(crate) fn exhausted(&self) -> bool {
        self.in_row >= MAX_FAILURES_IN_A_ROW
    }

    fn count(&mut self, next: Next, err: &AiError) -> Next {
        // A bad answer is no sign of a dead model: its splits end on their own.
        if kind(err) != Kind::BadAnswer {
            self.in_row += 1;
        }
        if next == Next::GiveUp || self.exhausted() {
            return Next::GiveUp;
        }
        self.retried += 1;
        next
    }
}

/// How a failed call is retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// The connection or the CLI failed: the same call may pass.
    Transient,
    /// The call was too big for its time or output limit: smaller calls may pass.
    TooBig,
    /// The answer failed validation after its repairs: smaller calls may pass.
    BadAnswer,
    /// Refused before or outside the model (budget, cancel, setup): never retried.
    Refused,
}

fn kind(err: &AiError) -> Kind {
    match err {
        AiError::Cli { .. } | AiError::NoResult | AiError::Io(_) => Kind::Transient,
        AiError::Timeout { .. } | AiError::OutputTooLarge { .. } => Kind::TooBig,
        AiError::Validation { .. } => Kind::BadAnswer,
        _ => Kind::Refused,
    }
}

/// One single-element generation call, retried once on a transient failure (see
/// [this module](self)). Returns the element, its notes, the attempts and the retry note (if retried).
pub(super) async fn generate_single(
    runner: &dyn AiRunner,
    g: &GenCtx<'_>,
    planned: &PlannedElement,
    opts: &RunOptions<'_>,
    progress: Progress<'_>,
    retries: &mut Retries,
) -> Result<(Value, Vec<String>, u32, Vec<String>), String> {
    let mut failed: Option<AiError> = None;
    loop {
        match run_element_call(runner, g, planned, "import_generate", None, None, opts).await {
            Ok((el, notes, attempts)) => {
                retries.succeeded();
                let retried = failed
                    .iter()
                    .map(|e| note(&e.to_string(), "once"))
                    .collect();
                return Ok((el, notes, attempts, retried));
            }
            Err(err) => {
                let again = failed.is_some();
                if retries.after_single(&err, again) != Next::Again {
                    return Err(err.to_string());
                }
                let msg = format!("retrying \"{}\" after: {err}", planned.name);
                tracing::warn!(element = %planned.id, "{msg}");
                emit(progress, Stage::Generate, 0, 0, &msg);
                failed = Some(err);
            }
        }
    }
}

/// A batch waiting for its call.
struct Queued {
    part: Vec<PlannedElement>,
    /// This same call was already retried once.
    again: bool,
    /// For a retry: how it is retried and the failure that caused it.
    retry: Option<(&'static str, String)>,
}

/// Generate the pending class and subclass features in batches (several per call). A failed
/// call is retried (again, in halves, or element by element on the single-element path, see
/// [this module](self)) before any element is marked failed. Returns the elements left to the
/// single-element path, with the failure that sent them there.
pub(super) async fn fill_batches(
    runner: &dyn AiRunner,
    ctx: &PackContext,
    store: &ContentStore,
    draft: &mut ImportDraft,
    opts: &RunOptions<'_>,
    progress: Progress<'_>,
    retries: &mut Retries,
) -> Vec<(String, String)> {
    let total = draft.elements.len();
    let mut alone = Vec::new();
    for batch in super::super::batch::plan_batches(draft) {
        let mut queue: std::collections::VecDeque<Queued> = vec![Queued {
            part: batch,
            again: false,
            retry: None,
        }]
        .into();
        while let Some(q) = queue.pop_front() {
            if let Some((_, err)) = q.retry.as_ref().filter(|_| retries.exhausted()) {
                fail_all(draft, &q.part, &stopped(err));
                continue;
            }
            let first = draft.elements.iter().position(|e| e.id == q.part[0].id);
            emit(
                progress,
                Stage::Generate,
                first.unwrap_or(0),
                total,
                &format!("{} elements", q.part.len()),
            );
            let parent = super::super::batch::batch_parent(draft, &q.part);
            let result = {
                let g = GenCtx {
                    ctx,
                    store,
                    draft,
                    parent,
                };
                super::super::batch::run_batch(runner, &g, &q.part, opts).await
            };
            match result {
                Ok((els, attempts)) => {
                    retries.succeeded();
                    let note = q.retry.map(|(how, err)| note(&err, how));
                    apply_batch(draft, &q.part, els, attempts, note);
                }
                Err(err) => {
                    let next = retries.after_batch(&err, q.part.len(), q.again);
                    alone.extend(queue_retry(draft, &mut queue, q.part, &err, next, progress));
                }
            }
        }
    }
    alone
}

/// The failure of a retry that was not made: no call is retried any more.
pub(super) fn stopped(err: &str) -> String {
    format!(
        "generation failed: {err} (not retried: {} model calls failed in a row)",
        MAX_FAILURES_IN_A_ROW
    )
}

fn fail_all(draft: &mut ImportDraft, part: &[PlannedElement], msg: &str) {
    for p in part {
        if let Some(e) = draft.elements.iter_mut().find(|e| e.id == p.id) {
            e.status = ElementStatus::Failed;
            e.issues = vec![msg.to_string()];
        }
    }
}

/// Store a batch's answer on its elements (with the retry note when it was a retry).
fn apply_batch(
    draft: &mut ImportDraft,
    part: &[PlannedElement],
    els: Vec<(Value, Vec<String>)>,
    attempts: u32,
    retried: Option<String>,
) {
    for (p, (el, notes)) in part.iter().zip(els) {
        let Some(e) = draft.elements.iter_mut().find(|e| e.id == p.id) else {
            continue;
        };
        e.json = el;
        e.repair_rounds = 0;
        e.status = match (notes.is_empty(), attempts > 1) {
            (false, _) => ElementStatus::NeedsReview,
            (true, true) => ElementStatus::Repaired,
            (true, false) => ElementStatus::Ok,
        };
        e.issues = notes;
        push_new(&mut e.normalised, retried.iter().cloned().collect());
    }
}

/// Act on a failed batch call: queue it again or in halves, leave single elements to the
/// single-element path (returned with the failure), or mark its elements failed.
fn queue_retry(
    draft: &mut ImportDraft,
    queue: &mut std::collections::VecDeque<Queued>,
    part: Vec<PlannedElement>,
    err: &AiError,
    next: Next,
    progress: Progress<'_>,
) -> Vec<(String, String)> {
    let n = part.len();
    let msg = match next {
        Next::Again => format!("retrying {n} element(s) after: {err}"),
        Next::Split => format!("retrying {n} element(s) in smaller batches after: {err}"),
        Next::Alone => format!("retrying \"{}\" on its own after: {err}", part[0].name),
        Next::GiveUp => format!("{n} element(s) failed: {err}"),
    };
    tracing::warn!("{msg}");
    emit(progress, Stage::Generate, 0, 0, &msg);
    let retry = |how| Some((how, err.to_string()));
    match next {
        Next::Again => {
            queue.push_front(Queued {
                part,
                again: true,
                retry: retry("as it was"),
            });
            Vec::new()
        }
        Next::Split => {
            // Halves of one element go to the single-element path.
            let (a, b) = part.split_at(n.div_ceil(2));
            let mut alone = Vec::new();
            for half in [b, a] {
                if half.len() == 1 {
                    alone.extend(leave_alone(draft, half, err));
                } else {
                    queue.push_front(Queued {
                        part: half.to_vec(),
                        again: false,
                        retry: retry("in a smaller batch"),
                    });
                }
            }
            alone.reverse();
            alone
        }
        Next::Alone => leave_alone(draft, &part, err),
        Next::GiveUp => {
            fail_all(draft, &part, &format!("generation failed: {err}"));
            Vec::new()
        }
    }
}

/// The elements stay pending: the single-element path generates them.
fn leave_alone(
    draft: &mut ImportDraft,
    part: &[PlannedElement],
    err: &AiError,
) -> Vec<(String, String)> {
    let note = note(&err.to_string(), "on its own");
    for p in part {
        if let Some(e) = draft.elements.iter_mut().find(|e| e.id == p.id) {
            push_new(&mut e.normalised, vec![note.clone()]);
        }
    }
    part.iter()
        .map(|p| (p.id.clone(), err.to_string()))
        .collect()
}

/// The note (in the import report's normalised list) of an element generated by a retried call.
pub(crate) fn note(err: &str, how: &str) -> String {
    format!("generation retried {how} after a failed model call ({err})")
}

#[cfg(test)]
#[path = "retry_tests.rs"]
mod tests;
