//! `#[tauri::command]` wrappers for the "new version available" notice (see `app_update`).

use crate::app_update::{self, AppUpdateStatus};
use crate::app_update_debug;
use crate::error::ApiResult;
use crate::state::AppState;
use tauri::State;

/// `borealis://app-update`: payload is an [`AppUpdateStatus`].
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub const EVENT_APP_UPDATE: &str = "borealis://app-update";

/// The last outcome of the startup check (`idle` before it ran, `unsupported` on mobile, where the
/// UI makes the request itself). A phone debug build with a release-feed fixture answers from it.
#[tauri::command(async)]
pub fn app_update_status(state: State<'_, AppState>) -> ApiResult<AppUpdateStatus> {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        if let Some(status) = fixture_check(&state.paths.data) {
            return Ok(status);
        }
    }
    app_update::current_status()
}

/// The debug release-feed fixture's answer, stored like a real check (`None` without a fixture).
fn fixture_check(data: &std::path::Path) -> Option<AppUpdateStatus> {
    app_update_debug::check_from_fixture(data, std::time::SystemTime::now())
        .map(|status| app_update::cell().set(status))
}

/// Desktop: one check against GitHub, or against the release-feed fixture in a debug build.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn desktop_check(data: &std::path::Path) -> AppUpdateStatus {
    fixture_check(data).unwrap_or_else(|| {
        app_update::cell().set(app_update::check_now(
            &app_update::GithubFetcher,
            std::time::SystemTime::now(),
        ))
    })
}

/// "Check now": ask GitHub right away (an explicit click, so it ignores the startup setting).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn check_app_update(app: tauri::AppHandle) -> ApiResult<AppUpdateStatus> {
    use tauri::{Emitter, Manager};
    let data = app.state::<AppState>().paths.data.clone();
    let status = tauri::async_runtime::spawn_blocking(move || desktop_check(&data))
        .await
        .map_err(|e| crate::error::ApiError::internal(e.to_string()))?;
    let _ = app.emit(EVENT_APP_UPDATE, &status);
    Ok(status)
}

#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command(async)]
pub fn check_app_update(state: State<'_, AppState>) -> ApiResult<AppUpdateStatus> {
    Ok(fixture_check(&state.paths.data).unwrap_or_else(app_update::unsupported))
}

/// Open the official download page in the default browser. Takes no address: only the built-in
/// constant is ever opened.
///
/// # Errors
/// `io` when the OS opener cannot start.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command(async)]
pub fn open_download_page() -> ApiResult<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(app_update::DOWNLOAD_PAGE)
        .spawn()
        .map(|_| ())
        .map_err(|e| crate::error::ApiError::io("open the download page", e))
}

/// Phones: hand the built-in download page to the system browser through the opener plugin's
/// Rust API (an Android `ACTION_VIEW` intent, `UIApplication.open` on iOS). The web view gets no
/// opener permission, so it cannot open any other address, and the page never loads inside the app.
///
/// # Errors
/// `io` when the system cannot open the page.
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command(async)]
pub fn open_download_page(app: tauri::AppHandle) -> ApiResult<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(app_update::DOWNLOAD_PAGE, None::<&str>)
        .map_err(|e| crate::error::ApiError::io("open the download page", e))
}

/// Desktop startup: after [`app_update::STARTUP_CHECK_DELAY`], on a background thread, ask GitHub
/// for the latest version unless "Check for new versions of Borealis" is off.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) fn spawn_startup_update_check(app: &tauri::AppHandle) {
    use tauri::{Emitter, Manager};
    let handle = app.clone();
    crate::content_auto::spawn_startup_task(move || {
        std::thread::sleep(app_update::STARTUP_CHECK_DELAY);
        let state = handle.state::<AppState>();
        let enabled = state.settings().map_or(true, |s| s.check_app_updates);
        let status = if enabled {
            desktop_check(&state.paths.data)
        } else {
            app_update::cell().set(app_update::check_if_enabled(
                false,
                &app_update::GithubFetcher,
                std::time::SystemTime::now(),
            ))
        };
        let _ = handle.emit(EVENT_APP_UPDATE, &status);
    });
}
