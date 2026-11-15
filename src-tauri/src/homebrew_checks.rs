//! Validation of homebrew creator drafts (spec 8): the live pipeline of every autosave and save
//! (`borealis_authoring::check::live_problems`), and the engine checks of `homebrew_check_draft`
//! (`borealis_authoring::check::deep_problems`: a test character takes the element, the 1-20 level sweep, choices
//! that offer nothing, balance hints). The work is in `borealis-authoring`; this module gives it the loaded content,
//! the pack and the draft.
//!
//! The engine checks run on a blocking thread (`commands_content::blocking`) with nothing locked: the draft, the pack
//! and the content are read first, so autosaves go on while a check runs. The UI drops an answer for a form that has
//! changed since (`draftSession.ts`). Checks take turns, and one replaced by a newer check of its draft before its turn
//! is not run ([`crate::homebrew_check_gate`]).

use crate::error::ApiResult;
use crate::homebrew_draft_api::{draft_elements, saved_ids, wire_problem};
use crate::homebrew_drafts::DraftFile;
use crate::homebrew_dto::HomebrewProblem;
use crate::homebrew_packs::CreatorPack;
use crate::state::{AppState, ContentHub};
use borealis_authoring::check::{
    deep_problems, live_problems, DeepInput, LiveInput, PackElement, Problem,
};
use borealis_authoring::form::{
    assign_ids, edition_problems, CatalogBlockCodec, FormContext, FormState,
};
use borealis_authoring::pack::{pack_edition, without_elements, SOURCE_TYPE};
use borealis_content::{ParsedPack, Ruleset};
use std::collections::HashSet;

/// The name the draft's content is loaded under for the engine checks.
const DRAFT_FILE: &str = "homebrew-draft";

impl AppState {
    /// The live problems of `form` (the draft's form with the ids its elements have or would get now), with the pack's
    /// saved elements and the loaded content (spec 8.1 steps 1-5).
    pub(crate) fn hb_live(
        hub: &ContentHub,
        pack: &CreatorPack,
        draft: &DraftFile,
        form: &FormState,
        cx: &FormContext<'_>,
    ) -> Vec<Problem> {
        let now: HashSet<&str> = form
            .flatten()
            .into_iter()
            .filter_map(FormState::id)
            .collect();
        let hidden: HashSet<&str> = draft
            .base_ids
            .iter()
            .map(String::as_str)
            .filter(|id| !now.contains(id))
            .collect();
        let siblings: Vec<PackElement> = pack
            .disk
            .content
            .elements
            .iter()
            .filter(|e| e.ty != SOURCE_TYPE)
            .map(|e| PackElement {
                id: e.id.clone(),
                name: e.name.clone(),
                ty: e.ty.clone(),
            })
            .collect();
        let mut out = live_problems(&LiveInput {
            form,
            cx,
            store: &hub.store,
            file: &format!("{}/{}", pack.id(), pack.disk.content_path),
            hidden: &hidden,
            pack: &siblings,
        });
        // a class keeps its edition; other content of the other edition is adapted (spec 11)
        out.extend(edition_problems(
            form,
            pack_edition(&pack.disk.manifest, &pack.disk.content),
        ));
        out
    }

    /// The engine checks of a draft (spec 8.1 steps 6-7): a test character takes it, classes and subclasses are
    /// levelled 1 to 20, choices that offer nothing and balance hints. Never errors: they don't block Save.
    ///
    /// # Errors
    /// `not_found` for an unknown draft or pack; `requirement_failed` when a newer check of the draft replaced this
    /// one before its turn ([`crate::homebrew_check_gate::superseded`]); `internal` when a lock is poisoned.
    pub fn homebrew_check_draft(&self, draft_id: &str) -> ApiResult<Vec<HomebrewProblem>> {
        let ticket = self.hb_checks.request(draft_id)?;
        let _turn = self.hb_checks.turn(&ticket)?;
        let draft = self.hb_drafts.get(&self.paths.data, draft_id)?;
        let pack = self.hb_pack(&draft.pack_id)?;
        let hub = self.hub()?;
        let codec = CatalogBlockCodec;
        let cx = FormContext::from_store(&hub.store, &codec);
        let mut ids = self.hb_ids(&hub, &pack)?;
        let form = assign_ids(&draft.form, &mut ids);
        // the pack as it would be without the draft's saved version; the draft goes on top
        let base_ids: HashSet<&str> = draft.base_ids.iter().map(String::as_str).collect();
        let (rest, _) = without_elements(&pack.disk.content, &base_ids);
        let parsed = ParsedPack::from_parts(
            pack.disk.manifest.clone(),
            vec![(pack.disk.content_path.clone(), rest)],
        );
        let base = hub
            .builder_with_pack(parsed)
            .unwrap_or_else(|| hub.builder.as_ref().clone());
        let ruleset = match pack_edition(&pack.disk.manifest, &pack.disk.content) {
            Ruleset::Dnd2014 => Ruleset::Dnd2014,
            _ => Ruleset::Dnd2024,
        };
        // as a save writes it: an unsaved copy's grants name its own copied features (H8mmg)
        let written = draft_elements(&form, &draft.copy, &cx, &mut ids);
        let ruleset = written
            .as_ref()
            .and_then(|els| els.first().and_then(|e| e.ruleset))
            .filter(|r| *r != Ruleset::Any)
            .unwrap_or(ruleset);
        let saved = saved_ids(&draft);
        Ok(deep_problems(&DeepInput {
            base: &base,
            ruleset,
            pack_name: DRAFT_FILE,
            form: &form,
            written: written.as_deref(),
            cx: &cx,
            loaded: Some(&hub.store),
            loaded_engine: Some(&hub.engine),
        })
        .into_iter()
        .map(|p| wire_problem(p, &saved))
        .collect())
    }
}
