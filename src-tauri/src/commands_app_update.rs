//! `#[tauri::command]` wrappers for the "new version available" notice (see `app_update`).

use crate::app_update::{self, AppUpdateStatus};
use crate::error::ApiResult;

/// `borealis://app-update`: payload is an [`AppUpdateStatus`].
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub const EVENT_APP_UPDATE: &str = "borealis://app-update";

/// The last outcome of the startup check (`idle` before it ran, `unsupported` on mobile, where the
/// UI makes the request itself).
#[tauri::command(async)]
pub fn app_update_status() -> ApiResult<AppUpdateStatus> {
    app_update::current_status()
}

/// "Check now": ask GitHub right away (an explicit click, so it ignores the startup setting).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn check_app_update(app: tauri::AppHandle) -> ApiResult<AppUpdateStatus> {
    use tauri::Emitter;
    let status = tauri::async_runtime::spawn_blocking(|| {
        app_update::cell().set(app_update::check_now(
            &app_update::GithubFetcher,
            std::time::SystemTime::now(),
        ))
    })
    .await
    .map_err(|e| crate::error::ApiError::internal(e.to_string()))?;
    let _ = app.emit(EVENT_APP_UPDATE, &status);
    Ok(status)
}

#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
pub async fn check_app_update() -> ApiResult<AppUpdateStatus> {
    Ok(app_update::unsupported())
}

/// Open the official download page in the default browser. Takes no address: only the built-in
/// constant is ever opened. On mobile this answers an error and the UI opens the page itself.
///
/// # Errors
/// `io` when the OS opener cannot start; `invalid_argument` on mobile.
#[tauri::command(async)]
pub fn open_download_page() -> ApiResult<()> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
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
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        Err(crate::error::ApiError::invalid(
            "Open the download page in your browser.",
        ))
    }
}

/// Desktop startup: after [`app_update::STARTUP_CHECK_DELAY`], on a background thread, ask GitHub
/// for the latest version unless "Check for new versions of Borealis" is off.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) fn spawn_startup_update_check(app: &tauri::AppHandle) {
    use tauri::{Emitter, Manager};
    let handle = app.clone();
    crate::content_auto::spawn_startup_task(move || {
        std::thread::sleep(app_update::STARTUP_CHECK_DELAY);
        let enabled = handle
            .state::<crate::state::AppState>()
            .settings()
            .map_or(true, |s| s.check_app_updates);
        let status = app_update::cell().set(app_update::check_if_enabled(
            enabled,
            &app_update::GithubFetcher,
            std::time::SystemTime::now(),
        ));
        let _ = handle.emit(EVENT_APP_UPDATE, &status);
    });
}
