//! Homebrew creator overrides (HC20, `specs/homebrew-creator-spec.md` section 22): a loaded element the user did not
//! make (bundled SRD, imported books) opened in the creator, changed, and saved in a homebrew pack's `overrides` with
//! its original's id, so it replaces the original everywhere (characters keep their picks). Revert removes it.
//!
//! The pure steps are `borealis_authoring::pack::{split_override, revert_override, ...}`; this module finds the
//! originals in the loaded content, checks what may be overridden, writes the pack through the creator's save step
//! ([`AppState::hb_commit`]) and builds the views ("Changed by", the creator's notice).

use crate::content_packs::{creator_editable, EventSink};
use crate::error::{ApiError, ApiResult};
use crate::homebrew_draft_api::wire_problem;
use crate::homebrew_drafts::{created_now, new_draft_id, DraftFile, HB_DRAFT_FORMAT};
use crate::homebrew_dto::{
    HomebrewDeleteResult, HomebrewDraftView, HomebrewOverrideInfo, HomebrewProblem,
    HomebrewSaveResult,
};
use crate::homebrew_files::PackWrite;
use crate::homebrew_packs::{CommitError, CreatorPack};
use crate::ids::rfc3339;
use crate::state::{AppState, ContentHub, BUNDLED_PACKS};
use borealis_authoring::check::{issue_problems, Problem};
use borealis_authoring::form::{
    assign_ids, form_to_elements, override_form, CatalogBlockCodec, CopyExtras, FormContext,
    FormState, WithElements,
};
use borealis_authoring::pack::{
    bump_patch, digest, original_changed, override_parts, revert_override, source_name,
    split_override, with_elements, with_overrides, without_overrides, CopiedFrom, CreatorElement,
    CreatorOverride, SOURCE_TYPE,
};
use borealis_content::{ContentStore, Element, StoredElement};
use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

/// "Changed by <pack>" on an overridden element (spec 22.6).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementChangedBy {
    pub pack_id: String,
    pub pack_name: String,
}

/// The pack id of a store file name (`<pack id>/<file>`).
fn pack_of(file: &str) -> &str {
    file.split('/').next().unwrap_or_default()
}

/// The file that defines the original of a loaded element: the file of the element an override replaced, else its own.
fn defining_file(stored: &StoredElement) -> &str {
    stored
        .overridden
        .as_ref()
        .map_or(&*stored.file, |o| &*o.file)
}

/// The original of a loaded element as loaded: the element an override replaced, else the element itself.
fn original_of(stored: &StoredElement) -> &Element {
    stored
        .overridden
        .as_ref()
        .map_or(stored.element.as_ref(), |o| o.element.as_ref())
}

/// "Changed by <pack>" for the loaded element `id`, when an override replaces it.
#[must_use]
pub(crate) fn changed_by(store: &ContentStore, id: &str) -> Option<ElementChangedBy> {
    let stored = store.get_exact(id)?;
    stored.overridden.as_ref()?;
    let pack_id = pack_of(&stored.file).to_string();
    let pack_name = store
        .packs()
        .iter()
        .find(|p| p.id == pack_id)
        .map_or_else(|| pack_id.clone(), |p| p.name.clone());
    Some(ElementChangedBy { pack_id, pack_name })
}

/// Where the original of `stored` came from (CS1 provenance; `srd` = a bundled pack).
fn provenance(stored: &StoredElement) -> CopiedFrom {
    let pack_id = pack_of(defining_file(stored)).to_string();
    CopiedFrom {
        id: stored.element.id.clone(),
        srd: BUNDLED_PACKS.contains(&pack_id.as_str()),
        pack_id,
    }
}

fn now() -> String {
    rfc3339(SystemTime::now())
}

impl AppState {
    /// The loaded original of `id` for the pack `pack_id`: the element of another pack (or the element an override of
    /// another pack's element replaced). `None` for an id only the pack itself defines, or none.
    fn hb_original<'s>(
        store: &'s ContentStore,
        pack_id: &str,
        id: &str,
    ) -> Option<&'s StoredElement> {
        let stored = store.get_exact(id)?;
        (pack_of(defining_file(stored)) != pack_id).then_some(stored)
    }

    /// Why `stored` can't be overridden in the pack `pack_id`, as the sentence the UI shows (spec 22.1).
    fn hb_override_refusal(
        &self,
        store: &ContentStore,
        pack_id: &str,
        stored: &StoredElement,
    ) -> Option<String> {
        if stored.element.ty == SOURCE_TYPE {
            return Some("A book element can't be changed.".into());
        }
        let home = pack_of(defining_file(stored));
        if let Some(own) = self
            .installed_packs()
            .into_iter()
            .find(|p| p.manifest.id == home && creator_editable(p))
        {
            return Some(format!(
                "That element is in your pack {}. Open it there to change it.",
                own.manifest.name
            ));
        }
        if stored.overridden.is_some() && pack_of(&stored.file) != pack_id {
            let by =
                changed_by(store, &stored.element.id).map_or_else(String::new, |c| c.pack_name);
            return Some(format!(
                "{by} already changes this element. Change it there, or revert that change first."
            ));
        }
        None
    }

    /// A new override (spec 22.1, 22.4): the loaded element `element_id` opened in the creator with its children inside
    /// it, every id kept. An element the pack already overrides opens that override.
    ///
    /// # Errors
    /// As `hb_pack`; `not_found` for an element that isn't loaded; `invalid_argument` with the sentence of
    /// [`Self::hb_override_refusal`].
    pub fn homebrew_new_override(
        &self,
        pack_id: &str,
        element_id: &str,
    ) -> ApiResult<HomebrewDraftView> {
        let pack = self.hb_pack(pack_id)?;
        if pack
            .disk
            .content
            .overrides
            .iter()
            .any(|e| e.id == element_id)
        {
            return self.homebrew_open_draft(pack_id, Some(element_id), None);
        }
        let hub = self.hub()?;
        let stored = hub
            .store
            .get_exact(element_id)
            .ok_or_else(|| ApiError::not_found("That element isn't loaded any more."))?;
        if let Some(why) = self.hb_override_refusal(&hub.store, pack_id, stored) {
            return Err(ApiError::invalid(why));
        }
        let draft = Self::hb_override_draft(&hub, &pack, element_id)?;
        self.hb_drafts.hold(draft.clone())?;
        self.hb_view(&draft, &pack)
    }

    /// The draft of an override (new, or of one the pack saved): its form from the loaded content with the pack's saved
    /// overrides on top (they count even while the pack is off); its base is the pack's saved overrides of this edit.
    pub(crate) fn hb_override_draft(
        hub: &ContentHub,
        pack: &CreatorPack,
        element_id: &str,
    ) -> ApiResult<DraftFile> {
        let saved = &pack.disk.content.overrides;
        let lookup = WithElements {
            base: &*hub.store,
            extra: saved,
        };
        let codec = CatalogBlockCodec;
        let cx = FormContext {
            lookup: &lookup,
            ..FormContext::from_store(&hub.store, &codec)
        };
        let el = saved
            .iter()
            .find(|e| e.id == element_id)
            .or_else(|| hub.store.element(element_id))
            .ok_or_else(|| ApiError::not_found("That element isn't loaded any more."))?;
        let form = override_form(el, &cx);
        let base_ids = override_parts(&pack.disk.content, &pack.creator, element_id);
        let base: Vec<&Element> = base_ids
            .iter()
            .filter_map(|id| saved.iter().find(|e| &e.id == id))
            .collect();
        Ok(DraftFile {
            format: HB_DRAFT_FORMAT.into(),
            draft_id: new_draft_id(),
            pack_id: pack.id().to_string(),
            element_id: Some(element_id.to_string()),
            kind: form.kind.clone(),
            form,
            base_digest: if base.is_empty() {
                String::new()
            } else {
                digest(&base)
            },
            base_ids,
            updated_at: now(),
            copied_from: None,
            copy: CopyExtras::default(),
            provisional: Vec::new(),
            created: created_now(),
            is_override: true,
        })
    }

    /// The override notice of a draft (spec 22.6): the original's book, whether it is SRD, changed underneath, or not
    /// loaded.
    pub(crate) fn hb_override_info(
        hub: &ContentHub,
        pack: &CreatorPack,
        draft: &DraftFile,
    ) -> Option<HomebrewOverrideInfo> {
        if !draft.is_override {
            return None;
        }
        let id = draft.element_id.as_deref()?;
        Some(Self::hb_override_state(hub, pack, id))
    }

    /// The state of the pack's override of `id` (also the element list's note).
    pub(crate) fn hb_override_state(
        hub: &ContentHub,
        pack: &CreatorPack,
        id: &str,
    ) -> HomebrewOverrideInfo {
        let entry = pack.creator.overrides.get(id);
        match Self::hb_original(&hub.store, pack.id(), id) {
            Some(stored) => {
                let original = original_of(stored);
                let from = provenance(stored);
                HomebrewOverrideInfo {
                    original_id: id.to_string(),
                    source_name: original.source.clone(),
                    srd: from.srd,
                    changed: original_changed(entry, &digest(&[original])),
                    missing: false,
                }
            }
            None => HomebrewOverrideInfo {
                original_id: id.to_string(),
                source_name: pack
                    .disk
                    .content
                    .overrides
                    .iter()
                    .find(|e| e.id == id)
                    .map(|e| e.source.clone())
                    .unwrap_or_default(),
                srd: entry
                    .and_then(|o| o.original.as_ref())
                    .is_some_and(|o| o.srd),
                changed: false,
                missing: true,
            },
        }
    }

    /// Save an override draft (spec 22.4): the main element and each changed child to the pack's `overrides`, a child
    /// back to equal its original dropped, a new child as the pack's own element; `creator.json` records each override
    /// with its original's digest (change detection). Checked, written and reloaded like any save.
    pub(crate) fn hb_save_override(
        &self,
        draft: &DraftFile,
        sink: &dyn EventSink,
    ) -> ApiResult<HomebrewSaveResult> {
        let pack = self.hb_pack(&draft.pack_id)?;
        let hub = self.hub()?;
        let codec = CatalogBlockCodec;
        let cx = FormContext::from_store(&hub.store, &codec);
        let main = draft
            .element_id
            .clone()
            .ok_or_else(|| ApiError::internal("an override draft without its element"))?;
        let refuse = |problems: Vec<HomebrewProblem>| {
            let mut e = ApiError::invalid("Fix the problems marked in red before saving.");
            e.details = serde_json::to_value(&problems).ok();
            e
        };
        let before: HashSet<&str> = draft.base_ids.iter().map(String::as_str).collect();
        let saved_before: HashSet<&str> = before.iter().copied().chain([main.as_str()]).collect();
        let mut ids = self.hb_ids(&hub, &pack)?;
        let form = assign_ids(&draft.form, &mut ids);
        let errors: Vec<HomebrewProblem> = Self::hb_live(&hub, &pack, draft, &form, &cx)
            .into_iter()
            .filter(Problem::is_error)
            .map(|p| wire_problem(p, &saved_before))
            .collect();
        if !errors.is_empty() {
            return Err(refuse(errors));
        }
        let written = form_to_elements(&form, &cx).map_err(|ps| {
            refuse(
                ps.into_iter()
                    .map(|p| HomebrewProblem {
                        severity: "error".into(),
                        message: p.message,
                        path: (!p.path.is_empty()).then_some(p.path),
                        element_id: None,
                        code: "form".into(),
                    })
                    .collect(),
            )
        })?;
        let kinds: HashMap<String, String> = form
            .flatten()
            .into_iter()
            .filter_map(|f| f.id().map(|id| (id.to_string(), f.kind.clone())))
            .collect();
        let pack_id = pack.id().to_string();
        let split = split_override(
            written.clone(),
            |id| Self::hb_original(&hub.store, &pack_id, id).map(original_of),
            &before,
            &source_name(&pack.disk.manifest, &pack.disk.content),
        );
        let reverted: HashSet<&str> = split.reverted.iter().map(String::as_str).collect();
        let (content, trashed) = without_overrides(
            &with_overrides(&pack.disk.content, &split.overrides),
            &reverted,
        );
        let content = with_elements(&content, &split.own);
        let at = now();
        let kind_of = |id: &str| kinds.get(id).cloned().unwrap_or_else(|| "other".into());
        let mut creator = pack.creator.without_overrides(reverted.iter().copied());
        for e in &split.overrides {
            let Some(stored) = Self::hb_original(&hub.store, &pack_id, &e.id) else {
                continue;
            };
            let from = provenance(stored);
            creator = creator.with_override(
                &e.id,
                CreatorOverride {
                    kind: kind_of(&e.id),
                    created_at: at.clone(),
                    non_srd_text: !from.srd,
                    original: Some(from),
                    original_digest: digest(&[original_of(stored)]),
                    part_of: (e.id != main).then(|| main.clone()),
                },
            );
        }
        for e in &split.own {
            creator = creator.with_saved(
                &e.id,
                CreatorElement {
                    kind: kind_of(&e.id),
                    created_at: at.clone(),
                    copied_from: None,
                    non_srd_text: false,
                },
            );
        }
        let mut manifest = pack.disk.manifest.clone();
        manifest.version = bump_patch(&manifest.version);
        let next = PackWrite {
            manifest,
            content,
            creator,
            trashed,
        };
        let checked: Vec<String> = written.iter().map(|e| e.id.clone()).collect();
        match self.hb_commit(&pack, &next, &checked, sink) {
            Ok(()) => {}
            Err(CommitError::Api(e)) => return Err(e),
            Err(CommitError::Problems(issues)) => {
                return Err(refuse(
                    issue_problems(&issues, &form, &written, &cx)
                        .into_iter()
                        .map(|p| wire_problem(p, &saved_before))
                        .collect(),
                ));
            }
        }
        self.hb_drafts.remove(&self.paths.data, &draft.draft_id)?;
        Ok(HomebrewSaveResult {
            pack: self.hb_pack_dto(&draft.pack_id)?,
            element_ids: checked,
        })
    }

    /// Revert (spec 22.6): the pack's override of `element_id` and the child overrides saved with it are removed, the
    /// original returns. Nothing is retired and no character changes, so nothing is asked first; undoable like a delete.
    ///
    /// # Errors
    /// As `hb_pack`; `not_found` when the pack doesn't override that element.
    pub fn homebrew_revert_override(
        &self,
        pack_id: &str,
        element_id: &str,
        sink: &dyn EventSink,
    ) -> ApiResult<HomebrewDeleteResult> {
        let _guard = self.content_lock()?;
        let pack = self.hb_pack(pack_id)?;
        let parts = override_parts(&pack.disk.content, &pack.creator, element_id);
        if parts.is_empty() {
            return Err(ApiError::not_found(
                "This pack doesn't change that element any more.",
            ));
        }
        let (content, creator, trashed) =
            revert_override(&pack.disk.content, &pack.creator, element_id);
        let mut manifest = pack.disk.manifest.clone();
        manifest.version = bump_patch(&manifest.version);
        let next = PackWrite {
            manifest,
            content,
            creator,
            trashed,
        };
        let before = crate::homebrew_undo::PackBefore::of(&pack, &next);
        match self.hb_commit(&pack, &next, &[], sink) {
            Ok(()) => {}
            Err(CommitError::Api(e)) => return Err(e),
            Err(CommitError::Problems(_)) => {
                return Err(ApiError::internal("a revert failed a check"))
            }
        }
        self.hb_drafts
            .remove_for_elements(&self.paths.data, pack_id, &parts)?;
        Ok(HomebrewDeleteResult {
            deleted: true,
            used_by: Vec::new(),
            pack: Some(self.hb_pack_dto(pack_id)?),
            undo_id: Some(self.hb_undo.record(vec![before])?),
        })
    }

    /// The ids an override draft overrides (exempt from the id-clash warning, spec 22.5): every id of its forms.
    pub(crate) fn hb_override_ids(draft: &DraftFile, form: &FormState) -> HashSet<String> {
        if !draft.is_override {
            return HashSet::new();
        }
        form.flatten()
            .into_iter()
            .filter_map(FormState::id)
            .map(str::to_string)
            .collect()
    }
}
