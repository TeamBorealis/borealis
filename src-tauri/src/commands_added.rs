//! Tauri wrappers of the builder's Add actions for extras a DM grants (B3, GitHub #35).

use crate::added_picks::AddPickOption;
use crate::dto::CharacterSnapshot;
use crate::error::ApiResult;
use crate::state::AppState;
use borealis_engine::added_picks::AddedKind;
use tauri::State;

type S<'a> = State<'a, AppState>;

#[tauri::command(async)]
pub fn list_added_pick_options(
    state: S<'_>,
    character_id: String,
    kind: AddedKind,
    spellcasting: Option<String>,
) -> ApiResult<Vec<AddPickOption>> {
    state.list_added_pick_options(&character_id, kind, spellcasting.as_deref())
}

#[tauri::command(async)]
pub fn add_pick(
    state: S<'_>,
    character_id: String,
    kind: AddedKind,
    element_id: String,
    spellcasting: Option<String>,
) -> ApiResult<CharacterSnapshot> {
    state.add_pick(&character_id, kind, &element_id, spellcasting.as_deref())
}

#[tauri::command(async)]
pub fn remove_pick(
    state: S<'_>,
    character_id: String,
    inventory_id: String,
) -> ApiResult<CharacterSnapshot> {
    state.remove_pick(&character_id, &inventory_id)
}
