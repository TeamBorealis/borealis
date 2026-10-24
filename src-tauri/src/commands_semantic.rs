//! `#[tauri::command]` wrappers for Settings > Search semantic model actions (S3.2). Progress streams on
//! [`EVENT_SEMANTIC_DOWNLOAD`]. All run off the main thread: the Android tier probe blocks on a plugin call.

use crate::error::ApiResult;
use crate::semantic_api::SemanticLicense;
use crate::semantic_device::SemanticDevice;
use crate::semantic_service::{SemanticContext, SemanticDownload, SemanticSink, SemanticStatus};
use crate::state::AppState;
use tauri::{AppHandle, Emitter, Manager, State};

type S<'a> = State<'a, AppState>;

/// Download progress event (payload [`SemanticDownload`]).
pub const EVENT_SEMANTIC_DOWNLOAD: &str = "borealis://semantic-download";

/// Forwards download progress to the web view.
pub struct SemanticEvents(pub AppHandle);

impl SemanticSink for SemanticEvents {
    fn download(&self, status: &SemanticDownload) {
        let _ = self.0.emit(EVENT_SEMANTIC_DOWNLOAD, status);
    }
}

fn context(app: &AppHandle, state: &AppState) -> ApiResult<SemanticContext> {
    let profile = app
        .try_state::<SemanticDevice>()
        .map_or_else(|| SemanticDevice::desktop().profile(), |d| d.profile());
    state.semantic_context(&profile, cfg!(mobile))
}

fn lifecycle(app: &AppHandle) -> Option<borealis_embed::Lifecycle> {
    app.try_state::<SemanticDevice>()
        .map(|d| d.lifecycle().clone())
}

#[tauri::command(async)]
pub fn semantic_status(app: AppHandle, state: S<'_>) -> ApiResult<SemanticStatus> {
    Ok(state.semantic_status(&context(&app, &state)?))
}

/// The consent dialog's "Download and turn on" (and Resume): the only path to a network request.
#[tauri::command(async)]
pub fn semantic_download_start(app: AppHandle, state: S<'_>) -> ApiResult<SemanticStatus> {
    let ctx = context(&app, &state)?;
    state.semantic_start(&ctx, lifecycle(&app))
}

#[tauri::command(async)]
pub fn semantic_download_pause(app: AppHandle, state: S<'_>) -> ApiResult<SemanticStatus> {
    Ok(state.semantic_pause(&context(&app, &state)?))
}

#[tauri::command(async)]
pub fn semantic_download_cancel(app: AppHandle, state: S<'_>) -> ApiResult<SemanticStatus> {
    let ctx = context(&app, &state)?;
    state.semantic_cancel(&ctx)
}

/// "Delete model and semantic indexes".
#[tauri::command(async)]
pub fn semantic_delete_model(app: AppHandle, state: S<'_>) -> ApiResult<SemanticStatus> {
    let ctx = context(&app, &state)?;
    state.semantic_delete(&ctx)
}

/// The model's licence and NOTICE for Settings > About.
#[tauri::command]
pub fn semantic_model_license() -> SemanticLicense {
    crate::semantic_api::model_license()
}

/// Startup: progress events go to the web view. No download starts here.
pub(crate) fn install(app: &AppHandle) {
    app.state::<AppState>()
        .semantic
        .set_sink(std::sync::Arc::new(SemanticEvents(app.clone())));
}
