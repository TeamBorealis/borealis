//! `#[tauri::command]` wrappers for Settings > DATA FILES and the deferred-apply settings flow
//! (reference app: Save = `save_settings`, Restore Defaults = `restore_default_settings`).

use crate::commands_content::blocking;
#[cfg(desktop)]
use crate::commands_content::TauriSink;
use crate::error::ApiResult;
use crate::folder_access::{self, RememberedFolder};
use crate::settings::AppSettings;
use crate::settings_dto::{
    ClearCachesResult, ContentUpdateStatus, DataDirs, PortraitLibrary, SecondaryContentResult,
};
use crate::state::AppState;
use std::path::Path;
use tauri::{AppHandle, State};

type S<'a> = State<'a, AppState>;

#[tauri::command(async)]
pub fn get_data_dirs(state: S<'_>) -> ApiResult<DataDirs> {
    state.data_dirs()
}

/// Keep the security-scoped bookmarks (Mac App Store build, [`crate::folder_access`]) in step with the folder
/// settings after a change: `picked` was just chosen in a panel, so it is bookmarked; a folder whose setting is
/// now empty loses its bookmark. A no-op in every other build.
fn sync_folder_bookmarks(state: &AppState, picked: Option<RememberedFolder>) {
    let Ok(settings) = state.settings() else {
        return;
    };
    for which in RememberedFolder::ALL {
        let value = match which {
            RememberedFolder::Documents => settings.documents_dir.as_deref(),
            RememberedFolder::SecondaryContent => settings.secondary_content_dir.as_deref(),
            RememberedFolder::PortraitLibrary => settings.portrait_library_dir.as_deref(),
            RememberedFolder::Vault => settings.vault_dir.as_deref(),
        };
        match value {
            None => folder_access::after_pick(&state.paths.data, which, None),
            Some(dir) if picked == Some(which) => {
                folder_access::after_pick(&state.paths.data, which, Some(Path::new(dir)));
            }
            Some(_) => {}
        }
    }
}

/// `token` from `pick_path("documents-dir")`; `None` = back to the default folder.
#[tauri::command(async)]
pub fn set_documents_dir(state: S<'_>, token: Option<String>) -> ApiResult<DataDirs> {
    let dirs = state.set_documents_dir(token.as_deref())?;
    sync_folder_bookmarks(&state, Some(RememberedFolder::Documents));
    Ok(dirs)
}

/// `token` from `pick_path("portrait-library-dir")`; `None` = no portrait library (GitHub #32).
#[tauri::command(async)]
pub fn set_portrait_library_dir(state: S<'_>, token: Option<String>) -> ApiResult<DataDirs> {
    let dirs = state.set_portrait_library_dir(token.as_deref())?;
    sync_folder_bookmarks(&state, Some(RememberedFolder::PortraitLibrary));
    Ok(dirs)
}

/// `token` from `pick_path("vault-dir")`; `None` = no Obsidian vault (GitHub #15).
#[tauri::command(async)]
pub fn set_vault_dir(state: S<'_>, token: Option<String>) -> ApiResult<DataDirs> {
    let dirs = state.set_vault_dir(token.as_deref())?;
    sync_folder_bookmarks(&state, Some(RememberedFolder::Vault));
    Ok(dirs)
}

/// The portraits gallery's library images (names only; each image is read with `portrait_library_image`).
#[tauri::command(async)]
pub fn list_portrait_library(state: S<'_>) -> ApiResult<PortraitLibrary> {
    state.portrait_library()
}

/// One portrait library image as a `data:` URL; `id` comes from `list_portrait_library`.
#[tauri::command(async)]
pub fn portrait_library_image(state: S<'_>, id: String) -> ApiResult<String> {
    state.portrait_library_image(&id)
}

/// `token` from `pick_path("secondary-content-dir")`.
#[tauri::command]
pub async fn set_secondary_content_dir(
    app: AppHandle,
    token: String,
) -> ApiResult<SecondaryContentResult> {
    blocking(app, move |state, sink| {
        let result = state.set_secondary_content_dir(&token, sink)?;
        sync_folder_bookmarks(state, Some(RememberedFolder::SecondaryContent));
        Ok(result)
    })
    .await
}

#[tauri::command]
pub async fn unlink_secondary_content_dir(app: AppHandle) -> ApiResult<SecondaryContentResult> {
    blocking(app, |state, sink| {
        let result = state.unlink_secondary_content_dir(sink)?;
        sync_folder_bookmarks(state, None);
        Ok(result)
    })
    .await
}

#[tauri::command]
pub async fn restore_default_settings(app: AppHandle) -> ApiResult<AppSettings> {
    blocking(app, |state, sink| {
        let settings = state.restore_default_settings(sink)?;
        sync_folder_bookmarks(state, None);
        Ok(settings)
    })
    .await
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
