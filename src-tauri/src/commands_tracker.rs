//! Tauri wrappers of the Play tab spell tracker (GitHub #2): spell slots, concentration and
//! custom trackers.

use crate::dto::CharacterSnapshot;
use crate::error::ApiResult;
use crate::state::AppState;
use borealis_engine::state::TrackerReset;
use tauri::State;

type S<'a> = State<'a, AppState>;

#[tauri::command(async)]
pub fn set_spell_slots_used(
    state: S<'_>,
    character_id: String,
    level: u32,
    used: u32,
    pact: Option<bool>,
) -> ApiResult<CharacterSnapshot> {
    state.set_spell_slots_used(&character_id, level, used, pact.unwrap_or(false))
}

#[tauri::command(async)]
pub fn set_concentration(
    state: S<'_>,
    character_id: String,
    spell_id: Option<String>,
) -> ApiResult<CharacterSnapshot> {
    state.set_concentration(&character_id, spell_id.as_deref())
}

#[tauri::command(async)]
pub fn add_tracker(
    state: S<'_>,
    character_id: String,
    name: String,
    max: u32,
    reset: TrackerReset,
) -> ApiResult<CharacterSnapshot> {
    state.add_tracker(&character_id, &name, max, reset)
}

#[tauri::command(async)]
pub fn update_tracker(
    state: S<'_>,
    character_id: String,
    tracker_id: String,
    name: String,
    max: u32,
    reset: TrackerReset,
) -> ApiResult<CharacterSnapshot> {
    state.update_tracker(&character_id, &tracker_id, &name, max, reset)
}

#[tauri::command(async)]
pub fn remove_tracker(
    state: S<'_>,
    character_id: String,
    tracker_id: String,
) -> ApiResult<CharacterSnapshot> {
    state.remove_tracker(&character_id, &tracker_id)
}

#[tauri::command(async)]
pub fn set_tracker_used(
    state: S<'_>,
    character_id: String,
    tracker_id: String,
    used: u32,
) -> ApiResult<CharacterSnapshot> {
    state.set_tracker_used(&character_id, &tracker_id, used)
}
