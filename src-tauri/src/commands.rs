//! `#[tauri::command]` wrappers. Each is a thin adapter over an [`AppState`] service so the
//! logic stays testable without a running webview. Names match `COMMANDS` in the UI contract.

use crate::builder::ruleset_profile;
use crate::dto::{
    AbilityMethodDto, CharacterSnapshot, CharacterSummary, ExportResult, ImportReport,
    NewCharacterRequest, OptionsPatch, Page, RecheckReport, Ruleset, RulesetProfileDto, SlotOption,
};
use crate::error::{ApiError, ApiResult};
use crate::settings::AppSettings;
use crate::state::AppState;
use serde_json::Value;
use std::path::PathBuf;
use tauri::State;

type S<'a> = State<'a, AppState>;

/// Version of the running application, from Cargo metadata.
#[tauri::command]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command(async)]
pub fn get_settings(state: S<'_>) -> ApiResult<AppSettings> {
    state.get_settings()
}

#[tauri::command(async)]
pub fn save_settings(state: S<'_>, patch: Value) -> ApiResult<AppSettings> {
    state.save_settings(&patch)
}

#[tauri::command(async)]
pub fn list_characters(state: S<'_>) -> ApiResult<Vec<CharacterSummary>> {
    state.list_characters()
}

#[tauri::command(async)]
pub fn create_character(
    state: S<'_>,
    request: NewCharacterRequest,
) -> ApiResult<CharacterSnapshot> {
    state.create_character(request.ruleset, &request)
}

#[tauri::command(async)]
pub fn load_character(state: S<'_>, character_id: String) -> ApiResult<CharacterSnapshot> {
    state.load_character(&character_id)
}

#[tauri::command(async)]
pub fn get_snapshot(state: S<'_>, character_id: String) -> ApiResult<CharacterSnapshot> {
    state.get_snapshot(&character_id)
}

#[tauri::command(async)]
pub fn save_character(state: S<'_>, character_id: String) -> ApiResult<CharacterSnapshot> {
    state.save_character(&character_id)
}

/// Save a copy. With `name` it becomes a new library character; without, the backend's native
/// save dialog asks for a `.json` file (desktop). The webview never supplies a path.
#[tauri::command(async)]
pub fn save_character_as(
    app: tauri::AppHandle,
    state: S<'_>,
    character_id: String,
    name: Option<String>,
) -> ApiResult<CharacterSnapshot> {
    if name.is_some() {
        return state.save_character_as(&character_id, name.as_deref(), None);
    }
    let Some(chosen) = pick_save_path(&app, &state, &character_id)? else {
        return state.get_snapshot(&character_id);
    };
    state.save_character_as(&character_id, None, Some(&with_json_extension(chosen)))
}

/// The save target with a `.json` extension (added when the dialog returned none or another).
#[must_use]
pub fn with_json_extension(path: PathBuf) -> PathBuf {
    let is_json = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("json"));
    if is_json {
        path
    } else {
        let mut s = path.into_os_string();
        s.push(".json");
        PathBuf::from(s)
    }
}

#[cfg(desktop)]
fn pick_save_path(
    app: &tauri::AppHandle,
    state: &AppState,
    id: &str,
) -> ApiResult<Option<PathBuf>> {
    use tauri_plugin_dialog::DialogExt;
    let export = state.export_character(id)?;
    let picked = app
        .dialog()
        .file()
        .add_filter("Borealis character", &["json"])
        .set_file_name(export.file_name)
        .blocking_save_file();
    picked
        .map(|p| {
            p.into_path()
                .map_err(|_| ApiError::invalid("That location cannot be used."))
        })
        .transpose()
}

#[cfg(not(desktop))]
fn pick_save_path(_: &tauri::AppHandle, _: &AppState, _: &str) -> ApiResult<Option<PathBuf>> {
    Err(ApiError::invalid("Choose a name for the copy."))
}

#[tauri::command(async)]
pub fn close_character(state: S<'_>, character_id: String, discard: bool) -> ApiResult<()> {
    state.close_character(&character_id, discard)
}

#[tauri::command(async)]
pub fn delete_character(state: S<'_>, character_id: String) -> ApiResult<()> {
    state.delete_character(&character_id)
}

#[tauri::command(async)]
pub fn duplicate_character(state: S<'_>, character_id: String) -> ApiResult<CharacterSummary> {
    state.duplicate_character(&character_id)
}

#[tauri::command(async)]
pub fn update_character_meta(
    state: S<'_>,
    character_id: String,
    name: Option<String>,
    group: Option<String>,
    favorite: Option<bool>,
) -> ApiResult<CharacterSnapshot> {
    state.update_character_meta(&character_id, name.as_deref(), group.as_deref(), favorite)
}

#[tauri::command(async)]
pub fn set_portrait(
    state: S<'_>,
    character_id: String,
    data_url: Option<String>,
) -> ApiResult<CharacterSnapshot> {
    state.set_portrait(&character_id, data_url.as_deref())
}

#[tauri::command(async)]
pub fn import_character(
    state: S<'_>,
    file_name: String,
    content_base64: String,
) -> ApiResult<ImportReport> {
    state.import_character(&file_name, &content_base64)
}

#[tauri::command(async)]
pub fn recheck_missing_content(state: S<'_>) -> ApiResult<RecheckReport> {
    state.recheck_missing_content()
}

#[tauri::command(async)]
pub fn export_character(state: S<'_>, character_id: String) -> ApiResult<ExportResult> {
    state.export_character(&character_id)
}

#[tauri::command(async)]
pub fn get_slot_options(
    state: S<'_>,
    character_id: String,
    slot_id: String,
    query: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> ApiResult<Page<SlotOption>> {
    state.get_slot_options(&character_id, &slot_id, query.as_deref(), limit, offset)
}

#[tauri::command(async)]
pub fn apply_selection(
    state: S<'_>,
    character_id: String,
    slot_id: String,
    element_id: String,
) -> ApiResult<CharacterSnapshot> {
    state.apply_selection(&character_id, &slot_id, &element_id)
}

#[tauri::command(async)]
pub fn clear_selection(
    state: S<'_>,
    character_id: String,
    slot_id: String,
) -> ApiResult<CharacterSnapshot> {
    state.clear_selection(&character_id, &slot_id)
}

#[tauri::command(async)]
pub fn set_xp(state: S<'_>, character_id: String, xp: i64) -> ApiResult<CharacterSnapshot> {
    state.set_xp(&character_id, xp)
}

#[tauri::command(async)]
pub fn level_up(
    state: S<'_>,
    character_id: String,
    class_slot_id: String,
) -> ApiResult<CharacterSnapshot> {
    state.level_up(&character_id, &class_slot_id)
}

#[tauri::command(async)]
pub fn level_down(state: S<'_>, character_id: String) -> ApiResult<CharacterSnapshot> {
    state.level_down(&character_id)
}

#[tauri::command(async)]
pub fn add_multiclass(state: S<'_>, character_id: String) -> ApiResult<CharacterSnapshot> {
    state.add_multiclass(&character_id)
}

#[tauri::command(async)]
pub fn set_hp_roll(
    state: S<'_>,
    character_id: String,
    class_slot_id: String,
    level: u32,
    value: u32,
) -> ApiResult<CharacterSnapshot> {
    state.set_hp_roll(&character_id, &class_slot_id, level, value)
}

#[tauri::command(async)]
pub fn set_ability_method(
    state: S<'_>,
    character_id: String,
    method: AbilityMethodDto,
) -> ApiResult<CharacterSnapshot> {
    state.set_ability_method(&character_id, method)
}

#[tauri::command(async)]
pub fn set_base_score(
    state: S<'_>,
    character_id: String,
    ability: String,
    value: i32,
) -> ApiResult<CharacterSnapshot> {
    state.set_base_score(&character_id, &ability, value)
}

#[tauri::command(async)]
pub fn roll_abilities(
    state: S<'_>,
    character_id: String,
    ability: Option<String>,
) -> ApiResult<CharacterSnapshot> {
    state.roll_abilities(&character_id, ability.as_deref())
}

#[tauri::command(async)]
pub fn assign_standard_array(
    state: S<'_>,
    character_id: String,
    ability: String,
    value: Option<i32>,
) -> ApiResult<CharacterSnapshot> {
    state.assign_standard_array(&character_id, &ability, value)
}

#[tauri::command(async)]
pub fn reset_abilities(state: S<'_>, character_id: String) -> ApiResult<CharacterSnapshot> {
    state.reset_abilities(&character_id)
}

#[tauri::command(async)]
pub fn set_character_options(
    state: S<'_>,
    character_id: String,
    options: OptionsPatch,
) -> ApiResult<CharacterSnapshot> {
    state.set_character_options(&character_id, &options)
}

#[tauri::command]
pub fn get_ruleset_profile(ruleset: Ruleset) -> ApiResult<RulesetProfileDto> {
    Ok(ruleset_profile(ruleset))
}
