//! `#[tauri::command]` wrappers of the homebrew creator (1.5.0; `specs/homebrew-creator-spec.md` section 15.2,
//! `docs/ui-api-contract.md` "Homebrew creator"). Each delegates to an `AppState` method (`homebrew_packs`,
//! `homebrew_draft_api`, `homebrew_refs`, `homebrew_trial`, `homebrew_transfer`) or [`crate::homebrew_creator`]. Work that writes packs runs off the main thread (`commands_content::blocking`). The UI reaches
//! them only with `AppSettings::creator_enabled` on (HC3).

use crate::content_dto::ContentPack;
use crate::dto::{Page, SlotOption};
use crate::error::ApiResult;
use crate::homebrew_creator;
use crate::homebrew_dto::{
    HomebrewBlockSpec, HomebrewBulkPatch, HomebrewBulkResult, HomebrewDeleteResult,
    HomebrewDraftSummary, HomebrewDraftView, HomebrewElementRef, HomebrewElementSummary,
    HomebrewExportResult, HomebrewForm, HomebrewPackPatch, HomebrewPackRequest,
    HomebrewPageRequest, HomebrewPickerFilters, HomebrewPickerRow, HomebrewPickerTarget,
    HomebrewProblem, HomebrewSaveResult, HomebrewSpellListView, HomebrewStatGroup,
    HomebrewTrialChange, HomebrewTrialSetup, HomebrewTrialView,
};
use crate::state::AppState;
use tauri::State;

/// Every creator form (spec 3-4): `borealis_authoring::form::descriptors()`, the same JSON as
/// [`crate::homebrew_dto::HomebrewKindDescriptor`].
#[tauri::command]
pub fn homebrew_kinds() -> ApiResult<Vec<borealis_authoring::form::descriptor::KindDescriptor>> {
    Ok(borealis_authoring::form::descriptors())
}

/// The rule block catalog (spec 6).
#[tauri::command]
pub fn homebrew_block_catalog() -> ApiResult<Vec<HomebrewBlockSpec>> {
    Ok(homebrew_creator::block_catalog())
}

/// The statistic picker's groups (spec 5.2), with the names loaded packs use that the catalog doesn't know.
#[tauri::command]
pub fn homebrew_stat_catalog(state: State<'_, AppState>) -> ApiResult<Vec<HomebrewStatGroup>> {
    let hub = state.hub()?;
    Ok(homebrew_creator::stat_catalog(Some(hub.store.as_ref())))
}

/// NEW PACK (spec 14.1).
#[tauri::command]
pub async fn homebrew_create_pack(
    app: tauri::AppHandle,
    request: HomebrewPackRequest,
) -> ApiResult<ContentPack> {
    crate::commands_content::blocking(app, move |s, sink| s.homebrew_create_pack(&request, sink))
        .await
}

/// Pack settings (spec 13.10).
#[tauri::command]
pub async fn homebrew_update_pack(
    app: tauri::AppHandle,
    pack_id: String,
    patch: HomebrewPackPatch,
) -> ApiResult<ContentPack> {
    crate::commands_content::blocking(app, move |s, sink| {
        s.homebrew_update_pack(&pack_id, &patch, sink)
    })
    .await
}

/// The element list of a pack (spec 13.2).
#[tauri::command(async)]
pub fn homebrew_list_elements(
    state: State<'_, AppState>,
    pack_id: String,
) -> ApiResult<Vec<HomebrewElementSummary>> {
    state.homebrew_list_elements(&pack_id)
}

/// A new element, blank or from a copy (spec 10.1, 11).
#[tauri::command(async)]
pub fn homebrew_new_draft(
    state: State<'_, AppState>,
    pack_id: String,
    kind: String,
    copy_from: Option<String>,
    parent_id: Option<String>,
    deep_copy: Option<bool>,
) -> ApiResult<HomebrewDraftView> {
    state.homebrew_new_draft(
        &pack_id,
        &kind,
        copy_from.as_deref(),
        parent_id.as_deref(),
        deep_copy,
    )
}

/// Open a saved element, resuming its draft, or resume a draft by id (spec 10.1).
#[tauri::command(async)]
pub fn homebrew_open_draft(
    state: State<'_, AppState>,
    pack_id: String,
    element_id: Option<String>,
    draft_id: Option<String>,
) -> ApiResult<HomebrewDraftView> {
    state.homebrew_open_draft(&pack_id, element_id.as_deref(), draft_id.as_deref())
}

/// Every edit: autosave and live validation (spec 8.1, 10.1).
#[tauri::command(async)]
pub fn homebrew_update_draft(
    state: State<'_, AppState>,
    draft_id: String,
    form: HomebrewForm,
) -> ApiResult<HomebrewDraftView> {
    state.homebrew_update_draft(&draft_id, &form)
}

/// Engine checks and the level sweep (spec 8.1 step 6), off the main thread.
#[tauri::command]
pub async fn homebrew_check_draft(
    app: tauri::AppHandle,
    draft_id: String,
) -> ApiResult<Vec<HomebrewProblem>> {
    crate::commands_content::blocking(app, move |s, _| s.homebrew_check_draft(&draft_id)).await
}

/// Validate, write the pack, hot-reload (spec 10.1).
#[tauri::command]
pub async fn homebrew_save_draft(
    app: tauri::AppHandle,
    draft_id: String,
) -> ApiResult<HomebrewSaveResult> {
    crate::commands_content::blocking(app, move |s, sink| s.homebrew_save_draft(&draft_id, sink))
        .await
}

/// Drop a draft.
#[tauri::command(async)]
pub fn homebrew_discard_draft(state: State<'_, AppState>, draft_id: String) -> ApiResult<()> {
    state.homebrew_discard_draft(&draft_id)
}

/// Drafts on this device.
#[tauri::command(async)]
pub fn homebrew_list_drafts(state: State<'_, AppState>) -> ApiResult<Vec<HomebrewDraftSummary>> {
    state.homebrew_list_drafts()
}

/// Delete scan, then delete (spec 10.3).
#[tauri::command]
pub async fn homebrew_delete_elements(
    app: tauri::AppHandle,
    pack_id: String,
    element_ids: Vec<String>,
    confirm: bool,
) -> ApiResult<HomebrewDeleteResult> {
    crate::commands_content::blocking(app, move |s, sink| {
        s.homebrew_delete_elements(&pack_id, &element_ids, confirm, sink)
    })
    .await
}

/// Duplicate inside the pack (spec 12.3).
#[tauri::command(async)]
pub fn homebrew_duplicate_element(
    state: State<'_, AppState>,
    pack_id: String,
    element_id: String,
) -> ApiResult<HomebrewDraftView> {
    state.homebrew_duplicate_element(&pack_id, &element_id)
}

/// Multi-select edits (spec 12.5): one write, one undoable step.
#[tauri::command]
pub async fn homebrew_bulk_edit(
    app: tauri::AppHandle,
    pack_id: String,
    element_ids: Vec<String>,
    patch: HomebrewBulkPatch,
) -> ApiResult<HomebrewBulkResult> {
    crate::commands_content::blocking(app, move |s, sink| {
        s.homebrew_bulk_edit(&pack_id, &element_ids, &patch, sink)
    })
    .await
}

/// Undo a delete or a multi-select edit (spec 10.2, 12.5).
#[tauri::command]
pub async fn homebrew_undo(app: tauri::AppHandle, undo_id: String) -> ApiResult<ContentPack> {
    crate::commands_content::blocking(app, move |s, sink| s.homebrew_undo(&undo_id, sink)).await
}

/// Picker rows (spec 5.1).
#[tauri::command(async)]
pub fn homebrew_reference_options(
    state: State<'_, AppState>,
    target: HomebrewPickerTarget,
    query: String,
    filters: HomebrewPickerFilters,
    page: HomebrewPageRequest,
    draft_id: Option<String>,
) -> ApiResult<Page<HomebrewPickerRow>> {
    state.homebrew_reference_options(&target, &query, &filters, page, draft_id.as_deref())
}

/// Where used (spec 12.3).
#[tauri::command(async)]
pub fn homebrew_where_used(
    state: State<'_, AppState>,
    element_id: String,
) -> ApiResult<Vec<HomebrewElementRef>> {
    state.homebrew_where_used(&element_id)
}

/// A spell list (spec 12.4).
#[tauri::command(async)]
pub fn homebrew_spell_list(
    state: State<'_, AppState>,
    pack_id: String,
    list_tag: String,
) -> ApiResult<HomebrewSpellListView> {
    state.homebrew_spell_list(&pack_id, &list_tag)
}

/// Change a spell list (spec 12.4): one write, one reload.
#[tauri::command]
pub async fn homebrew_set_spell_list(
    app: tauri::AppHandle,
    pack_id: String,
    list_tag: String,
    add: Vec<String>,
    remove: Vec<String>,
) -> ApiResult<HomebrewSpellListView> {
    crate::commands_content::blocking(app, move |s, sink| {
        s.homebrew_set_spell_list(&pack_id, &list_tag, &add, &remove, sink)
    })
    .await
}

/// Try it (spec 9): a test character that takes the draft (`homebrew_trial.rs`).
#[tauri::command(async)]
pub fn homebrew_trial_start(
    state: State<'_, AppState>,
    draft_id: String,
    setup: HomebrewTrialSetup,
) -> ApiResult<HomebrewTrialView> {
    state.homebrew_trial_start(&draft_id, &setup)
}

/// Change the trial character (spec 9): level, setup, or one choice.
#[tauri::command(async)]
pub fn homebrew_trial_update(
    state: State<'_, AppState>,
    trial_id: String,
    change: HomebrewTrialChange,
) -> ApiResult<HomebrewTrialView> {
    state.homebrew_trial_update(&trial_id, &change)
}

/// Drop the trial character (spec 9).
#[tauri::command(async)]
pub fn homebrew_trial_end(state: State<'_, AppState>, trial_id: String) -> ApiResult<()> {
    state.homebrew_trial_end(&trial_id)
}

/// The options of one of the trial character's choices (spec 9 "Choices"), as `get_slot_options` answers them.
#[tauri::command(async)]
pub fn homebrew_trial_options(
    state: State<'_, AppState>,
    trial_id: String,
    slot_id: String,
    query: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> ApiResult<Page<SlotOption>> {
    state.homebrew_trial_options(&trial_id, &slot_id, query.as_deref(), limit, offset)
}

/// Export a pack file (spec 14.2, HC8): a `.borealis-pack.zip`. Desktop asks where to save it in a native dialog the
/// backend opens (a cancelled dialog answers no `path`; nothing is written then).
#[cfg(desktop)]
#[tauri::command(async)]
pub fn homebrew_export_pack(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    pack_id: String,
    leave_out_copied_text: bool,
) -> ApiResult<HomebrewExportResult> {
    use tauri_plugin_dialog::DialogExt;
    let file = state.homebrew_export_file(&pack_id, leave_out_copied_text)?;
    let picked = app
        .dialog()
        .file()
        .add_filter("Borealis pack", &["zip"])
        .set_file_name(&file.file_name)
        .blocking_save_file();
    let mut out = HomebrewExportResult::of(&file);
    let Some(picked) = picked else {
        return Ok(out);
    };
    let mut dest = picked
        .into_path()
        .map_err(|_| crate::error::ApiError::invalid("That location cannot be used."))?;
    if dest.extension().is_none() {
        dest.set_extension("borealis-pack.zip");
    }
    std::fs::write(&dest, &file.bytes)
        .map_err(|e| crate::error::ApiError::io("save the pack file", e))?;
    out.path = Some(dest.to_string_lossy().into_owned());
    Ok(out)
}

/// Phones (spec 16): the share sheet. Android opens it from the backend (`ACTION_SEND`); elsewhere the bytes come back
/// for the web view's share sheet (`dataBase64`).
#[cfg(not(desktop))]
#[tauri::command(async)]
pub fn homebrew_export_pack(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    pack_id: String,
    leave_out_copied_text: bool,
) -> ApiResult<HomebrewExportResult> {
    let file = state.homebrew_export_file(&pack_id, leave_out_copied_text)?;
    share_pack_file(&app, &state, &file)
}

/// The share step of the phone export (compiled everywhere, so the desktop build checks it too).
#[cfg_attr(desktop, allow(dead_code))]
fn share_pack_file(
    app: &tauri::AppHandle,
    state: &AppState,
    file: &crate::homebrew_transfer::PackExportFile,
) -> ApiResult<HomebrewExportResult> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    let io = |e: std::io::Error| crate::error::ApiError::io("prepare the pack file", e);
    let dir = state.paths.data.join("pack-share");
    std::fs::create_dir_all(&dir).map_err(io)?;
    // the platform copies the file before the share sheet returns; the temporary file goes when this ends
    let tmp = tempfile::Builder::new()
        .suffix(".zip")
        .tempfile_in(&dir)
        .map_err(io)?;
    std::fs::write(tmp.path(), &file.bytes).map_err(io)?;
    let shared = crate::native_share::share_file(
        app,
        &crate::native_share::ShareFileArgs {
            path: &tmp.path().to_string_lossy(),
            file_name: &file.file_name,
            mime: "application/zip",
            title: "Borealis pack",
        },
    )?;
    let mut out = HomebrewExportResult::of(file);
    out.shared = shared;
    if !shared {
        out.data_base64 = Some(STANDARD.encode(&file.bytes));
    }
    Ok(out)
}
