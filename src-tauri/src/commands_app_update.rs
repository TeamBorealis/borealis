//! `#[tauri::command]` wrappers for the "new version available" notice (see `app_update`) and its Install and
//! restart (`self_update`, desktop direct installs only).

use crate::app_update::{self, AppUpdateStatus};
use crate::app_update_debug;
use crate::error::ApiResult;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::self_update::UpdateMethod;
use crate::state::AppState;
#[cfg(any(target_os = "android", target_os = "ios"))]
use tauri::State;

/// `borealis://app-update`: payload is an [`AppUpdateStatus`].
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub const EVENT_APP_UPDATE: &str = "borealis://app-update";

/// `borealis://app-install`: payload is a [`crate::self_update::InstallProgress`] while an update downloads.
#[cfg(self_update)]
pub const EVENT_APP_INSTALL: &str = "borealis://app-install";

/// How this copy gets a new version (Install and restart only where the updater is registered).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn update_method(app: &tauri::AppHandle) -> UpdateMethod {
    #[cfg(self_update)]
    {
        crate::self_update_plugin::support(app).update_method()
    }
    #[cfg(not(self_update))]
    {
        let _ = app;
        UpdateMethod::Download
    }
}

/// The last outcome of the startup check (`idle` before it ran, `unsupported` on mobile, where the
/// UI makes the request itself). A phone debug build with a release-feed fixture answers from it.
/// A store build (iOS, Google Play, the Mac App Store) answers `disabled` and never checks
/// ([`app_update::phone_status`], [`app_update::unless_store_build`]).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command(async)]
pub fn app_update_status(app: tauri::AppHandle) -> ApiResult<AppUpdateStatus> {
    let stored = app_update::current_status()?.with_method(update_method(&app));
    Ok(app_update::unless_store_build(
        crate::distribution::is_store_build(),
        || stored,
    ))
}

/// Phones: the fixture's answer, else `unsupported` (the UI asks GitHub); `disabled` in a store build.
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command(async)]
pub fn app_update_status(state: State<'_, AppState>) -> ApiResult<AppUpdateStatus> {
    Ok(app_update::phone_status(
        crate::distribution::is_store_build(),
        || fixture_check(&state.paths.data),
    ))
}

/// The debug release-feed fixture's answer, stored like a real check (`None` without a fixture).
#[cfg(any(target_os = "android", target_os = "ios"))]
fn fixture_check(data: &std::path::Path) -> Option<AppUpdateStatus> {
    app_update_debug::check_from_fixture(data, std::time::SystemTime::now())
        .map(|status| app_update::cell().set(status))
}

/// Desktop: one check against GitHub, or against the release-feed fixture in a debug build, marked with how this
/// copy updates.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn desktop_check(data: &std::path::Path, method: UpdateMethod) -> AppUpdateStatus {
    let status = app_update_debug::check_from_fixture(data, std::time::SystemTime::now())
        .unwrap_or_else(|| {
            app_update::check_now(&app_update::GithubFetcher, std::time::SystemTime::now())
        });
    app_update::cell().set(status.with_method(method))
}

/// "Check now": ask GitHub right away (an explicit click, so it ignores the startup setting). A
/// store build (the Mac App Store) answers `disabled` and asks nothing.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn check_app_update(app: tauri::AppHandle) -> ApiResult<AppUpdateStatus> {
    use tauri::{Emitter, Manager};
    let data = app.state::<AppState>().paths.data.clone();
    let store_build = crate::distribution::is_store_build();
    let method = update_method(&app);
    let status = tauri::async_runtime::spawn_blocking(move || {
        app_update::unless_store_build(store_build, || desktop_check(&data, method))
    })
    .await
    .map_err(|e| crate::error::ApiError::internal(e.to_string()))?;
    let _ = app.emit(EVENT_APP_UPDATE, &status);
    Ok(status)
}

/// Phones: the fixture's answer, else `unsupported` (the UI asks GitHub); `disabled` in a store build.
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command(async)]
pub fn check_app_update(state: State<'_, AppState>) -> ApiResult<AppUpdateStatus> {
    Ok(app_update::phone_status(
        crate::distribution::is_store_build(),
        || fixture_check(&state.paths.data),
    ))
}

/// Install and restart (#25, UP2): fetch the signed update of the version the notice showed, check its signature,
/// install it and restart. Only a direct desktop install with the updater registered can (`self_update`); progress
/// goes out on `borealis://app-install`. On success the app restarts and the call never answers.
///
/// # Errors
/// `requirement_failed` when this copy cannot install or the update is not installable (no feed for this platform,
/// another version, a bad signature); `io_error` when the download or the install failed. Nothing is installed
/// unless the signature matched.
#[cfg(self_update)]
#[tauri::command]
pub async fn install_app_update(app: tauri::AppHandle) -> ApiResult<()> {
    use tauri::Emitter;
    let expected = app_update::cell().get().latest;
    let progress = app.clone();
    crate::self_update_plugin::install(&app, expected.as_deref(), move |p| {
        let _ = progress.emit(EVENT_APP_INSTALL, p);
    })
    .await
    .map_err(crate::self_update::InstallFailure::to_api_error)?;
    app.restart()
}

/// Phones and the Mac App Store flavour have no updater: their stores (or the download page) update them.
///
/// # Errors
/// Always `requirement_failed`.
#[cfg(not(self_update))]
#[tauri::command]
pub async fn install_app_update() -> ApiResult<()> {
    Err(crate::self_update::InstallFailure::Unavailable.to_api_error())
}

/// Open the official download page in the default browser. Takes no address: only the built-in
/// constant is ever opened.
///
/// # Errors
/// `io` when the OS opener cannot start (desktop) or the system cannot open the page (phones).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command(async)]
pub fn open_download_page() -> ApiResult<()> {
    open_external(app_update::DOWNLOAD_PAGE)
}

/// Phones: see [`open_external`].
///
/// # Errors
/// `io` when the system cannot open the page.
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command(async)]
pub fn open_download_page(app: tauri::AppHandle) -> ApiResult<()> {
    open_external(&app, app_update::DOWNLOAD_PAGE)
}

/// Open one of the app's fixed web pages ([`crate::links::ALLOWED`]: Help menu, globe, download
/// page) in the system browser (U15). Any other address is refused.
///
/// # Errors
/// `invalid_argument` for an address that is not on the list; `io` when it cannot be opened.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command(async)]
pub fn open_link(url: String) -> ApiResult<()> {
    open_external(allowed_link(&url)?)
}

/// Phones: see [`open_external`].
///
/// # Errors
/// `invalid_argument` for an address that is not on the list; `io` when it cannot be opened.
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command(async)]
pub fn open_link(app: tauri::AppHandle, url: String) -> ApiResult<()> {
    open_external(&app, allowed_link(&url)?)
}

fn allowed_link(url: &str) -> ApiResult<&'static str> {
    crate::links::allowed(url)
        .ok_or_else(|| crate::error::ApiError::invalid("Borealis only opens its own web pages."))
}

/// Desktop: hand a fixed address to the OS opener (`open`, `explorer`, `xdg-open`).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn open_external(url: &'static str) -> ApiResult<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| crate::error::ApiError::io("open the web page", e))
}

/// Phones: hand a fixed address to the system browser through the opener plugin's Rust API (an
/// Android `ACTION_VIEW` intent, `UIApplication.open` on iOS). The web view gets no opener
/// permission, so it cannot open any other address, and the page never loads inside the app.
#[cfg(any(target_os = "android", target_os = "ios"))]
fn open_external(app: &tauri::AppHandle, url: &'static str) -> ApiResult<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| crate::error::ApiError::io("open the web page", e))
}

/// Desktop startup: after [`app_update::STARTUP_CHECK_DELAY`], on a background thread, ask GitHub
/// for the latest version unless "Check for new versions of Borealis" is off. A store build (the Mac
/// App Store) starts no task and never asks.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) fn spawn_startup_update_check(app: &tauri::AppHandle) {
    use tauri::{Emitter, Manager};
    if crate::distribution::is_store_build() {
        app_update::cell().set(app_update::unless_store_build(
            true,
            app_update::unsupported,
        ));
        return;
    }
    let handle = app.clone();
    crate::content_auto::spawn_startup_task(move || {
        std::thread::sleep(app_update::STARTUP_CHECK_DELAY);
        let state = handle.state::<AppState>();
        let enabled = state.settings().map_or(true, |s| s.check_app_updates);
        let status = if enabled {
            desktop_check(&state.paths.data, update_method(&handle))
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
