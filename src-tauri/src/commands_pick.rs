//! Backend-owned native dialogs.
//!
//! The webview asks for a pick by purpose; the backend opens the dialog and returns only an opaque
//! token (plus a display name). Confirmations that guard risky actions are native dialogs shown by
//! the backend, so script in the webview cannot click through them.

#[cfg(desktop)]
use crate::ai_service::AiService;
use crate::content_preview::OpenedFile;
use crate::error::{ApiError, ApiResult};
use crate::picked_paths::{PickPurpose, PickedPathDto};
#[cfg(desktop)]
use crate::settings::AppSettings;
use crate::state::AppState;
use std::path::PathBuf;
#[cfg(desktop)]
use std::sync::Arc;
use tauri::{AppHandle, State};

type S<'a> = State<'a, AppState>;
#[cfg(desktop)]
type A<'a> = State<'a, Arc<AiService>>;

/// Open the native picker for `purpose` (import files: multi-select). Empty when the user cancels.
/// `start_dir` only positions the dialog; it grants nothing.
///
/// `save-character` is not offered: `save_character_as` opens its own save dialog.
#[tauri::command(async)]
pub fn pick_path(
    app: AppHandle,
    state: S<'_>,
    purpose: PickPurpose,
    start_dir: Option<String>,
) -> ApiResult<Vec<PickedPathDto>> {
    let start = start_dir
        .map(|d| PathBuf::from(d.trim()))
        .filter(|d| d.is_absolute() && d.is_dir());
    native_pick(&app, purpose, start)?
        .into_iter()
        .map(|path| state.picked.grant(path, purpose))
        .collect()
}

/// Read and consume `character-file` or `dropped-file` picks. The webview gets file names and
/// contents only; a token works once, for the purpose it was minted for.
#[tauri::command(async)]
pub fn take_picked_files(
    state: S<'_>,
    purpose: PickPurpose,
    tokens: Vec<String>,
) -> ApiResult<Vec<OpenedFile>> {
    crate::picked_files::take_picked_files(&state.picked, purpose, &tokens)
}

/// Use the program behind an `ai-override` pick as the AI CLI, after it validates and the user
/// confirms the exact path in a native dialog. A declined confirmation changes nothing.
#[cfg(desktop)]
#[tauri::command(async)]
pub fn set_ai_override(
    app: AppHandle,
    state: S<'_>,
    ai: A<'_>,
    token: String,
) -> ApiResult<AppSettings> {
    let data_dir = ai.data_dir();
    state.set_ai_override(&token, data_dir.as_deref(), |path| {
        native_confirm(
            &app,
            "Use this program for AI?",
            &format!(
                "Borealis will run this program and send it your prompts and character data:\n\n{}",
                path.display()
            ),
            "Use program",
        )
    })
}

#[cfg(desktop)]
fn native_pick(
    app: &AppHandle,
    purpose: PickPurpose,
    start: Option<PathBuf>,
) -> ApiResult<Vec<PathBuf>> {
    use tauri_plugin_dialog::DialogExt;
    let mut dialog = app.dialog().file();
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    let picked = match purpose {
        PickPurpose::ImportFile => dialog
            .add_filter(
                "Rules content",
                &[
                    "png", "jpg", "jpeg", "webp", "pdf", "docx", "odt", "txt", "md", "json",
                ],
            )
            .blocking_pick_files()
            .unwrap_or_default(),
        PickPurpose::FivetoolsFolder
        | PickPurpose::ContentFolder
        | PickPurpose::DocumentsDir
        | PickPurpose::SecondaryContentDir => dialog.blocking_pick_folder().into_iter().collect(),
        PickPurpose::CharacterFile => dialog
            .add_filter("Character files", crate::picked_files::CHARACTER_EXTENSIONS)
            .blocking_pick_files()
            .unwrap_or_default(),
        PickPurpose::AiOverride => dialog.blocking_pick_file().into_iter().collect(),
        PickPurpose::SaveCharacter => {
            return Err(ApiError::invalid("Use Save as to choose where to save."));
        }
        PickPurpose::DroppedFile => {
            return Err(ApiError::invalid("Drop files on the window to use them."));
        }
    };
    picked
        .into_iter()
        .map(|p| {
            p.into_path()
                .map_err(|_| ApiError::invalid("That location cannot be used."))
        })
        .collect()
}

#[cfg(not(desktop))]
fn native_pick(_: &AppHandle, _: PickPurpose, _: Option<PathBuf>) -> ApiResult<Vec<PathBuf>> {
    Err(ApiError::invalid(
        "Choosing files is not available on this device.",
    ))
}

/// A native OK/Cancel dialog; `true` only when the user pressed the OK button.
#[cfg(desktop)]
pub(crate) fn native_confirm(app: &AppHandle, title: &str, message: &str, ok: &str) -> bool {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    app.dialog()
        .message(message)
        .title(title)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            ok.to_string(),
            "Cancel".to_string(),
        ))
        .blocking_show()
}
