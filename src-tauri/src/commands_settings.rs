//! `#[tauri::command]` wrappers for Settings > DATA FILES and the deferred-apply settings flow
//! (reference app: Save = `save_settings`, Restore Defaults = `restore_default_settings`).

use crate::commands_content::blocking;
#[cfg(desktop)]
use crate::commands_content::TauriSink;
use crate::error::ApiResult;
use crate::settings::AppSettings;
use crate::settings_dto::{
    ClearCachesResult, ContentUpdateStatus, DataDirs, SecondaryContentResult,
};
use crate::state::AppState;
use tauri::{AppHandle, State};

type S<'a> = State<'a, AppState>;

#[tauri::command(async)]
pub fn get_data_dirs(state: S<'_>) -> ApiResult<DataDirs> {
    state.data_dirs()
}

/// `token` from `pick_path("documents-dir")`; `None` = back to the default folder.
#[tauri::command(async)]
pub fn set_documents_dir(state: S<'_>, token: Option<String>) -> ApiResult<DataDirs> {
    state.set_documents_dir(token.as_deref())
}

/// `token` from `pick_path("secondary-content-dir")`.
#[tauri::command]
pub async fn set_secondary_content_dir(
    app: AppHandle,
    token: String,
) -> ApiResult<SecondaryContentResult> {
    blocking(app, move |state, sink| {
        state.set_secondary_content_dir(&token, sink)
    })
    .await
}

#[tauri::command]
pub async fn unlink_secondary_content_dir(app: AppHandle) -> ApiResult<SecondaryContentResult> {
    blocking(app, |state, sink| state.unlink_secondary_content_dir(sink)).await
}

#[tauri::command]
pub async fn restore_default_settings(app: AppHandle) -> ApiResult<AppSettings> {
    blocking(app, |state, sink| state.restore_default_settings(sink)).await
}

#[tauri::command(async)]
pub fn clear_caches(state: S<'_>) -> ApiResult<ClearCachesResult> {
    state.clear_caches(borealis_ai::sweep_stale_temp_dirs)
}

#[tauri::command(async)]
pub fn content_update_status(state: S<'_>) -> ApiResult<ContentUpdateStatus> {
    state.content_update_status()
}

/// Desktop startup: load the secondary folder now and run the automatic content update after
/// [`crate::content_auto::STARTUP_UPDATE_DELAY`], on a background thread.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) fn spawn_startup_content_tasks(app: &AppHandle) {
    use tauri::Manager;
    app.state::<AppState>().mark_content_update_pending();
    let handle = app.clone();
    crate::content_auto::spawn_startup_task(move || {
        let state = handle.state::<AppState>();
        let sink = TauriSink(handle.clone());
        state.startup_content_tasks(
            &crate::content_index::HttpsFetcher,
            &sink,
            crate::content_auto::STARTUP_UPDATE_DELAY,
        );
    });
}
