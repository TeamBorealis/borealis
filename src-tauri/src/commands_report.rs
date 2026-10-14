//! `#[tauri::command]` wrappers for opt-in import bug reports (BR1-BR3).

use std::sync::Arc;

use crate::error::{ApiError, ApiResult};
use crate::import_bug_report::{
    ImportReportPreview, ImportReportSent, ReportOutbox, ReportRequest,
};
use crate::state::AppState;
use tauri::State;

#[tauri::command(async)]
pub fn import_report_preview(
    state: State<'_, AppState>,
    outbox: State<'_, Arc<ReportOutbox>>,
    draft_id: Option<String>,
    pack_id: Option<String>,
    note: Option<String>,
    flagged: Option<Vec<String>>,
    include_source: Option<bool>,
) -> ApiResult<ImportReportPreview> {
    let req = ReportRequest {
        draft_id,
        pack_id,
        note: note.unwrap_or_default(),
        flagged: flagged.unwrap_or_default(),
        include_source: include_source.unwrap_or(false),
    };
    state.import_report_preview(&outbox, req)
}

#[tauri::command]
pub async fn import_report_send(
    outbox: State<'_, Arc<ReportOutbox>>,
    preview_id: String,
) -> ApiResult<ImportReportSent> {
    let outbox = Arc::clone(&outbox);
    tauri::async_runtime::spawn_blocking(move || outbox.send(&preview_id))
        .await
        .map_err(ApiError::internal)?
}
