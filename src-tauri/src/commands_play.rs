//! Tauri wrappers of the 2024 play-state commands and "Convert to 2024".

use crate::dto::CharacterSnapshot;
use crate::error::ApiResult;
use crate::play_dto::{ConversionApplied, ConversionPreviewDto};
use crate::state::AppState;
use tauri::State;

type S<'a> = State<'a, AppState>;

#[tauri::command(async)]
pub fn preview_convert_to_2024(
    state: S<'_>,
    character_id: String,
) -> ApiResult<ConversionPreviewDto> {
    state.preview_convert_to_2024(&character_id)
}

#[tauri::command(async)]
pub fn apply_convert_to_2024(state: S<'_>, character_id: String) -> ApiResult<ConversionApplied> {
    state.apply_convert_to_2024(&character_id)
}

#[tauri::command(async)]
pub fn spend_resource(
    state: S<'_>,
    character_id: String,
    resource_id: String,
    uses: Option<u32>,
) -> ApiResult<CharacterSnapshot> {
    state.spend_resource(&character_id, &resource_id, uses.unwrap_or(1))
}

#[tauri::command(async)]
pub fn restore_resource(
    state: S<'_>,
    character_id: String,
    resource_id: String,
    uses: Option<u32>,
) -> ApiResult<CharacterSnapshot> {
    state.restore_resource(&character_id, &resource_id, uses.unwrap_or(1))
}

#[tauri::command(async)]
pub fn short_rest(state: S<'_>, character_id: String) -> ApiResult<CharacterSnapshot> {
    state.short_rest(&character_id)
}

#[tauri::command(async)]
pub fn long_rest(state: S<'_>, character_id: String) -> ApiResult<CharacterSnapshot> {
    state.long_rest(&character_id)
}

#[tauri::command(async)]
pub fn set_toggle(
    state: S<'_>,
    character_id: String,
    toggle_id: String,
    on: bool,
) -> ApiResult<CharacterSnapshot> {
    state.set_toggle(&character_id, &toggle_id, on)
}

#[tauri::command(async)]
pub fn set_exhaustion(
    state: S<'_>,
    character_id: String,
    level: u32,
) -> ApiResult<CharacterSnapshot> {
    state.set_exhaustion(&character_id, level)
}

#[tauri::command(async)]
pub fn set_heroic_inspiration(
    state: S<'_>,
    character_id: String,
    has: bool,
) -> ApiResult<CharacterSnapshot> {
    state.set_heroic_inspiration(&character_id, has)
}

#[tauri::command(async)]
pub fn choose_weapon_masteries(
    state: S<'_>,
    character_id: String,
    weapon_ids: Vec<String>,
) -> ApiResult<CharacterSnapshot> {
    state.choose_weapon_masteries(&character_id, &weapon_ids)
}

#[tauri::command(async)]
pub fn set_wild_shape_forms(
    state: S<'_>,
    character_id: String,
    form_ids: Vec<String>,
) -> ApiResult<CharacterSnapshot> {
    state.set_wild_shape_forms(&character_id, &form_ids)
}

#[tauri::command(async)]
pub fn choose_equipment_package(
    state: S<'_>,
    character_id: String,
    package_id: String,
    option: String,
) -> ApiResult<CharacterSnapshot> {
    state.choose_equipment_package(&character_id, &package_id, &option)
}
