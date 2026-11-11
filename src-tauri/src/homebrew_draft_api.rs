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
use borealis_authoring::check::{issue_problems, Problem};
use borealis_authoring::form::{
    add_child, assign_ids, copy_child, copy_element, data_json, data_tree, element_group_to_form,
    kind, merge_ui, new_background_form, new_child_key, new_class_form, new_creature_form,
    refresh_blocks, to_ui, write_copy, CatalogBlockCodec, Copy, CopyExtras, FormContext, FormState,
    WithElements, BACKGROUND_KIND, CLASS_KIND, CREATURE_KIND, SUBCLASS_KIND,
};
use borealis_authoring::pack::{
    bump_patch, digest, granted_children, pack_edition, source_name, with_elements,
    without_elements, CopiedFrom, CreatorElement, SOURCE_TYPE,
};
use borealis_content::{Element, Ruleset};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::SystemTime;

fn now() -> String {
    rfc3339(SystemTime::now())
}

/// A problem as the UI gets it. `saved` are the draft's saved element ids: a problem of an element that has none yet
/// (its id is only provisional) belongs to the draft, not to an element.
pub(crate) fn wire_problem(p: Problem, saved: &HashSet<&str>) -> HomebrewProblem {
    let severity = serde_json::to_value(p.severity)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "error".into());
    HomebrewProblem {
        severity,
        message: p.message,
        path: p.path,
        element_id: p.element_id.filter(|id| saved.contains(id.as_str())),
        code: p.code,
    }
}

/// The saved element ids of a draft (the ones it was opened with).
pub(crate) fn saved_ids(draft: &DraftFile) -> HashSet<&str> {
    draft
        .base_ids
        .iter()
        .map(String::as_str)
        .chain(draft.element_id.as_deref())
        .collect()
}

/// Where the loaded element `id` came from: its pack, and whether that is bundled SRD content (spec 11, CS1).
fn origin(hub: &ContentHub, id: &str) -> Option<CopiedFrom> {
    let stored = hub.store.get(id)?;
    let pack_id = stored
        .file
        .split('/')
        .next()
        .unwrap_or_default()
        .to_string();
    Some(CopiedFrom {
        id: id.to_string(),
        srd: BUNDLED_PACKS.contains(&pack_id.as_str()),
        pack_id,
    })
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
        let saved = saved_ids(draft);
        let problems = Self::hb_live(&hub, pack, draft, &provisional, &cx)
            .into_iter()
            .map(|p| wire_problem(p, &saved))
            .collect();
        let elements = borealis_authoring::form::form_to_elements(&provisional, &cx).ok();
        let preview = elements
            .as_ref()
            .and_then(|els| els.first())
            .map(|e| element_detail(&hub.store, e));
        let (json, tree) = elements.as_ref().map_or_else(
            || (String::new(), Vec::new()),
            |els| (data_json(els), data_tree(&provisional, els, &cx)),
        );
        Ok(HomebrewDraftView {
            draft_id: draft.draft_id.clone(),
            pack_id: draft.pack_id.clone(),
            element_id: draft.element_id.clone(),
            form: to_ui(&draft.form),
            preview,
            problems,
            generated: HomebrewGeneratedView { json, tree },
            updated_at: draft.updated_at.clone(),
            base_changed: Self::hb_base_changed(pack, draft),
        })
    }

    fn hb_new_draft_file(
        pack: &CreatorPack,
        form: FormState,
        copied_from: Option<CopiedFrom>,
        copy: CopyExtras,
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
            copy,
        }
    }

    /// A new element (spec 10.1): blank, or a copy of any loaded element (spec 11; the copy keeps the rules, tags,
    /// description and edition, gets "<Name> (copy)" and a new id on its first save; where it and every element copied
    /// with it came from goes to `creator.json`). Nothing is written until the first edit.
    ///
    /// # Errors
    /// As `hb_pack`; `invalid_argument` for an unknown kind or a copy of a book element; `not_found` for an unknown
    /// copy. With `parent_id` (a draft of the pack, or a saved element of it) it makes a child:
    /// [`Self::hb_new_child`]. With `copy_from`, `kind_id` is not read (the copy opens in its element's form) and
    /// `deep_copy` copies its features, traits and subclasses too ([`copy_element`]).
    pub fn homebrew_new_draft(
        &self,
        pack_id: &str,
        kind_id: &str,
        copy_from: Option<&str>,
        parent_id: Option<&str>,
        deep_copy: Option<bool>,
    ) -> ApiResult<HomebrewDraftView> {
        if let Some(parent) = parent_id {
            return self.hb_new_child(pack_id, kind_id, parent, copy_from);
        }
        let pack = self.hb_pack(pack_id)?;
        let hub = self.hub()?;
        let source = source_name(&pack.disk.manifest, &pack.disk.content);
        let (form, copied_from, extras) = match copy_from {
            Some(id) => {
                let el = &hub
                    .store
                    .get(id)
                    .ok_or_else(|| ApiError::not_found("That element isn't loaded any more."))?
                    .element;
                if el.ty == SOURCE_TYPE {
                    return Err(ApiError::invalid("A book element can't be copied."));
                }
                let codec = CatalogBlockCodec;
                let cx = Self::hb_context(&hub, &codec);
                let Copy {
                    mut form,
                    mut extras,
                } = copy_element(el, &cx, deep_copy == Some(true));
                with_source(&mut form, &source);
                for c in &mut extras.companions {
                    with_source(c, &source);
                }
                for l in &mut extras.links {
                    l.from = origin(&hub, &l.original);
                }
                (form, origin(&hub, &el.id), extras)
            }
            None => {
                let k = kind(kind_id).ok_or_else(|| {
                    ApiError::invalid(format!("There is no form called {kind_id:?}."))
                })?;
                // a class, a background and a creature are of one edition (R4; the form shows that edition's
                // fields, a creature's stat block follows it), a pack of both editions makes 2024 ones
                let edition = match pack_edition(&pack.disk.manifest, &pack.disk.content) {
                    Ruleset::Dnd2014 => Ruleset::Dnd2014,
                    _ => Ruleset::Dnd2024,
                };
                let mut form = match k.id {
                    // a class starts with its generated features (spec 4.5)
                    CLASS_KIND => new_class_form(edition),
                    BACKGROUND_KIND => new_background_form(edition),
                    CREATURE_KIND => new_creature_form(edition),
                    _ => k.new_form("e"),
                };
                with_source(&mut form, &source);
                (form, None, CopyExtras::default())
            }
        };
        let draft = Self::hb_new_draft_file(&pack, form, copied_from, extras);
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
            copy: CopyExtras::default(),
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
        // a subclass's class can be a class of the pack not saved yet (spec 4.7: own classes too)
        let unsaved: Vec<Element> = if stored.form.kind == SUBCLASS_KIND {
            self.hb_unsaved_elements(draft_id)?
                .into_iter()
                .filter(|e| e.ty == "Class" || e.ty == "Class Feature")
                .collect()
        } else {
            Vec::new()
        };
        let lookup = WithElements {
            base: &*hub.store,
            extra: &unsaved,
        };
        let cx = FormContext {
            lookup: &lookup,
            ..Self::hb_context(&hub, &codec)
        };
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
        let saved_before = saved_ids(&draft);
        let mut ids = self.hb_ids(&hub, &pack)?;
        let form = assign_ids(&draft.form, &mut ids);
        let errors: Vec<HomebrewProblem> = Self::hb_live(&hub, &pack, &draft, &form, &cx)
            .into_iter()
            .filter(Problem::is_error)
            .map(|p| wire_problem(p, &saved_before))
            .collect();
        if !errors.is_empty() {
            return Err(refuse(errors));
        }
        // the form's elements and, for a copy, the elements saved with it, every copied original re-pointed (spec 11)
        let written = write_copy(&form, &draft.copy, &cx, &mut ids).map_err(|ps| {
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
        let elements: Vec<Element> = written.iter().map(|w| w.element.clone()).collect();
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
        for w in &written {
            let copied = match draft.copy.link(&w.key) {
                Some(l) => l.from.clone(),
                None => (w.key == form.key)
                    .then(|| draft.copied_from.clone())
                    .flatten(),
            };
            let non_srd_text =
                copied.as_ref().is_some_and(|c| !c.srd) && !w.element.description.trim().is_empty();
            creator = creator.with_saved(
                &w.element.id,
                CreatorElement {
                    kind: w.kind.clone(),
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
                let own = form.flatten().len().min(elements.len());
                return Err(refuse(
                    issue_problems(&issues, &form, &elements[..own], &cx)
                        .into_iter()
                        .map(|p| wire_problem(p, &saved_before))
                        .collect(),
                ));
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
    /// "<Name> (copy)", and a new id on its first save. A class is copied deep (spec 11: its features, package and
    /// subclasses); other elements grant the same children.
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
        // the pack as it is on disk first (a turned-off pack is not loaded), then the loaded content
        let lookup = WithElements {
            base: &*hub.store,
            extra: &pack.disk.content.elements,
        };
        let cx = FormContext {
            lookup: &lookup,
            ..Self::hb_context(&hub, &codec)
        };
        let Copy { form, mut extras } = copy_element(el, &cx, el.ty == "Class");
        // a duplicate of text copied from elsewhere is that text too (CS1)
        let from = |id: &str| {
            pack.creator
                .elements
                .get(id)
                .and_then(|c| c.copied_from.clone())
        };
        for l in &mut extras.links {
            l.from = from(&l.original);
        }
        let draft = Self::hb_new_draft_file(&pack, form, from(&el.id), extras);
        self.hb_drafts.hold(draft.clone())?;
        self.hb_view(&draft, &pack)
    }
}

/// The form and its children name the pack's book as their source.
fn with_source(form: &mut FormState, source: &str) {
    if kind(&form.kind).is_some_and(|k| k.field("source").is_some()) {
        form.fields.insert("source".into(), json!(source));
    }
    for c in &mut form.children {
        with_source(c, source);
    }
}

impl AppState {
    /// A new child (spec 3.1, `homebrew_new_draft {parentId}`): a new element of `kind_id` (or a copy of a loaded
    /// one) inside its parent, given by the parent's field for it (a class feature on the LEVELS grid at level 1,
    /// the class's starting-equipment package). The parent is a draft of the pack (`parent_id` = its draft id) or a
    /// saved element of it (its draft is opened or resumed). The parent's draft is written and its view returned;
    /// the child is its last child.
    ///
    /// # Errors
    /// As `hb_pack`; `not_found` for an unknown parent or copy; `invalid_argument` when the parent's form has no
    /// place for a child of that kind.
    fn hb_new_child(
        &self,
        pack_id: &str,
        kind_id: &str,
        parent_id: &str,
        copy_from: Option<&str>,
    ) -> ApiResult<HomebrewDraftView> {
        let pack = self.hb_pack(pack_id)?;
        let draft = match self.hb_drafts.get(&self.paths.data, parent_id) {
            Ok(d) if d.pack_id == pack_id => d,
            _ => {
                let view = self.homebrew_open_draft(pack_id, Some(parent_id), None)?;
                self.hb_drafts.get(&self.paths.data, &view.draft_id)?
            }
        };
        let k = kind(kind_id)
            .ok_or_else(|| ApiError::invalid(format!("There is no form called {kind_id:?}.")))?;
        let hub = self.hub()?;
        let key = new_child_key(&draft.form);
        let mut child = match copy_from {
            Some(id) => {
                let el = hub
                    .store
                    .element(id)
                    .ok_or_else(|| ApiError::not_found("That element isn't loaded any more."))?;
                if !k.element_types.contains(&el.ty.as_str()) {
                    return Err(ApiError::invalid(format!(
                        "A {} can't be copied into this form.",
                        el.ty.to_lowercase()
                    )));
                }
                let codec = CatalogBlockCodec;
                let cx = Self::hb_context(&hub, &codec);
                copy_child(el, &key, &cx)
            }
            None => k.new_form(key),
        };
        with_source(
            &mut child,
            &source_name(&pack.disk.manifest, &pack.disk.content),
        );
        let form = add_child(&draft.form, child, None).ok_or_else(|| {
            ApiError::invalid(format!(
                "A {} can't be made inside this element.",
                k.label.to_lowercase()
            ))
        })?;
        let next = DraftFile {
            form,
            updated_at: now(),
            ..draft
        };
        self.hb_drafts.put(&self.paths.data, next.clone())?;
        self.hb_view(&next, &pack)
    }
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
