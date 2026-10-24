//! `#[tauri::command]` wrappers for the AI group. Task commands return at once; events stream on
//! `borealis://ai-event`.

use crate::ai_dto::{AiHostStatus, AiSink, AiStatusDto, AiTaskHandle, AiTaskRequest, AiWireEvent};
use crate::ai_service::AiService;
use crate::error::ApiResult;
use crate::state::AppState;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

/// Event name of the AI stream.
pub const EVENT_AI: &str = "borealis://ai-event";

/// Emits AI events through the Tauri app handle.
pub struct TauriAiSink(pub AppHandle);

impl AiSink for TauriAiSink {
    fn emit(&self, event: &AiWireEvent) {
        let _ = self.0.emit(EVENT_AI, event);
    }
}

type S<'a> = State<'a, AppState>;
type A<'a> = State<'a, Arc<AiService>>;

#[tauri::command]
pub async fn ai_status(state: S<'_>, ai: A<'_>) -> ApiResult<AiStatusDto> {
    Ok(ai.status(&state.settings()?.ai).await)
}

#[tauri::command]
pub async fn ai_start_task(
    app: AppHandle,
    state: S<'_>,
    ai: A<'_>,
    request: AiTaskRequest,
) -> ApiResult<AiTaskHandle> {
    ai.launch(&state, request, Arc::new(TauriAiSink(app))).await
}

#[tauri::command]
pub fn ai_cancel_task(ai: A<'_>, task_id: String) -> ApiResult<()> {
    ai.cancel(&task_id);
    Ok(())
}

#[tauri::command]
pub async fn ai_host_enable(
    state: S<'_>,
    ai: A<'_>,
    allow_untrusted_with_codex: Option<bool>,
) -> ApiResult<AiHostStatus> {
    ai.host_enable(&state, allow_untrusted_with_codex.unwrap_or(false))
        .await
}

#[tauri::command]
pub async fn ai_host_disable(state: S<'_>, ai: A<'_>) -> ApiResult<AiHostStatus> {
    ai.host_disable(&state).await
}

#[tauri::command]
pub async fn ai_host_status(state: S<'_>, ai: A<'_>) -> ApiResult<AiHostStatus> {
    ai.host_status(&state).await
}

#[tauri::command]
pub async fn ai_host_begin_pairing(ai: A<'_>) -> ApiResult<AiHostStatus> {
    ai.host_begin_pairing().await
}

#[tauri::command]
pub async fn ai_host_revoke(state: S<'_>, ai: A<'_>, device_id: String) -> ApiResult<AiHostStatus> {
    ai.host_revoke(&state, &device_id).await
}
