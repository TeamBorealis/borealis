//! The creator's expert commands (spec 12.3-12.5): where used, SPELL LISTS, and multi-select edits (edition, tags,
//! move to another homebrew pack, duplicate). Every change goes through the shared pieces: the element forms
//! (`borealis_authoring::form::bulk_patched`, `with_spell_list`), WB-22's copier for duplicates, the one reference
//! scanner (`borealis_authoring::pack::use_of`, the delete scan's), and the pack commit with its fast reload
//! ([`AppState::hb_commit_all`]). Each multi-select edit is one undoable step ([`crate::homebrew_undo`]).

use crate::content_packs::EventSink;
use crate::error::{ApiError, ApiResult};
use crate::homebrew_dto::{
    HomebrewBulkPatch, HomebrewBulkResult, HomebrewElementRef, HomebrewProblem,
    HomebrewSpellListRow, HomebrewSpellListView,
};
use crate::homebrew_files::PackWrite;
use crate::homebrew_packs::{CommitError, CreatorPack};
use crate::homebrew_refs::{is_bundled, picker_row, plain_list_tag};
use crate::homebrew_undo::PackBefore;
use crate::state::AppState;
use borealis_authoring::form::{
    assign_ids, bulk_patched, copy_element, write_copy, BulkChange, CatalogBlockCodec, Copy,
    FormContext, FormProblem, WithElements,
};
use borealis_authoring::pack::{
    bump_patch, granted_children, on_list, parent_of, removable, source_name, use_of,
    with_elements, with_spell_list, without_elements, CreatorElement, SOURCE_TYPE,
};
use borealis_content::{ContentFile, Element, Issue};
use std::collections::HashSet;
use std::time::SystemTime;

/// A refused change: the sentence, and the problems (each at its element) as `details`, as a refused save sends them.
fn refuse(message: &str, problems: Vec<HomebrewProblem>) -> ApiError {
    let mut e = ApiError::invalid(message);
    e.details = serde_json::to_value(&problems).ok();
    e
}

fn form_problem(p: FormProblem, el: &Element) -> HomebrewProblem {
    HomebrewProblem {
        severity: "error".into(),
        message: p.message,
        path: (!p.path.is_empty()).then_some(p.path),
        element_id: Some(el.id.clone()),
        code: "form".into(),
    }
}

fn issue_problem(i: &Issue) -> HomebrewProblem {
    HomebrewProblem {
        severity: "error".into(),
        message: i.message.clone(),
        path: None,
        element_id: i.element.clone(),
        code: "content".into(),
    }
}

/// The error of a commit: content errors are a refusal with their problems.
fn commit_error(e: CommitError) -> ApiError {
    match e {
        CommitError::Api(e) => e,
        CommitError::Problems(issues) => refuse(
            "The change would leave problems marked in red, so nothing was changed.",
            issues.iter().map(issue_problem).collect(),
        ),
    }
}

fn now() -> String {
    crate::ids::rfc3339(SystemTime::now())
}

fn pack_of(file: &str) -> &str {
    file.split('/').next().unwrap_or_default()
}

/// The element of a pack (never its book).
fn element_of<'p>(pack: &'p CreatorPack, id: &str) -> ApiResult<&'p Element> {
    pack.disk
        .content
        .elements
        .iter()
        .find(|e| e.id == id && e.ty != SOURCE_TYPE)
        .ok_or_else(|| ApiError::not_found("That element isn't in this pack any more."))
}

fn next_write(pack: &CreatorPack, content: ContentFile) -> PackWrite {
    let mut manifest = pack.disk.manifest.clone();
    manifest.version = bump_patch(&manifest.version);
    PackWrite {
        manifest,
        content,
        creator: pack.creator.clone(),
        trashed: Vec::new(),
    }
}

impl AppState {
    /// Where used (spec 12.3): the loaded elements (own packs and every other) that grant, offer, require or list
    /// the element, strongest use first, then by name.
    ///
    /// # Errors
    /// `not_found` for an element that is not loaded.
    pub fn homebrew_where_used(&self, element_id: &str) -> ApiResult<Vec<HomebrewElementRef>> {
        let hub = self.hub()?;
        let target = hub
            .store
            .element(element_id)
            .ok_or_else(|| ApiError::not_found("That element isn't loaded."))?;
        let mut found: Vec<(borealis_authoring::pack::UseRelation, HomebrewElementRef)> = hub
            .store
            .iter()
            .filter_map(|s| {
                let relation = use_of(&s.element, target)?;
                let pack = pack_of(&s.file);
                Some((
                    relation,
                    HomebrewElementRef {
                        id: s.element.id.clone(),
                        name: s.element.name.clone(),
                        ty: s.element.ty.clone(),
                        pack_id: (!is_bundled(&s.file)).then(|| pack.to_string()),
                        relation: relation.as_str().to_string(),
                    },
                ))
            })
            .collect();
        found.sort_by(|(ra, a), (rb, b)| {
            ra.cmp(rb)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(found.into_iter().map(|(_, r)| r).collect())
    }

    /// The SPELL LISTS grid of a pack for one list (spec 12.4): every loaded spell and the pack's own, whether it is
    /// on the list and whether the pack can take it off. The list is the pack's own when one of its elements declares
    /// it (else additions are appends).
    fn hb_spell_list_view(
        &self,
        pack: &CreatorPack,
        tag: &str,
    ) -> ApiResult<HomebrewSpellListView> {
        let hub = self.hub()?;
        let content = &pack.disk.content;
        let tag = tag.trim();
        let own = content.elements.iter().any(|e| {
            e.spellcasting
                .as_ref()
                .and_then(|s| s.list.as_ref())
                .and_then(|l| plain_list_tag(&l.supports))
                .is_some_and(|t| {
                    on_list(
                        &Element {
                            supports: vec![t.to_string()],
                            ..Element::default()
                        },
                        tag,
                    )
                })
        });
        let mut seen: HashSet<&str> = HashSet::new();
        let spells = content
            .elements
            .iter()
            .filter(|e| e.ty == "Spell")
            .chain(hub.store.by_type("Spell").map(|s| &s.element))
            .filter(|e| seen.insert(e.id.as_str()));
        let mut rows: Vec<HomebrewSpellListRow> = spells
            .map(|el| HomebrewSpellListRow {
                row: picker_row(&hub.store, el, false),
                on_list: on_list(el, tag),
                removable: removable(content, el, tag),
            })
            .collect();
        rows.sort_by(|a, b| {
            a.row
                .name
                .to_lowercase()
                .cmp(&b.row.name.to_lowercase())
                .then_with(|| a.row.source_name.cmp(&b.row.source_name))
                .then_with(|| a.row.id.cmp(&b.row.id))
        });
        Ok(HomebrewSpellListView {
            list_tag: tag.to_string(),
            own,
            count: rows.iter().filter(|r| r.on_list).count(),
            rows,
        })
    }

    /// A spell list of the pack (spec 12.4).
    ///
    /// # Errors
    /// As `hb_pack`; `invalid_argument` for an empty list name.
    pub fn homebrew_spell_list(
        &self,
        pack_id: &str,
        list_tag: &str,
    ) -> ApiResult<HomebrewSpellListView> {
        if list_tag.trim().is_empty() {
            return Err(ApiError::invalid("Choose a spell list first."));
        }
        let pack = self.hb_pack(pack_id)?;
        self.hb_spell_list_view(&pack, list_tag)
    }

    /// Put spells on a list and take them off (spec 12.4, HC5): the pack's own spells change their tags, loaded
    /// spells get (or lose) the pack's append. One write, one reload.
    ///
    /// # Errors
    /// As `hb_pack`; `not_found` for an unknown spell; `invalid_argument` for a spell the pack can't take off, or
    /// one that is not a spell.
    pub fn homebrew_set_spell_list(
        &self,
        pack_id: &str,
        list_tag: &str,
        add: &[String],
        remove: &[String],
        sink: &dyn EventSink,
    ) -> ApiResult<HomebrewSpellListView> {
        let _guard = self.content_lock()?;
        let pack = self.hb_pack(pack_id)?;
        let hub = self.hub()?;
        let content = &pack.disk.content;
        let find = |id: &String| -> ApiResult<&Element> {
            let el = content
                .elements
                .iter()
                .find(|e| &e.id == id)
                .or_else(|| hub.store.element(id))
                .ok_or_else(|| ApiError::not_found("That spell isn't loaded any more."))?;
            if el.ty == "Spell" {
                Ok(el)
            } else {
                Err(ApiError::invalid(format!("{} is not a spell.", el.name)))
            }
        };
        let add: Vec<&Element> = add.iter().map(find).collect::<ApiResult<_>>()?;
        let remove: Vec<&Element> = remove.iter().map(find).collect::<ApiResult<_>>()?;
        let codec = CatalogBlockCodec;
        let cx = FormContext::from_store(&hub.store, &codec);
        let change = with_spell_list(content, list_tag, &add, &remove, &cx).map_err(|ps| {
            refuse(
                "Some spells can't be changed on this list.",
                ps.into_iter()
                    .map(|p| HomebrewProblem {
                        severity: "error".into(),
                        message: p.message,
                        path: None,
                        element_id: None,
                        code: "form".into(),
                    })
                    .collect(),
            )
        })?;
        if change.content == *content {
            return self.hb_spell_list_view(&pack, list_tag);
        }
        let next = next_write(&pack, change.content);
        drop(hub);
        self.hb_commit(&pack, &next, &change.edited, sink)
            .map_err(commit_error)?;
        self.hb_spell_list_view(&self.hb_pack(pack_id)?, list_tag)
    }

    /// Multi-select edits (spec 12.5): set the edition, add or remove tags, move to another homebrew pack, or
    /// duplicate, for every chosen element, as one write and one undoable step.
    ///
    /// # Errors
    /// As `hb_pack`; `not_found` for an element not in the pack; `invalid_argument` with the problems as `details`
    /// when an element can't take the change (a class keeps its edition, R4), nothing is changed then.
    pub fn homebrew_bulk_edit(
        &self,
        pack_id: &str,
        element_ids: &[String],
        patch: &HomebrewBulkPatch,
        sink: &dyn EventSink,
    ) -> ApiResult<HomebrewBulkResult> {
        let _guard = self.content_lock()?;
        let pack = self.hb_pack(pack_id)?;
        if element_ids.is_empty() {
            return Err(ApiError::invalid("Choose the elements to change first."));
        }
        let chosen: Vec<&Element> = element_ids
            .iter()
            .map(|id| element_of(&pack, id))
            .collect::<ApiResult<_>>()?;
        let change = BulkChange {
            edition: patch.edition,
            add_tags: patch.add_tags.clone().unwrap_or_default(),
            remove_tags: patch.remove_tags.clone().unwrap_or_default(),
        };
        let moving = patch.move_to_pack_id.as_deref();
        let duplicating = patch.duplicate == Some(true);
        let asked = usize::from(!change.is_empty())
            + usize::from(moving.is_some())
            + usize::from(duplicating);
        if asked != 1 {
            return Err(ApiError::invalid(
                "Choose one change: the edition and tags, a move, or a duplicate.",
            ));
        }
        if let Some(target) = moving {
            return self.hb_bulk_move(&pack, &chosen, target, sink);
        }
        if duplicating {
            return self.hb_bulk_duplicate(&pack, &chosen, sink);
        }
        self.hb_bulk_fields(&pack, &chosen, &change, sink)
    }

    fn hb_bulk_fields(
        &self,
        pack: &CreatorPack,
        chosen: &[&Element],
        change: &BulkChange,
        sink: &dyn EventSink,
    ) -> ApiResult<HomebrewBulkResult> {
        let hub = self.hub()?;
        let codec = CatalogBlockCodec;
        let lookup = WithElements {
            base: &*hub.store,
            extra: &pack.disk.content.elements,
        };
        let cx = FormContext {
            lookup: &lookup,
            ..FormContext::from_store(&hub.store, &codec)
        };
        let mut changed = Vec::new();
        let mut problems = Vec::new();
        for el in chosen {
            match bulk_patched(el, change, &cx) {
                Ok(Some(e)) => changed.push(e),
                Ok(None) => {}
                Err(ps) => problems.extend(ps.into_iter().map(|p| form_problem(p, el))),
            }
        }
        if !problems.is_empty() {
            return Err(refuse(
                "Some elements can't take that change, so nothing was changed.",
                problems,
            ));
        }
        if changed.is_empty() {
            return Ok(HomebrewBulkResult {
                pack: self.hb_pack_dto(pack.id())?,
                element_ids: Vec::new(),
                undo_id: None,
            });
        }
        let ids: Vec<String> = changed.iter().map(|e| e.id.clone()).collect();
        let next = next_write(pack, with_elements(&pack.disk.content, &changed));
        drop(hub);
        self.hb_finish(vec![(pack, next)], ids, sink)
    }

    /// Write the packs of a multi-select edit, keep its undo, answer with the first pack.
    fn hb_finish(
        &self,
        writes: Vec<(&CreatorPack, PackWrite)>,
        ids: Vec<String>,
        sink: &dyn EventSink,
    ) -> ApiResult<HomebrewBulkResult> {
        let before: Vec<PackBefore> = writes.iter().map(|(p, n)| PackBefore::of(p, n)).collect();
        let pairs: Vec<(&CreatorPack, &PackWrite)> = writes.iter().map(|(p, n)| (*p, n)).collect();
        self.hb_commit_all(&pairs, &ids, sink)
            .map_err(commit_error)?;
        let first = writes
            .first()
            .map(|(p, _)| p.id().to_string())
            .ok_or_else(|| ApiError::internal("an empty change"))?;
        Ok(HomebrewBulkResult {
            pack: self.hb_pack_dto(&first)?,
            element_ids: ids,
            undo_id: Some(self.hb_undo.record(before)?),
        })
    }

    /// Move to another homebrew pack: the elements and their children keep their ids (characters keep them), name
    /// the other pack's book, keep their edition (an element that followed its pack's edition gets it as its own when
    /// the packs differ) and take their `creator.json` entries along (provenance and CS1 marks); the old pack retires
    /// the ids.
    fn hb_bulk_move(
        &self,
        pack: &CreatorPack,
        chosen: &[&Element],
        target_id: &str,
        sink: &dyn EventSink,
    ) -> ApiResult<HomebrewBulkResult> {
        if target_id == pack.id() {
            return Err(ApiError::invalid(
                "Those elements are in that pack already.",
            ));
        }
        let target = self.hb_pack(target_id)?;
        let elements = &pack.disk.content.elements;
        let mut moving: Vec<&Element> = Vec::new();
        for el in chosen {
            for e in std::iter::once(*el).chain(granted_children(el, elements)) {
                if !moving.iter().any(|m| m.id == e.id) {
                    moving.push(e);
                }
            }
        }
        let ids: Vec<String> = moving.iter().map(|e| e.id.clone()).collect();
        if let Some(d) = self
            .hb_drafts
            .list(&self.paths.data)?
            .into_iter()
            .find(|d| {
                d.pack_id == pack.id() && d.element_id.as_ref().is_some_and(|id| ids.contains(id))
            })
        {
            let name = d
                .element_id
                .and_then(|id| elements.iter().find(|e| e.id == id))
                .map_or("an element", |e| e.name.as_str())
                .to_string();
            return Err(ApiError::invalid(format!(
                "Save or discard the unsaved changes of {name} before moving it."
            )));
        }
        let book = source_name(&target.disk.manifest, &target.disk.content);
        let (from_ed, to_ed) = (pack.disk.content.ruleset, target.disk.content.ruleset);
        let moved: Vec<Element> = moving
            .iter()
            .map(|e| {
                let mut m = (*e).clone();
                m.source.clone_from(&book);
                if m.ruleset.is_none() && from_ed != to_ed {
                    m.ruleset = from_ed;
                }
                m
            })
            .collect();
        let gone: HashSet<&str> = ids.iter().map(String::as_str).collect();
        let (left, _) = without_elements(&pack.disk.content, &gone);
        let mut from_next = next_write(pack, left);
        from_next.creator = pack.creator.with_retired(gone.iter().copied());
        let mut to_next = next_write(&target, with_elements(&target.disk.content, &moved));
        for id in &ids {
            if let Some(entry) = pack.creator.elements.get(id) {
                to_next.creator = to_next.creator.with_saved(id, entry.clone());
            }
        }
        self.hb_finish(vec![(pack, from_next), (&target, to_next)], ids, sink)
    }

    /// Duplicate (spec 12.3 for several): each chosen element copied by WB-22's copier into the same pack ("<Name>
    /// (copy)", new ids, a class with its features and subclasses), a child whose parent is chosen too copied with
    /// it. Copies of copied text keep their provenance and CS1 mark.
    fn hb_bulk_duplicate(
        &self,
        pack: &CreatorPack,
        chosen: &[&Element],
        sink: &dyn EventSink,
    ) -> ApiResult<HomebrewBulkResult> {
        let hub = self.hub()?;
        let codec = CatalogBlockCodec;
        let elements = &pack.disk.content.elements;
        let lookup = WithElements {
            base: &*hub.store,
            extra: elements,
        };
        let cx = FormContext {
            lookup: &lookup,
            ..FormContext::from_store(&hub.store, &codec)
        };
        let picked: HashSet<&str> = chosen.iter().map(|e| e.id.as_str()).collect();
        let from = |id: &str| {
            pack.creator
                .elements
                .get(id)
                .and_then(|c| c.copied_from.clone())
        };
        let mut gen = self.hb_ids(&hub, pack)?;
        let mut creator = pack.creator.clone();
        let mut made: Vec<Element> = Vec::new();
        let mut problems = Vec::new();
        let at = now();
        for el in chosen
            .iter()
            .filter(|e| !parent_of(e, elements).is_some_and(|p| picked.contains(p.id.as_str())))
        {
            let Copy { form, mut extras } = copy_element(el, &cx, el.ty == "Class");
            for l in &mut extras.links {
                l.from = from(&l.original);
            }
            let form = assign_ids(&form, &mut gen);
            let written = match write_copy(&form, &extras, &cx, &mut gen) {
                Ok(w) => w,
                Err(ps) => {
                    problems.extend(ps.into_iter().map(|p| form_problem(p, el)));
                    continue;
                }
            };
            for w in written {
                let copied = match extras.link(&w.key) {
                    Some(l) => l.from.clone(),
                    None => (w.key == form.key).then(|| from(&el.id)).flatten(),
                };
                let non_srd_text = copied.as_ref().is_some_and(|c| !c.srd)
                    && !w.element.description.trim().is_empty();
                creator = creator.with_saved(
                    &w.element.id,
                    CreatorElement {
                        kind: w.kind,
                        created_at: at.clone(),
                        copied_from: copied,
                        non_srd_text,
                    },
                );
                made.push(w.element);
            }
        }
        if !problems.is_empty() {
            return Err(refuse(
                "Some elements can't be copied, so nothing was copied.",
                problems,
            ));
        }
        let ids: Vec<String> = made.iter().map(|e| e.id.clone()).collect();
        let mut next = next_write(pack, with_elements(&pack.disk.content, &made));
        next.creator = creator;
        drop(hub);
        self.hb_finish(vec![(pack, next)], ids, sink)
    }
}
