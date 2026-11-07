//! Tauri wrappers of the Play tab hit points (B-194): damage, healing, temporary hit points and
//! hit dice. Damage and healing are relative, like `spend_resource`.

use crate::dto::CharacterSnapshot;
use crate::error::ApiResult;
use crate::state::AppState;
use tauri::State;

type S<'a> = State<'a, AppState>;

#[tauri::command(async)]
pub fn take_damage(
    state: S<'_>,
    character_id: String,
    amount: u32,
) -> ApiResult<CharacterSnapshot> {
    state.take_damage(&character_id, amount)
}

#[tauri::command(async)]
pub fn regain_hit_points(
    state: S<'_>,
    character_id: String,
    amount: u32,
) -> ApiResult<CharacterSnapshot> {
    state.regain_hit_points(&character_id, amount)
}

#[tauri::command(async)]
pub fn set_temp_hit_points(
    state: S<'_>,
    character_id: String,
    amount: u32,
) -> ApiResult<CharacterSnapshot> {
    state.set_temp_hit_points(&character_id, amount)
}

#[tauri::command(async)]
pub fn spend_hit_die(
    state: S<'_>,
    character_id: String,
    class_id: String,
    roll: u32,
) -> ApiResult<CharacterSnapshot> {
    state.spend_hit_die(&character_id, &class_id, roll)
}

#[tauri::command(async)]
pub fn restore_hit_die(
    state: S<'_>,
    character_id: String,
    class_id: String,
    uses: Option<u32>,
) -> ApiResult<CharacterSnapshot> {
    state.restore_hit_die(&character_id, &class_id, uses.unwrap_or(1))
}
