//! `#[tauri::command]` wrappers for the first-run AI setup. Install output streams on
//! `borealis://ai-setup-event`.

use crate::ai_service::AiService;
use crate::ai_setup::{
    AiSetupStatusDto, InstallConsent, LoginHelp, SetupEvent, SetupSink, EVENT_AI_SETUP,
};
use crate::error::{ApiError, ApiResult, ErrorCode};
use crate::state::AppState;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

/// Emits setup events through the Tauri app handle.
pub struct TauriSetupSink(pub AppHandle);

impl SetupSink for TauriSetupSink {
    fn emit(&self, event: &SetupEvent) {
        let _ = self.0.emit(EVENT_AI_SETUP, event);
    }
}

type S<'a> = State<'a, AppState>;
type A<'a> = State<'a, Arc<AiService>>;

#[tauri::command]
pub async fn ai_setup_status(state: S<'_>, ai: A<'_>) -> ApiResult<AiSetupStatusDto> {
    ai.setup_status(&state).await
}

#[tauri::command]
pub fn ai_setup_confirm_install(ai: A<'_>, provider: String) -> ApiResult<InstallConsent> {
    ai.setup_confirm_install(&provider)
}

#[tauri::command]
pub async fn ai_setup_install(
    app: AppHandle,
    ai: A<'_>,
    provider: String,
    consent_token: String,
) -> ApiResult<()> {
    // The webview minted the consent token itself, so the backend asks again natively, naming
    // the exact command, before npm runs.
    let command = ai.consent_command(&provider, &consent_token)?;
    let handle = app.clone();
    let accepted = tauri::async_runtime::spawn_blocking(move || {
        crate::commands_pick::native_confirm(
            &handle,
            "Install the AI tool?",
            &format!("Borealis will run this command on your computer:\n\n{command}"),
            "Install",
        )
    })
    .await
    .map_err(ApiError::internal)?;
    if !accepted {
        ai.revoke_consent(&consent_token);
        return Err(ApiError::new(
            ErrorCode::RequirementFailed,
            "Installation was cancelled.",
        ));
    }
    ai.setup_install(&provider, &consent_token, Arc::new(TauriSetupSink(app)))
        .await
}

#[tauri::command]
pub fn ai_setup_login(
    ai: A<'_>,
    provider: String,
    open_terminal: Option<bool>,
) -> ApiResult<LoginHelp> {
    ai.setup_login(&provider, open_terminal.unwrap_or(false))
}

#[tauri::command]
pub fn ai_setup_complete(state: S<'_>, ai: A<'_>, skipped: Option<bool>) -> ApiResult<()> {
    ai.setup_complete(&state, skipped.unwrap_or(false))
}
