//! Debug builds only (`#[cfg(debug_assertions)]` on the module and its handler entry in lib.rs): the
//! S5 cross-platform acceptance digest. A release build neither compiles nor registers it.

use crate::commands_content::blocking;
use crate::error::ApiResult;
use serde_json::Value;
use tauri::AppHandle;

/// Per-type counts and digests of the loaded content plus snapshot digests of 12 fixed characters
/// built from it (`acceptance_digest::compute`).
#[tauri::command]
pub async fn acceptance_digest(app: AppHandle) -> ApiResult<Value> {
    blocking(app, |state, _| crate::acceptance_digest::compute(state)).await
}
