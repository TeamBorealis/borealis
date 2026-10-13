//! `#[tauri::command]` wrappers for "Import anything" and homebrew. Long stages are spawned on the
//! async runtime and report through `borealis://import-event`.

use crate::ai_service::AiService;
use crate::commands_content::TauriSink;
use crate::content_dto::ContentPack;
use crate::error::ApiResult;
use crate::homebrew_api::ValidationReport;
use crate::import_dto::ImportEvent;
use crate::import_dto::{ImportDraftDto, ImportHintDto, ImportSink, ImportStarted, EVENT_IMPORT};
use crate::import_sources::ImportSourceDto;
use crate::state::AppState;
use serde_json::Value;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

type S<'a> = State<'a, AppState>;
type A<'a> = State<'a, Arc<AiService>>;

/// Emits import events through the Tauri app handle.
pub struct TauriImportSink(pub AppHandle);

impl ImportSink for TauriImportSink {
    fn import_event(&self, event: &ImportEvent) {
        let _ = self.0.emit(EVENT_IMPORT, event);
    }
}

fn sink(app: &AppHandle) -> Arc<dyn ImportSink> {
    Arc::new(TauriImportSink(app.clone()))
}

#[tauri::command]
pub async fn import_start(
    app: AppHandle,
    state: S<'_>,
    ai: A<'_>,
    sources: Vec<ImportSourceDto>,
    hint: Option<ImportHintDto>,
    allow_untrusted_with_codex: Option<bool>,
) -> ApiResult<ImportStarted> {
    let consent = allow_untrusted_with_codex.unwrap_or(false);
    let job = state.import_start(&ai, sources, hint, consent).await?;
    let draft_id = job.draft_id.clone();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = handle.state::<AppState>();
        let ai = handle.state::<Arc<AiService>>();
        state.run_import_start(&ai, job, sink(&handle)).await;
    });
    Ok(ImportStarted { draft_id })
}

#[tauri::command]
pub fn import_get_draft(state: S<'_>, draft_id: String) -> ApiResult<ImportDraftDto> {
    state.import_get_draft(&draft_id)
}

#[tauri::command]
pub async fn import_choose_parent(
    app: AppHandle,
    state: S<'_>,
    ai: A<'_>,
    draft_id: String,
    class_id: String,
    allow_untrusted_with_codex: Option<bool>,
) -> ApiResult<ImportDraftDto> {
    ai.import_authorize(
        &state,
        &draft_id,
        allow_untrusted_with_codex.unwrap_or(false),
    )
    .await?;
    state.import_choose_parent(&draft_id, &class_id)?;
    let dto = state.import_get_draft(&draft_id)?;
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = handle.state::<AppState>();
        let ai = handle.state::<Arc<AiService>>();
        state
            .run_import_continue(&ai, &draft_id, sink(&handle))
            .await;
    });
    Ok(dto)
}

#[tauri::command]
pub async fn import_revise(
    app: AppHandle,
    state: S<'_>,
    ai: A<'_>,
    draft_id: String,
    instruction: String,
    element_id: Option<String>,
    allow_untrusted_with_codex: Option<bool>,
) -> ApiResult<()> {
    ai.import_authorize(
        &state,
        &draft_id,
        allow_untrusted_with_codex.unwrap_or(false),
    )
    .await?;
    state.import_begin_revise(&draft_id, &instruction)?;
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = handle.state::<AppState>();
        let ai = handle.state::<Arc<AiService>>();
        state
            .run_import_revise(
                &ai,
                &draft_id,
                &instruction,
                element_id.as_deref(),
                sink(&handle),
            )
            .await;
    });
    Ok(())
}

#[tauri::command]
pub fn import_update_element(
    state: S<'_>,
    draft_id: String,
    element_id: String,
    json: Value,
) -> ApiResult<ImportDraftDto> {
    state.import_update_element(&draft_id, &element_id, json)
}

#[tauri::command]
pub async fn import_finalize(
    app: AppHandle,
    draft_id: String,
    pack_name: String,
    prefix: String,
) -> ApiResult<ContentPack> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        state.import_finalize(&draft_id, &pack_name, &prefix, &TauriSink(app.clone()))
    })
    .await
    .map_err(crate::error::ApiError::internal)?
}

#[tauri::command]
pub fn import_discard(state: S<'_>, ai: A<'_>, draft_id: String) -> ApiResult<()> {
    ai.import_forget(&draft_id);
    state.import_discard(&draft_id)
}

#[tauri::command]
pub fn ai_validate_homebrew(state: S<'_>, json: Value) -> ApiResult<ValidationReport> {
    state.ai_validate_homebrew(&json)
}

#[tauri::command]
pub async fn ai_add_homebrew_to_pack(
    app: AppHandle,
    json: Value,
    pack_id: Option<String>,
) -> ApiResult<ContentPack> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        state.ai_add_homebrew_to_pack(&json, pack_id.as_deref(), &TauriSink(app.clone()))
    })
    .await
    .map_err(crate::error::ApiError::internal)?
}
