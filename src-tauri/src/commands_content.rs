//! `#[tauri::command]` wrappers for the Manage and Content command groups, plus the event sink
//! that pushes progress and reload events to the UI. Long operations run on a blocking thread.

use crate::content_dto::{
    AttackInput, CompatibilityReport, ContentIndex, ContentPack, ContentProgress, ContentSource,
    FiveToolsImportReport, FiveToolsImportRequest, FiveToolsScan, ImportPackRequest,
    ImportPackResult, MoveDirection, RestrictionPreview, UpdateIndexesResult,
};
use crate::content_index::HttpsFetcher;
use crate::content_packs::EventSink;
use crate::content_preview::{ImportPreview, OpenedFile, PreviewImportRequest};
use crate::dto::CharacterSnapshot;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

type S<'a> = State<'a, AppState>;

/// Event names (`EVENTS` in the UI contract).
pub const EVENT_CONTENT_PROGRESS: &str = "borealis://content-progress";
/// 5etools import progress.
pub const EVENT_IMPORT_PROGRESS: &str = "borealis://import_5etools_progress";
/// Content hot-reloaded.
pub const EVENT_CONTENT_RELOADED: &str = "borealis://content-reloaded";
/// The OS handed the app a file ("open with" / share); the UI collects it with `take_opened_files`.
pub const EVENT_OPEN_FILE: &str = "borealis://open-file";

/// Emits content events through the Tauri app handle.
pub struct TauriSink(pub AppHandle);

impl EventSink for TauriSink {
    fn content_progress(&self, progress: &ContentProgress) {
        let _ = self.0.emit(EVENT_CONTENT_PROGRESS, progress);
    }
    fn import_progress(&self, progress: &ContentProgress) {
        let _ = self.0.emit(EVENT_IMPORT_PROGRESS, progress);
    }
    fn content_reloaded(&self, reason: &str) {
        let _ = self.0.emit(
            EVENT_CONTENT_RELOADED,
            serde_json::json!({ "reason": reason }),
        );
    }
}

pub(crate) async fn blocking<T, F>(app: AppHandle, f: F) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce(&AppState, &TauriSink) -> ApiResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let sink = TauriSink(app.clone());
        let state = app.state::<AppState>();
        f(&state, &sink)
    })
    .await
    .map_err(ApiError::internal)?
}

#[tauri::command(async)]
pub fn update_details(
    state: S<'_>,
    character_id: String,
    patch: Value,
) -> ApiResult<CharacterSnapshot> {
    state.update_details(&character_id, &patch)
}

#[tauri::command(async)]
pub fn upsert_attack(
    state: S<'_>,
    character_id: String,
    attack: AttackInput,
) -> ApiResult<CharacterSnapshot> {
    state.upsert_attack(&character_id, &attack)
}

#[tauri::command(async)]
pub fn remove_attack(
    state: S<'_>,
    character_id: String,
    attack_id: String,
) -> ApiResult<CharacterSnapshot> {
    state.remove_attack(&character_id, &attack_id)
}

#[tauri::command(async)]
pub fn move_attack(
    state: S<'_>,
    character_id: String,
    attack_id: String,
    direction: MoveDirection,
) -> ApiResult<CharacterSnapshot> {
    state.move_attack(&character_id, &attack_id, direction)
}

#[tauri::command(async)]
pub fn update_companion(
    state: S<'_>,
    character_id: String,
    patch: Value,
) -> ApiResult<CharacterSnapshot> {
    state.update_companion(&character_id, &patch)
}

#[tauri::command(async)]
pub fn preview_source_restrictions(
    state: S<'_>,
    character_id: String,
    enabled_sources: Option<Vec<String>>,
) -> ApiResult<RestrictionPreview> {
    state.preview_source_restrictions(&character_id, enabled_sources.as_deref())
}

#[tauri::command(async)]
pub fn set_source_restrictions(
    state: S<'_>,
    character_id: String,
    enabled_sources: Option<Vec<String>>,
) -> ApiResult<CharacterSnapshot> {
    state.set_source_restrictions(&character_id, enabled_sources.as_deref())
}

#[tauri::command(async)]
pub fn set_allow_cross_edition(
    state: S<'_>,
    character_id: String,
    allow: Option<bool>,
) -> ApiResult<CharacterSnapshot> {
    state.set_allow_cross_edition(&character_id, allow)
}

#[tauri::command(async)]
pub fn get_compatibility_report(
    state: S<'_>,
    character_id: String,
) -> ApiResult<CompatibilityReport> {
    state.get_compatibility_report(&character_id)
}

#[tauri::command(async)]
pub fn list_packs(state: S<'_>) -> ApiResult<Vec<ContentPack>> {
    state.list_packs()
}

#[tauri::command]
pub async fn import_pack(
    app: AppHandle,
    request: ImportPackRequest,
) -> ApiResult<ImportPackResult> {
    blocking(app, move |state, sink| state.import_pack(&request, sink)).await
}

/// Convert files or a picked folder without installing; the result waits for `install_import_preview`.
#[tauri::command]
pub async fn preview_import_pack(
    app: AppHandle,
    request: PreviewImportRequest,
) -> ApiResult<ImportPreview> {
    blocking(app, move |state, _| state.preview_import_pack(&request)).await
}

#[tauri::command]
pub async fn install_import_preview(
    app: AppHandle,
    preview_id: String,
) -> ApiResult<ImportPackResult> {
    blocking(app, move |state, sink| {
        state.install_import_preview(&preview_id, sink)
    })
    .await
}

#[tauri::command(async)]
pub fn cancel_import_preview(state: S<'_>, preview_id: String) -> ApiResult<()> {
    state.cancel_import_preview(&preview_id)
}

/// Files the OS opened with the app (macOS/iOS "open with", Android share/open), oldest first.
#[tauri::command]
#[must_use]
pub fn take_opened_files() -> Vec<OpenedFile> {
    crate::content_preview::take_opened_files()
}

/// Queue files from `RunEvent::Opened` and tell the UI.
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub fn handle_opened(app: &AppHandle, urls: &[tauri::Url]) {
    for path in urls.iter().filter_map(|u| u.to_file_path().ok()) {
        crate::content_preview::push_opened_file(&path);
    }
    let _ = app.emit(EVENT_OPEN_FILE, ());
}

#[tauri::command(async)]
pub fn set_pack_enabled(
    app: AppHandle,
    state: S<'_>,
    pack_id: String,
    enabled: bool,
) -> ApiResult<Vec<ContentPack>> {
    state.set_pack_enabled(&pack_id, enabled, &TauriSink(app))
}

#[tauri::command(async)]
pub fn remove_pack(app: AppHandle, state: S<'_>, pack_id: String) -> ApiResult<Vec<ContentPack>> {
    state.remove_pack(&pack_id, &TauriSink(app))
}

#[tauri::command(async)]
pub fn list_sources(state: S<'_>) -> ApiResult<Vec<ContentSource>> {
    state.list_sources()
}

#[tauri::command(async)]
pub fn list_indexes(state: S<'_>) -> Vec<ContentIndex> {
    state.list_indexes()
}

#[tauri::command(async)]
pub fn add_index(state: S<'_>, url: String) -> ApiResult<Vec<ContentIndex>> {
    state.add_index(&url)
}

#[tauri::command(async)]
pub fn remove_index(state: S<'_>, url: String) -> ApiResult<Vec<ContentIndex>> {
    state.remove_index(&url)
}

#[tauri::command]
pub async fn update_indexes(app: AppHandle) -> ApiResult<UpdateIndexesResult> {
    blocking(app, |state, sink| state.update_indexes(&HttpsFetcher, sink)).await
}

#[tauri::command(async)]
pub fn clear_downloaded_content(app: AppHandle, state: S<'_>) -> ApiResult<Vec<ContentPack>> {
    state.clear_downloaded_content(&TauriSink(app))
}

#[tauri::command(async)]
pub fn scan_5etools_folder(state: S<'_>, folder: String) -> ApiResult<FiveToolsScan> {
    state.scan_5etools_folder(&folder)
}

#[tauri::command]
pub async fn import_5etools(
    app: AppHandle,
    request: FiveToolsImportRequest,
) -> ApiResult<FiveToolsImportReport> {
    blocking(app, move |state, sink| state.import_5etools(&request, sink)).await
}
