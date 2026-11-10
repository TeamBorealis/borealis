//! `#[tauri::command]` wrappers of the homebrew creator (1.5.0; `specs/homebrew-creator-spec.md` section 15.2,
//! `docs/ui-api-contract.md` "Homebrew creator"). Each delegates to [`crate::homebrew_creator`]; until its work item
//! lands it answers `not_implemented`. The UI reaches them only with `AppSettings::creator_enabled` on (HC3).

use crate::content_dto::ContentPack;
use crate::dto::{ExportResult, Page};
use crate::error::ApiResult;
use crate::homebrew_creator::not_implemented;
use crate::homebrew_dto::{
    HomebrewBlockSpec, HomebrewBulkPatch, HomebrewDeleteResult, HomebrewDraftSummary,
    HomebrewDraftView, HomebrewElementRef, HomebrewElementSummary, HomebrewForm,
    HomebrewKindDescriptor, HomebrewPackPatch, HomebrewPackRequest, HomebrewPageRequest,
    HomebrewPickerFilters, HomebrewPickerRow, HomebrewPickerTarget, HomebrewProblem,
    HomebrewSaveResult, HomebrewSpellListView, HomebrewStatGroup, HomebrewTrialChange,
    HomebrewTrialSetup, HomebrewTrialView,
};

/// Every creator form (spec 3-4).
#[tauri::command]
pub fn homebrew_kinds() -> ApiResult<Vec<HomebrewKindDescriptor>> {
    Err(not_implemented("homebrew_kinds"))
}

/// The rule block catalog (spec 6).
#[tauri::command]
pub fn homebrew_block_catalog() -> ApiResult<Vec<HomebrewBlockSpec>> {
    Err(not_implemented("homebrew_block_catalog"))
}

/// The statistic picker's groups (spec 5.2).
#[tauri::command]
pub fn homebrew_stat_catalog() -> ApiResult<Vec<HomebrewStatGroup>> {
    Err(not_implemented("homebrew_stat_catalog"))
}

/// NEW PACK (spec 14.1).
#[tauri::command]
pub fn homebrew_create_pack(request: HomebrewPackRequest) -> ApiResult<ContentPack> {
    let _ = request;
    Err(not_implemented("homebrew_create_pack"))
}

/// Pack settings (spec 13.10).
#[tauri::command]
pub fn homebrew_update_pack(pack_id: String, patch: HomebrewPackPatch) -> ApiResult<ContentPack> {
    let _ = (pack_id, patch);
    Err(not_implemented("homebrew_update_pack"))
}

/// The element list of a pack (spec 13.2).
#[tauri::command]
pub fn homebrew_list_elements(pack_id: String) -> ApiResult<Vec<HomebrewElementSummary>> {
    let _ = pack_id;
    Err(not_implemented("homebrew_list_elements"))
}

/// A new element, blank or from a copy (spec 10.1, 11).
#[tauri::command]
pub fn homebrew_new_draft(
    pack_id: String,
    kind: String,
    copy_from: Option<String>,
    parent_id: Option<String>,
    deep_copy: Option<bool>,
) -> ApiResult<HomebrewDraftView> {
    let _ = (pack_id, kind, copy_from, parent_id, deep_copy);
    Err(not_implemented("homebrew_new_draft"))
}

/// Open a saved element, resuming its draft (spec 10.1).
#[tauri::command]
pub fn homebrew_open_draft(pack_id: String, element_id: String) -> ApiResult<HomebrewDraftView> {
    let _ = (pack_id, element_id);
    Err(not_implemented("homebrew_open_draft"))
}

/// Every edit: autosave and live validation (spec 8.1, 10.1).
#[tauri::command]
pub fn homebrew_update_draft(draft_id: String, form: HomebrewForm) -> ApiResult<HomebrewDraftView> {
    let _ = (draft_id, form);
    Err(not_implemented("homebrew_update_draft"))
}

/// Engine checks and the level sweep (spec 8.1 step 6).
#[tauri::command]
pub fn homebrew_check_draft(draft_id: String) -> ApiResult<Vec<HomebrewProblem>> {
    let _ = draft_id;
    Err(not_implemented("homebrew_check_draft"))
}

/// Validate, write the pack, hot-reload (spec 10.1).
#[tauri::command]
pub fn homebrew_save_draft(draft_id: String) -> ApiResult<HomebrewSaveResult> {
    let _ = draft_id;
    Err(not_implemented("homebrew_save_draft"))
}

/// Drop a draft.
#[tauri::command]
pub fn homebrew_discard_draft(draft_id: String) -> ApiResult<()> {
    let _ = draft_id;
    Err(not_implemented("homebrew_discard_draft"))
}

/// Drafts on this device.
#[tauri::command]
pub fn homebrew_list_drafts() -> ApiResult<Vec<HomebrewDraftSummary>> {
    Err(not_implemented("homebrew_list_drafts"))
}

/// Delete scan, then delete (spec 10.3).
#[tauri::command]
pub fn homebrew_delete_elements(
    pack_id: String,
    element_ids: Vec<String>,
    confirm: bool,
) -> ApiResult<HomebrewDeleteResult> {
    let _ = (pack_id, element_ids, confirm);
    Err(not_implemented("homebrew_delete_elements"))
}

/// Duplicate inside the pack (spec 12.3).
#[tauri::command]
pub fn homebrew_duplicate_element(
    pack_id: String,
    element_id: String,
) -> ApiResult<HomebrewDraftView> {
    let _ = (pack_id, element_id);
    Err(not_implemented("homebrew_duplicate_element"))
}

/// Multi-select edits (spec 12.5).
#[tauri::command]
pub fn homebrew_bulk_edit(
    pack_id: String,
    element_ids: Vec<String>,
    patch: HomebrewBulkPatch,
) -> ApiResult<ContentPack> {
    let _ = (pack_id, element_ids, patch);
    Err(not_implemented("homebrew_bulk_edit"))
}

/// Picker rows (spec 5.1).
#[tauri::command]
pub fn homebrew_reference_options(
    target: HomebrewPickerTarget,
    query: String,
    filters: HomebrewPickerFilters,
    page: HomebrewPageRequest,
    draft_id: Option<String>,
) -> ApiResult<Page<HomebrewPickerRow>> {
    let _ = (target, query, filters, page, draft_id);
    Err(not_implemented("homebrew_reference_options"))
}

/// Where used (spec 12.3).
#[tauri::command]
pub fn homebrew_where_used(element_id: String) -> ApiResult<Vec<HomebrewElementRef>> {
    let _ = element_id;
    Err(not_implemented("homebrew_where_used"))
}

/// A spell list (spec 12.4).
#[tauri::command]
pub fn homebrew_spell_list(pack_id: String, list_tag: String) -> ApiResult<HomebrewSpellListView> {
    let _ = (pack_id, list_tag);
    Err(not_implemented("homebrew_spell_list"))
}

/// Change a spell list (spec 12.4).
#[tauri::command]
pub fn homebrew_set_spell_list(
    pack_id: String,
    list_tag: String,
    add: Vec<String>,
    remove: Vec<String>,
) -> ApiResult<HomebrewSpellListView> {
    let _ = (pack_id, list_tag, add, remove);
    Err(not_implemented("homebrew_set_spell_list"))
}

/// Try it (spec 9).
#[tauri::command]
pub fn homebrew_trial_start(
    draft_id: String,
    setup: HomebrewTrialSetup,
) -> ApiResult<HomebrewTrialView> {
    let _ = (draft_id, setup);
    Err(not_implemented("homebrew_trial_start"))
}

/// Change the trial character (spec 9).
#[tauri::command]
pub fn homebrew_trial_update(
    trial_id: String,
    change: HomebrewTrialChange,
) -> ApiResult<HomebrewTrialView> {
    let _ = (trial_id, change);
    Err(not_implemented("homebrew_trial_update"))
}

/// Drop the trial character (spec 9).
#[tauri::command]
pub fn homebrew_trial_end(trial_id: String) -> ApiResult<()> {
    let _ = trial_id;
    Err(not_implemented("homebrew_trial_end"))
}

/// Export a pack file (spec 14.2).
#[tauri::command]
pub fn homebrew_export_pack(
    pack_id: String,
    leave_out_copied_text: bool,
) -> ApiResult<ExportResult> {
    let _ = (pack_id, leave_out_copied_text);
    Err(not_implemented("homebrew_export_pack"))
}
