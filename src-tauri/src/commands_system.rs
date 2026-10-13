//! App-level system commands: open an app-owned folder in the OS file manager and restart.
//! The webview names a folder by enum; it never supplies a path.

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use serde::Deserialize;
use std::path::PathBuf;
use tauri::{AppHandle, State};

type S<'a> = State<'a, AppState>;

/// App-owned folders the UI may ask to have opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppFolder {
    /// Bundled content packs (the reference app "Open content folder").
    Content,
    /// User content packs (the reference app "Open user content folder").
    UserContent,
    /// Saved characters.
    Characters,
}

impl AppState {
    /// Absolute folder behind `folder`, resolved from the app's own data-dir settings.
    #[must_use]
    pub fn app_folder(&self, folder: AppFolder) -> PathBuf {
        match folder {
            AppFolder::Content => self.paths.content_root.clone(),
            AppFolder::UserContent => self.paths.library(),
            AppFolder::Characters => self.characters_dir(),
        }
    }
}

/// OS "open this folder" program for the current platform.
#[cfg(target_os = "macos")]
const OPENER: &str = "open";
#[cfg(target_os = "windows")]
const OPENER: &str = "explorer";
#[cfg(all(unix, not(target_os = "macos")))]
const OPENER: &str = "xdg-open";

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn open_in_os(dir: &std::path::Path) -> ApiResult<()> {
    std::fs::create_dir_all(dir).map_err(|e| ApiError::io("create folder", e))?;
    std::process::Command::new(OPENER)
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| ApiError::io("open folder", e))
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn open_in_os(_dir: &std::path::Path) -> ApiResult<()> {
    Err(ApiError::new(
        crate::error::ErrorCode::InvalidArgument,
        "Opening folders is not available on this device.",
    ))
}

/// Open one of the app's folders in the OS file manager.
#[tauri::command(async)]
pub fn open_app_folder(state: S<'_>, folder: AppFolder) -> ApiResult<()> {
    open_in_os(&state.app_folder(folder))
}

/// Restart the application (new content is loaded once per process).
#[tauri::command]
pub fn restart_app(app: AppHandle) -> ApiResult<()> {
    app.restart()
}
