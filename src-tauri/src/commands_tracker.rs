//! Tauri wrappers of the Play tab spell tracker (GitHub #2): spell slots, concentration and
//! custom trackers. Spends and restores are relative, like `spend_resource`.

use crate::dto::CharacterSnapshot;
use crate::error::ApiResult;
use crate::state::AppState;
use crate::tracker_api::TrackerFields;
use borealis_engine::state::TrackerReset;
use tauri::State;

type S<'a> = State<'a, AppState>;

#[tauri::command(async)]
pub fn spend_spell_slot(
    state: S<'_>,
    character_id: String,
    level: u32,
    pact: Option<bool>,
    uses: Option<u32>,
) -> ApiResult<CharacterSnapshot> {
    state.spend_spell_slot(
        &character_id,
        level,
        pact.unwrap_or(false),
        uses.unwrap_or(1),
    )
}

#[tauri::command(async)]
pub fn restore_spell_slot(
    state: S<'_>,
    character_id: String,
    level: u32,
    pact: Option<bool>,
    uses: Option<u32>,
) -> ApiResult<CharacterSnapshot> {
    state.restore_spell_slot(
        &character_id,
        level,
        pact.unwrap_or(false),
        uses.unwrap_or(1),
    )
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
    counts_up: Option<bool>,
) -> ApiResult<CharacterSnapshot> {
    let fields = TrackerFields {
        name: &name,
        max,
        reset,
        counts_up: counts_up.unwrap_or(false),
    };
    state.add_tracker(&character_id, &fields)
}

#[tauri::command(async)]
pub fn update_tracker(
    state: S<'_>,
    character_id: String,
    tracker_id: String,
    name: String,
    max: u32,
    reset: TrackerReset,
    counts_up: Option<bool>,
) -> ApiResult<CharacterSnapshot> {
    let fields = TrackerFields {
        name: &name,
        max,
        reset,
        counts_up: counts_up.unwrap_or(false),
    };
    state.update_tracker(&character_id, &tracker_id, &fields)
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
pub fn spend_tracker(
    state: S<'_>,
    character_id: String,
    tracker_id: String,
    uses: Option<u32>,
) -> ApiResult<CharacterSnapshot> {
    state.spend_tracker(&character_id, &tracker_id, uses.unwrap_or(1))
}

#[tauri::command(async)]
pub fn restore_tracker(
    state: S<'_>,
    character_id: String,
    tracker_id: String,
    uses: Option<u32>,
) -> ApiResult<CharacterSnapshot> {
    state.restore_tracker(&character_id, &tracker_id, uses.unwrap_or(1))
}
