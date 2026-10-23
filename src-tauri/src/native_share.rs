//! Android share sheet for text (B-40). The Android System WebView has no Web Share API, so "Share
//! logs" goes through a small in-app Kotlin plugin (`SharePlugin.kt` in the Android project): it
//! writes the text to a file in the app's cache folder and sends it with `ACTION_SEND` and a
//! `FileProvider` content URI. iOS keeps the web view's own share sheet; desktop opens the logs
//! folder. Everywhere but Android this answers "not available", and the UI copies to the clipboard.
//!
//! The same plugin hands over the files Android opened with Borealis (B-24): the activity reads each
//! one when the intent arrives and parks it; the UI pulls them through `take_opened_files` on mount
//! and when nudged, so a file that starts the app is not lost before the page exists.

use crate::error::{ApiError, ApiResult, ErrorCode};
use serde::Serialize;

/// What the Kotlin `shareText` command receives.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareTextArgs<'a> {
    pub text: &'a str,
    pub file_name: &'a str,
    pub title: &'a str,
}

/// The message the UI shows when the platform has no native share.
pub const NOT_AVAILABLE: &str = "Sharing is not available on this device.";

/// Check the request before it reaches the platform.
///
/// # Errors
/// `InvalidArgument` for empty text or a file name that is not a plain `name.ext`.
pub fn validate(args: &ShareTextArgs<'_>) -> ApiResult<()> {
    if args.text.trim().is_empty() {
        return Err(ApiError::new(
            ErrorCode::InvalidArgument,
            "There is nothing to share.",
        ));
    }
    let plain = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.';
    if args.file_name.is_empty()
        || args.file_name.starts_with('.')
        || !args.file_name.chars().all(plain)
    {
        return Err(ApiError::new(
            ErrorCode::InvalidArgument,
            "Unusable share file name.",
        ));
    }
    Ok(())
}

#[cfg(target_os = "android")]
mod android {
    use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
    use tauri::{Manager, Runtime};

    /// The registered Kotlin plugin, kept in app state.
    pub struct ShareHandle<R: Runtime>(pub PluginHandle<R>);

    /// Registers `app.borealis.SharePlugin` (Kotlin, in the Android project).
    pub fn init<R: Runtime>() -> TauriPlugin<R> {
        Builder::new("borealis-share")
            .setup(|app, api| {
                let handle = api.register_android_plugin("app.borealis", "SharePlugin")?;
                app.manage(ShareHandle(handle));
                Ok(())
            })
            .build()
    }
}

#[cfg(target_os = "android")]
pub use android::init;

/// Open the system share sheet with `args.text` as a file.
///
/// # Errors
/// `InvalidArgument` (see [`validate`]), `Io` when the share sheet cannot open, and
/// `InvalidArgument` with [`NOT_AVAILABLE`] off Android.
#[cfg(target_os = "android")]
pub fn share_text<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    args: &ShareTextArgs<'_>,
) -> ApiResult<()> {
    use tauri::Manager;
    validate(args)?;
    let handle = app
        .try_state::<android::ShareHandle<R>>()
        .ok_or_else(|| ApiError::new(ErrorCode::InvalidArgument, NOT_AVAILABLE))?;
    handle
        .0
        .run_mobile_plugin::<serde_json::Value>("shareText", args)
        .map(|_| ())
        .map_err(|e| {
            tracing::warn!("share sheet failed to open");
            ApiError::new(
                ErrorCode::IoError,
                format!("The share sheet could not open: {e}"),
            )
        })
}

/// Off Android there is no native share here (see the module docs).
///
/// # Errors
/// Always `InvalidArgument` with [`NOT_AVAILABLE`] (after [`validate`]).
#[cfg(not(target_os = "android"))]
pub fn share_text<R: tauri::Runtime>(
    _app: &tauri::AppHandle<R>,
    args: &ShareTextArgs<'_>,
) -> ApiResult<()> {
    validate(args)?;
    Err(ApiError::new(ErrorCode::InvalidArgument, NOT_AVAILABLE))
}

/// What the Kotlin `takeOpenedFiles` command answers.
#[derive(Debug, Default, serde::Deserialize)]
pub struct TakenFiles {
    pub files: Vec<crate::content_preview::OpenedFile>,
}

/// The files Android opened with Borealis since the last call, oldest first. A failed plugin call
/// is logged and answers none (the UI asks again on the next nudge).
#[cfg(target_os = "android")]
pub fn take_opened_files<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Vec<crate::content_preview::OpenedFile> {
    use tauri::Manager;
    let Some(handle) = app.try_state::<android::ShareHandle<R>>() else {
        return Vec::new();
    };
    match handle
        .0
        .run_mobile_plugin::<TakenFiles>("takeOpenedFiles", ())
    {
        Ok(taken) => taken.files,
        Err(e) => {
            tracing::warn!("could not take opened files: {e}");
            Vec::new()
        }
    }
}

/// Off Android the OS hands files to Rust directly (`content_preview::push_opened_file`).
#[cfg(not(target_os = "android"))]
#[must_use]
pub fn take_opened_files<R: tauri::Runtime>(
    _app: &tauri::AppHandle<R>,
) -> Vec<crate::content_preview::OpenedFile> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args<'a>(text: &'a str, file_name: &'a str) -> ShareTextArgs<'a> {
        ShareTextArgs {
            text,
            file_name,
            title: "Borealis logs",
        }
    }

    #[test]
    fn empty_text_is_refused() {
        assert!(validate(&args("  \n", "borealis-logs.txt")).is_err());
    }

    #[test]
    fn file_names_must_be_plain() {
        assert!(validate(&args("x", "borealis-logs.txt")).is_ok());
        for bad in ["", "../x.txt", "a/b.txt", ".hidden", "a b.txt"] {
            assert!(validate(&args("x", bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_payload_is_camel_case() {
        let json = serde_json::to_value(args("t", "f.txt")).expect("serialize");
        assert_eq!(json["fileName"], "f.txt");
        assert_eq!(json["text"], "t");
        assert_eq!(json["title"], "Borealis logs");
    }

    #[test]
    fn the_kotlin_opened_files_answer_parses() {
        // The shape `SharePlugin.takeOpenedFiles` resolves with (B-24).
        let answer = r#"{"files":[{"fileName":"hero.dnd5e","contentBase64":"PGE+"}]}"#;
        let taken: TakenFiles = serde_json::from_str(answer).expect("parse");
        assert_eq!(taken.files.len(), 1);
        assert_eq!(taken.files[0].file_name, "hero.dnd5e");
        assert_eq!(taken.files[0].content_base64, "PGE+");
        let none: TakenFiles = serde_json::from_str(r#"{"files":[]}"#).expect("parse");
        assert!(none.files.is_empty());
    }
}
