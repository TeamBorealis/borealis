//! Review-time operations of "Import anything": read a draft, edit an element by hand, commit the
//! draft as a pack in the user library, or throw it away.

use crate::content_dto::ContentPack;
use crate::content_import::{slug, write_pack_meta, Scratch};
use crate::content_packs::EventSink;
use crate::error::{ApiError, ApiResult};
use crate::import_check::{check_content, content_doc};
use crate::import_drafts::StoredDraft;
use crate::import_dto::{draft_dto, ImportDraftDto};
use crate::import_service::assist_err;
use crate::state::AppState;
use borealis_assist::import::anything::review::{review_report, ReviewOverrides};
use borealis_assist::import::anything::{DraftElement, ElementStatus};
use borealis_assist::{finalize_draft, ImportDraft};
use serde_json::{json, Value};
use std::collections::HashSet;

/// Longest pack name.
const MAX_PACK_NAME_CHARS: usize = 80;
/// Longest id prefix.
const MAX_PREFIX_CHARS: usize = 8;

fn valid_prefix(prefix: &str) -> bool {
    (1..=MAX_PREFIX_CHARS).contains(&prefix.len())
        && prefix
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The upper-case id token of a prefix, as the pipeline writes it into ids.
fn id_token(prefix: &str) -> String {
    let mut out = String::new();
    for c in prefix.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_uppercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    format!("ID_{}_", out.trim_matches('_'))
}

fn replace_in(v: &mut Value, from: &str, to: &str) {
    match v {
        Value::String(s) if s.contains(from) => *s = s.replace(from, to),
        Value::Array(a) => a.iter_mut().for_each(|x| replace_in(x, from, to)),
        Value::Object(o) => o.values_mut().for_each(|x| replace_in(x, from, to)),
        _ => {}
    }
}

/// Rewrite the id prefix of every element and every reference to it.
fn reprefix(draft: &mut ImportDraft, new: &str) {
    let (from, to) = (id_token(&draft.id_prefix), id_token(new));
    if from != to {
        for e in &mut draft.elements {
            e.id = e.id.replace(&from, &to);
            replace_in(&mut e.json, &from, &to);
        }
    }
    draft.id_prefix = new.to_string();
}

impl AppState {
    /// A draft as the review screen shows it.
    ///
    /// # Errors
    /// `not_found`; `invalid_argument` with the reason when the run failed; `requirement_failed`
    /// while nothing exists yet.
    pub fn import_get_draft(&self, id: &str) -> ApiResult<ImportDraftDto> {
        let stored = self.drafts.get(&self.paths, id)?;
        match stored.draft {
            Some(d) => Ok(draft_dto(id, &stored.stage, &d, &stored.review)),
            None => Err(match stored.error {
                Some(msg) => ApiError::invalid(msg),
                None => ApiError::requirement("The import is still running."),
            }),
        }
    }

    /// Replace one element's JSON by hand and re-validate it (no model call).
    ///
    /// # Errors
    /// `not_found`, `requirement_failed` while a run is active, `invalid_argument` when the id or
    /// type would change.
    pub fn import_update_element(
        &self,
        id: &str,
        element_id: &str,
        json: Value,
    ) -> ApiResult<ImportDraftDto> {
        if self.drafts.is_running(id) {
            return Err(ApiError::requirement(
                "The AI is still working on this import.",
            ));
        }
        let mut stored = self.drafts.get(&self.paths, id)?;
        let mut draft = stored
            .draft
            .take()
            .ok_or_else(|| ApiError::requirement("This import has no draft yet."))?;
        let idx = draft
            .elements
            .iter()
            .position(|e| e.id == element_id)
            .ok_or_else(|| ApiError::not_found("That element is not in this import."))?;
        if json.get("id").and_then(Value::as_str) != Some(element_id)
            || json.get("type").and_then(Value::as_str) != Some(draft.elements[idx].ty.as_str())
        {
            return Err(ApiError::invalid(
                "An element's id and type cannot be changed here.",
            ));
        }
        let name = json
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let docs: Vec<Value> = draft
            .elements
            .iter()
            .enumerate()
            .map(|(i, e)| {
                if i == idx {
                    json.clone()
                } else {
                    e.json.clone()
                }
            })
            .collect();
        let hub = self.hub()?;
        let check = check_content(&hub.builder, draft.edition.ruleset(), &content_doc(&docs));
        let mine: Vec<String> = check
            .errors
            .iter()
            .filter(|m| m.contains(element_id))
            .cloned()
            .collect();
        let errors = if mine.is_empty() && check.reports.is_empty() {
            check.errors.clone()
        } else {
            mine
        };
        let el = &mut draft.elements[idx];
        if !name.is_empty() {
            el.name = name;
        }
        el.json = json;
        el.status = if errors.is_empty() {
            ElementStatus::Ok
        } else {
            ElementStatus::Failed
        };
        el.issues = errors;
        el.balance = check.reports.get(idx).cloned();
        el.revisions += 1;
        stored.review.prune(&draft);
        let dto = draft_dto(id, &stored.stage, &draft, &stored.review);
        stored.draft = Some(draft);
        self.drafts.put(&self.paths, id, stored)?;
        Ok(dto)
    }

    /// Commit the draft as a pack in the user library, hot-reload, and drop the draft.
    ///
    /// # Errors
    /// `requirement_failed` while the draft is not ready (open parent choice, failed elements, a
    /// run in progress), `invalid_argument` for a bad name or prefix, `content_error`.
    pub fn import_finalize(
        &self,
        id: &str,
        pack_name: &str,
        prefix: &str,
        sink: &dyn EventSink,
    ) -> ApiResult<ContentPack> {
        let name = pack_name.trim();
        if name.is_empty() || name.chars().count() > MAX_PACK_NAME_CHARS {
            return Err(ApiError::invalid(
                "Give the pack a name of up to 80 characters.",
            ));
        }
        if !valid_prefix(prefix) {
            return Err(ApiError::invalid(
                "The id prefix is 1 to 8 lower-case letters, digits or dashes.",
            ));
        }
        if self.drafts.is_running(id) {
            return Err(ApiError::requirement(
                "The AI is still working on this import.",
            ));
        }
        let stored = self.drafts.get(&self.paths, id)?;
        let review = stored.review;
        let mut draft = stored
            .draft
            .ok_or_else(|| ApiError::requirement("This import has no draft yet."))?;
        if !draft.plan.open_choices().is_empty() {
            return Err(ApiError::requirement("Pick the parent class first."));
        }
        if draft.elements.is_empty() {
            return Err(ApiError::requirement("There is nothing to add yet."));
        }
        // An element with no output ("not imported") is left out of the pack and listed in the
        // report; one the engine rejects but that has output must be fixed first.
        if draft
            .elements
            .iter()
            .any(|e| e.status == ElementStatus::Failed && !e.json.is_null())
        {
            return Err(ApiError::requirement(
                "Fix or revise the failed elements first.",
            ));
        }
        let _guard = self.content_lock()?;
        let taken: HashSet<String> = self
            .installed_packs()
            .into_iter()
            .map(|p| p.manifest.id)
            .collect();
        let base_id = slug(name);
        let pack_id = std::iter::once(base_id.clone())
            .chain((2..).map(|n| format!("{base_id}-{n}")))
            .find(|c| !taken.contains(c))
            .unwrap_or(base_id);
        reprefix(&mut draft, prefix);
        draft.pack_name = name.to_string();
        draft.pack_id = pack_id.clone();
        let hub = self.hub()?;
        let mut finalized = finalize_draft(&hub.builder, &draft).map_err(|e| assist_err(&e))?;
        // The review states and the user's overrides (text only, accepted) live in the pack.
        if let Some(report) = finalized.pack.report.as_mut() {
            report.review = Some(review_report(&draft, &review, &finalized.renamed));
        }
        let scratch = Scratch::new(&self.paths.data)?;
        let out = scratch.0.join("out");
        finalized
            .pack
            .write_to_dir(&out)
            .map_err(|e| assist_err(&e))?;
        write_pack_meta(&out, "imported", &json!({ "fileName": draft.source_name }));
        self.install_packs(&[(out, pack_id.clone())], sink, "pack-imported")?;
        self.drafts.remove(&self.paths, id)?;
        self.list_packs()?
            .into_iter()
            .find(|p| p.id == pack_id)
            .ok_or_else(|| ApiError::internal("the imported pack is missing after install"))
    }

    /// Mark one element as fine as it is (`false` clears the mark). The mark lapses when the
    /// element changes.
    ///
    /// # Errors
    /// `not_found`, `requirement_failed` while a run is active or with no draft.
    pub fn import_accept_element(
        &self,
        id: &str,
        element_id: &str,
        accepted: bool,
    ) -> ApiResult<ImportDraftDto> {
        self.update_review(id, element_id, |review, el| {
            review.set_accepted(el, accepted);
        })
    }

    /// Set (`Some`) or clear (`None`) whether the sheet computes one element's mechanics.
    ///
    /// # Errors
    /// `not_found`, `requirement_failed` while a run is active or with no draft.
    pub fn import_set_text_only(
        &self,
        id: &str,
        element_id: &str,
        text_only: Option<bool>,
    ) -> ApiResult<ImportDraftDto> {
        self.update_review(id, element_id, |review, el| {
            review.set_text_only(&el.id, text_only);
        })
    }

    fn update_review(
        &self,
        id: &str,
        element_id: &str,
        change: impl FnOnce(&mut ReviewOverrides, &DraftElement),
    ) -> ApiResult<ImportDraftDto> {
        if self.drafts.is_running(id) {
            return Err(ApiError::requirement(
                "The AI is still working on this import.",
            ));
        }
        let stored = self.drafts.get(&self.paths, id)?;
        let draft = stored
            .draft
            .as_ref()
            .ok_or_else(|| ApiError::requirement("This import has no draft yet."))?;
        let el = draft
            .element(element_id)
            .ok_or_else(|| ApiError::not_found("That element is not in this import."))?;
        let mut review = stored.review.clone();
        change(&mut review, el);
        let dto = draft_dto(id, &stored.stage, draft, &review);
        self.drafts
            .put(&self.paths, id, StoredDraft { review, ..stored })?;
        Ok(dto)
    }

    /// Delete a draft (a running one is left to finish and is then simply unreferenced).
    ///
    /// # Errors
    /// `invalid_argument` for a bad id.
    pub fn import_discard(&self, id: &str) -> ApiResult<()> {
        self.drafts.remove(&self.paths, id)
    }
}
