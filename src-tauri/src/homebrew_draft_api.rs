//! Homebrew creator drafts as commands (spec 10, 11, 12.3): new (blank or a copy), open (or resume), every edit,
//! save, discard, the list, duplicate.
//!
//! The draft file holds the full form; the UI gets `to_ui` of it and sends its edits back, which are merged by key
//! (`merge_ui`), so ids and kept bytes never travel and can't be forged. Ids are given on the first save
//! (`assign_ids`, spec 5.3) over every loaded id and the pack's retired ids; until then previews use provisional ids.

use crate::compendium::element_detail;
use crate::content_packs::EventSink;
use crate::error::{ApiError, ApiResult};
use crate::homebrew_drafts::{new_draft_id, DraftFile, HB_DRAFT_FORMAT};
use crate::homebrew_dto::{
    HomebrewDraftSummary, HomebrewDraftView, HomebrewGeneratedView, HomebrewProblem,
    HomebrewSaveResult,
};
use crate::homebrew_files::PackWrite;
use crate::homebrew_packs::{CommitError, CreatorPack};
use crate::ids::rfc3339;
use crate::state::{AppState, ContentHub, BUNDLED_PACKS};
use borealis_authoring::form::{
    assign_ids, check_fields, element_group_to_form, element_to_form, form_path_for_issue, kind,
    merge_ui, refresh_blocks, to_ui, CatalogBlockCodec, FormContext, FormProblem, FormState,
};
use borealis_authoring::pack::{
    bump_patch, child_types, digest, granted_children, source_name, with_elements,
    without_elements, CopiedFrom, CreatorElement, SOURCE_TYPE,
};
use borealis_content::{Element, Issue};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::SystemTime;

/// The problem code of a form's own problems (builder values, shapes, a missing name).
const FORM_CODE: &str = "form";
/// The problem code of an empty name.
const REQUIRED_CODE: &str = "form.required";

fn now() -> String {
    rfc3339(SystemTime::now())
}

fn problem(
    path: &str,
    message: impl Into<String>,
    code: &str,
    element: Option<&str>,
) -> HomebrewProblem {
    HomebrewProblem {
        severity: "error".into(),
        message: message.into(),
        path: (!path.is_empty()).then(|| path.to_string()),
        element_id: element.map(str::to_string),
        code: code.to_string(),
    }
}

/// The form's own problems: an empty name, then builder values and shapes (`check_fields`), at form paths.
fn form_problems(
    form: &FormState,
    cx: &FormContext<'_>,
    element: Option<&str>,
) -> Vec<HomebrewProblem> {
    let mut out = Vec::new();
    let named = form
        .fields
        .get("name")
        .and_then(Value::as_str)
        .is_some_and(|n| !n.trim().is_empty());
    if kind(&form.kind).is_some_and(|k| k.field("name").is_some()) && !named {
        out.push(problem(
            "fields.name",
            "Give it a name.",
            REQUIRED_CODE,
            element,
        ));
    }
    out.extend(
        check_fields(form, cx)
            .into_iter()
            .map(|FormProblem { path, message }| problem(&path, message, FORM_CODE, element)),
    );
    out
}

/// A copy of `form` with no ids (a new element), named "<name> (copy)".
fn as_copy(form: &FormState) -> FormState {
    let mut copy = form.clone();
    copy.memo.id = None;
    copy.children.clear();
    let name = form
        .fields
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    copy.fields
        .insert("name".into(), json!(format!("{name} (copy)")));
    copy
}

impl AppState {
    fn hb_context<'a>(hub: &'a ContentHub, codec: &'a CatalogBlockCodec) -> FormContext<'a> {
        FormContext::from_store(&hub.store, codec)
    }

    /// The saved elements a draft started from, as they are now (`None` for one that is gone).
    fn hb_base<'p>(pack: &'p CreatorPack, ids: &[String]) -> Vec<Option<&'p Element>> {
        ids.iter()
            .map(|id| pack.disk.content.elements.iter().find(|e| &e.id == id))
            .collect()
    }

    fn hb_base_changed(pack: &CreatorPack, draft: &DraftFile) -> bool {
        if draft.base_ids.is_empty() {
            return false;
        }
        let now: Option<Vec<&Element>> = Self::hb_base(pack, &draft.base_ids).into_iter().collect();
        now.is_none_or(|els| digest(&els) != draft.base_digest)
    }

    /// The view the UI gets: the form without its memo, the live preview, the form's problems, the generated data.
    fn hb_view(&self, draft: &DraftFile, pack: &CreatorPack) -> ApiResult<HomebrewDraftView> {
        let hub = self.hub()?;
        let codec = CatalogBlockCodec;
        let cx = Self::hb_context(&hub, &codec);
        let mut ids = self.hb_ids(&hub, pack)?;
        let provisional = assign_ids(&draft.form, &mut ids);
        let elements = borealis_authoring::form::form_to_elements(&provisional, &cx).ok();
        let preview = elements
            .as_ref()
            .and_then(|els| els.first())
            .map(|e| element_detail(&hub.store, e));
        let json = elements
            .as_ref()
            .and_then(|els| serde_json::to_string_pretty(els).ok())
            .unwrap_or_default();
        Ok(HomebrewDraftView {
            draft_id: draft.draft_id.clone(),
            pack_id: draft.pack_id.clone(),
            element_id: draft.element_id.clone(),
            form: to_ui(&draft.form),
            preview,
            problems: form_problems(&draft.form, &cx, draft.element_id.as_deref()),
            generated: HomebrewGeneratedView {
                json,
                tree: Vec::new(),
            },
            updated_at: draft.updated_at.clone(),
            base_changed: Self::hb_base_changed(pack, draft),
        })
    }

    fn hb_new_draft_file(
        pack: &CreatorPack,
        form: FormState,
        copied_from: Option<CopiedFrom>,
    ) -> DraftFile {
        DraftFile {
            format: HB_DRAFT_FORMAT.into(),
            draft_id: new_draft_id(),
            pack_id: pack.id().to_string(),
            element_id: None,
            kind: form.kind.clone(),
            form,
            base_ids: Vec::new(),
            base_digest: String::new(),
            updated_at: now(),
            copied_from,
        }
    }

    /// A new element (spec 10.1): blank, or a copy of any loaded element (spec 11; the copy keeps the rules, tags and
    /// description, gets "<Name> (copy)" and a new id on its first save; where it came from goes to `creator.json`).
    /// Nothing is written until the first edit.
    ///
    /// # Errors
    /// As `hb_pack`; `invalid_argument` for an unknown kind or a copy of a book element; `not_found` for an unknown
    /// copy; `requirement_failed` (not implemented) for a new child (`parentId`, WB-16/WB-18) and a deep copy of an
    /// element with features (WB-22).
    pub fn homebrew_new_draft(
        &self,
        pack_id: &str,
        kind_id: &str,
        copy_from: Option<&str>,
        parent_id: Option<&str>,
        deep_copy: Option<bool>,
    ) -> ApiResult<HomebrewDraftView> {
        if parent_id.is_some() {
            return Err(crate::homebrew_creator::not_implemented(
                "homebrew_new_draft (parentId)",
            ));
        }
        let pack = self.hb_pack(pack_id)?;
        let hub = self.hub()?;
        let source = source_name(&pack.disk.manifest, &pack.disk.content);
        let (form, copied_from) = match copy_from {
            Some(id) => {
                let stored = hub
                    .store
                    .get(id)
                    .ok_or_else(|| ApiError::not_found("That element isn't loaded any more."))?;
                let el = &stored.element;
                if el.ty == SOURCE_TYPE {
                    return Err(ApiError::invalid("A book element can't be copied."));
                }
                if deep_copy == Some(true) && has_loaded_children(el, &hub) {
                    return Err(crate::homebrew_creator::not_implemented(
                        "homebrew_new_draft (deepCopy)",
                    ));
                }
                let codec = CatalogBlockCodec;
                let cx = Self::hb_context(&hub, &codec);
                let mut form = as_copy(&element_to_form(el, &cx));
                form.fields.insert("source".into(), json!(source));
                let from_pack = stored
                    .file
                    .split('/')
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let srd = BUNDLED_PACKS.contains(&from_pack.as_str());
                (
                    form,
                    Some(CopiedFrom {
                        id: el.id.clone(),
                        pack_id: from_pack,
                        srd,
                    }),
                )
            }
            None => {
                let k = kind(kind_id).ok_or_else(|| {
                    ApiError::invalid(format!("There is no form called {kind_id:?}."))
                })?;
                let mut form = FormState::new("e", k.id);
                form.fields.insert("name".into(), json!(""));
                if k.field("source").is_some() {
                    form.fields.insert("source".into(), json!(source));
                }
                (form, None)
            }
        };
        let draft = Self::hb_new_draft_file(&pack, form, copied_from);
        self.hb_drafts.hold(draft.clone())?;
        self.hb_view(&draft, &pack)
    }

    /// Open a saved element of the pack (spec 10.1), resuming its edited draft when it has one; or, with `draft_id`,
    /// resume that draft (a new element's draft has no element to open it by). The element is edited with its
    /// children (features, traits) inside it.
    ///
    /// # Errors
    /// As `hb_pack`; `not_found` for an element not in the pack or an unknown draft; `invalid_argument` with
    /// neither an element nor a draft.
    pub fn homebrew_open_draft(
        &self,
        pack_id: &str,
        element_id: Option<&str>,
        draft_id: Option<&str>,
    ) -> ApiResult<HomebrewDraftView> {
        let pack = self.hb_pack(pack_id)?;
        if let Some(id) = draft_id {
            let draft = self.hb_drafts.get(&self.paths.data, id)?;
            if draft.pack_id != pack_id {
                return Err(ApiError::not_found("That draft belongs to another pack."));
            }
            return self.hb_view(&draft, &pack);
        }
        let element_id =
            element_id.ok_or_else(|| ApiError::invalid("Say which element to open."))?;
        if let Some(draft) = self
            .hb_drafts
            .for_element(&self.paths.data, pack_id, element_id)?
        {
            return self.hb_view(&draft, &pack);
        }
        let elements = &pack.disk.content.elements;
        let el = elements
            .iter()
            .find(|e| e.id == element_id && e.ty != SOURCE_TYPE)
            .ok_or_else(|| ApiError::not_found("That element isn't in this pack any more."))?;
        let children = granted_children(el, elements);
        let hub = self.hub()?;
        let codec = CatalogBlockCodec;
        let cx = Self::hb_context(&hub, &codec);
        let form = element_group_to_form(el, &children, &cx);
        let base: Vec<&Element> = std::iter::once(el)
            .chain(children.iter().copied())
            .collect();
        let draft = DraftFile {
            format: HB_DRAFT_FORMAT.into(),
            draft_id: new_draft_id(),
            pack_id: pack_id.to_string(),
            element_id: Some(element_id.to_string()),
            kind: form.kind.clone(),
            form,
            base_ids: base.iter().map(|e| e.id.clone()).collect(),
            base_digest: digest(&base),
            updated_at: now(),
            copied_from: None,
        };
        self.hb_drafts.hold(draft.clone())?;
        self.hb_view(&draft, &pack)
    }

    /// Every edit (spec 8.1, 10.1): the UI's form merged into the stored one by key (ids and kept bytes come from
    /// the stored copy; read-only fields keep their stored value), block sentences rewritten, the draft file
    /// written (atomic).
    ///
    /// # Errors
    /// `not_found` for an unknown draft; `invalid_argument` for a form of another kind; `io_error` when the draft
    /// can't be written (the previous version stays).
    pub fn homebrew_update_draft(
        &self,
        draft_id: &str,
        ui: &FormState,
    ) -> ApiResult<HomebrewDraftView> {
        let stored = self.hb_drafts.get(&self.paths.data, draft_id)?;
        if ui.kind != stored.form.kind {
            return Err(ApiError::invalid(
                "A draft keeps its form; start a new element for another kind.",
            ));
        }
        let pack = self.hb_pack(&stored.pack_id)?;
        let hub = self.hub()?;
        let codec = CatalogBlockCodec;
        let cx = Self::hb_context(&hub, &codec);
        let mut merged = merge_ui(&stored.form, ui);
        keep_read_only(&stored.form, &mut merged);
        let draft = DraftFile {
            form: refresh_blocks(&merged, &cx),
            updated_at: now(),
            ..stored
        };
        self.hb_drafts.put(&self.paths.data, draft.clone())?;
        self.hb_view(&draft, &pack)
    }

    /// Save (spec 10.1): ids for new elements (frozen from now on), the form's checks and the content validator
    /// (errors stop the save with the problems at their form paths, `invalid_argument` with `details` = the
    /// problems), the pack written and reloaded (only this pack), the draft deleted. Children the draft started
    /// with and no longer has are deleted and retired.
    ///
    /// # Errors
    /// As above; `not_found` for an unknown draft.
    pub fn homebrew_save_draft(
        &self,
        draft_id: &str,
        sink: &dyn EventSink,
    ) -> ApiResult<HomebrewSaveResult> {
        let _guard = self.content_lock()?;
        let draft = self.hb_drafts.get(&self.paths.data, draft_id)?;
        let pack = self.hb_pack(&draft.pack_id)?;
        let hub = self.hub()?;
        let codec = CatalogBlockCodec;
        let cx = Self::hb_context(&hub, &codec);
        let refuse = |problems: Vec<HomebrewProblem>| {
            let mut e = ApiError::invalid("Fix the problems marked in red before saving.");
            e.details = serde_json::to_value(&problems).ok();
            e
        };
        let own = form_problems(&draft.form, &cx, draft.element_id.as_deref());
        if !own.is_empty() {
            return Err(refuse(own));
        }
        let mut ids = self.hb_ids(&hub, &pack)?;
        let form = assign_ids(&draft.form, &mut ids);
        let elements = borealis_authoring::form::form_to_elements(&form, &cx).map_err(|ps| {
            refuse(
                ps.into_iter()
                    .map(|p| problem(&p.path, p.message, FORM_CODE, draft.element_id.as_deref()))
                    .collect(),
            )
        })?;
        let saved: Vec<String> = elements.iter().map(|e| e.id.clone()).collect();
        let dropped: HashSet<&str> = draft
            .base_ids
            .iter()
            .map(String::as_str)
            .filter(|id| !saved.iter().any(|s| s == id))
            .collect();
        let (content, trashed) = without_elements(&pack.disk.content, &dropped);
        let at = now();
        let mut creator = pack.creator.with_retired(dropped.iter().copied());
        for (f, e) in form.flatten().into_iter().zip(&elements) {
            let copied = (f.key == form.key)
                .then(|| draft.copied_from.clone())
                .flatten();
            let non_srd_text =
                copied.as_ref().is_some_and(|c| !c.srd) && !e.description.trim().is_empty();
            creator = creator.with_saved(
                &e.id,
                CreatorElement {
                    kind: f.kind.clone(),
                    created_at: at.clone(),
                    copied_from: copied,
                    non_srd_text,
                },
            );
        }
        let mut manifest = pack.disk.manifest.clone();
        manifest.version = bump_patch(&manifest.version);
        let next = PackWrite {
            manifest,
            content: with_elements(&content, &elements),
            creator,
            trashed,
        };
        match self.hb_commit(&pack, &next, &saved, sink) {
            Ok(()) => {}
            Err(CommitError::Api(e)) => return Err(e),
            Err(CommitError::Problems(issues)) => {
                return Err(refuse(issue_problems(&issues, &form, &elements, &cx)))
            }
        }
        self.hb_drafts.remove(&self.paths.data, draft_id)?;
        Ok(HomebrewSaveResult {
            pack: self.hb_pack_dto(&draft.pack_id)?,
            element_ids: saved,
        })
    }

    /// Drop a draft (its file too).
    ///
    /// # Errors
    /// `not_found` for an unknown draft.
    pub fn homebrew_discard_draft(&self, draft_id: &str) -> ApiResult<()> {
        self.hb_drafts.get(&self.paths.data, draft_id)?;
        self.hb_drafts.remove(&self.paths.data, draft_id)
    }

    /// The edited drafts of this device, newest first ("Unsaved changes from <time>").
    ///
    /// # Errors
    /// `internal` when a lock is poisoned.
    pub fn homebrew_list_drafts(&self) -> ApiResult<Vec<HomebrewDraftSummary>> {
        Ok(self
            .hb_drafts
            .list(&self.paths.data)?
            .into_iter()
            .map(|d| HomebrewDraftSummary {
                name: d
                    .form
                    .fields
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                draft_id: d.draft_id,
                pack_id: d.pack_id,
                element_id: d.element_id,
                kind: d.kind,
                updated_at: d.updated_at,
            })
            .collect())
    }

    /// Duplicate (spec 12.3): a new element in the same pack with the element's fields, rules and description,
    /// "<Name> (copy)", and a new id on its first save. Its features are not copied: the copy grants the same ones
    /// (a deep copy is WB-22).
    ///
    /// # Errors
    /// As `hb_pack`; `not_found` for an element not in the pack.
    pub fn homebrew_duplicate_element(
        &self,
        pack_id: &str,
        element_id: &str,
    ) -> ApiResult<HomebrewDraftView> {
        let pack = self.hb_pack(pack_id)?;
        let el = pack
            .disk
            .content
            .elements
            .iter()
            .find(|e| e.id == element_id && e.ty != SOURCE_TYPE)
            .ok_or_else(|| ApiError::not_found("That element isn't in this pack any more."))?;
        let hub = self.hub()?;
        let codec = CatalogBlockCodec;
        let cx = Self::hb_context(&hub, &codec);
        let draft = Self::hb_new_draft_file(&pack, as_copy(&element_to_form(el, &cx)), None);
        self.hb_drafts.hold(draft.clone())?;
        self.hb_view(&draft, &pack)
    }
}

/// The element grants loaded elements of its child types (a class's features): a deep copy would copy them.
fn has_loaded_children(el: &Element, hub: &ContentHub) -> bool {
    let types = child_types(&el.ty);
    el.rules.iter().any(|r| match r {
        borealis_content::Rule::Grant(g) => hub
            .store
            .element(&g.id)
            .is_some_and(|c| types.contains(&c.ty.as_str())),
        _ => false,
    })
}

/// Read-only fields (the Source label) keep the stored value whatever the UI sends.
fn keep_read_only(stored: &FormState, merged: &mut FormState) {
    if let Some(k) = kind(&stored.kind) {
        for f in k.fields.iter().filter(|f| f.read_only) {
            match stored.fields.get(f.key) {
                Some(v) => {
                    merged.fields.insert(f.key.to_string(), v.clone());
                }
                None => {
                    merged.fields.remove(f.key);
                }
            }
        }
    }
    for (child, old) in merged.children.iter_mut().filter_map(|c| {
        let key = c.key.clone();
        stored
            .children
            .iter()
            .find(|s| s.key == key)
            .map(|s| (c, s))
    }) {
        keep_read_only(old, child);
    }
}

/// The validator's errors on the saved elements, at their form paths.
fn issue_problems(
    issues: &[Issue],
    form: &FormState,
    elements: &[Element],
    cx: &FormContext<'_>,
) -> Vec<HomebrewProblem> {
    issues
        .iter()
        .map(|i| {
            let id = i.element.as_deref().unwrap_or_default();
            let path =
                form_path_for_issue(form, elements, id, i.path.as_deref(), cx).unwrap_or_default();
            HomebrewProblem {
                severity: "error".into(),
                message: i.message.clone(),
                path: (!path.is_empty()).then_some(path),
                element_id: i.element.clone(),
                code: serde_json::to_value(i.code)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default(),
            }
        })
        .collect()
}
