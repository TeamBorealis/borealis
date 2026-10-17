//! Tauri wrappers for magic, equipment and currency, sheet, compendium and traces.

use crate::dto::{CharacterSnapshot, ElementSummary, ExportResult, Page};
use crate::dto_ext::{
    CompendiumFacets, CompendiumQuery, ElementDetail, EquipRequest, ItemFilter, ItemPatch,
    ItemSummary, SheetOptionsDto, SheetPreview, StatTraceDto,
};
#[cfg(desktop)]
use crate::error::ApiError;
use crate::error::ApiResult;
use crate::sheet_api::SheetFile;
use crate::state::AppState;
use tauri::State;

type S<'a> = State<'a, AppState>;

#[tauri::command(async)]
pub fn replace_spell(
    state: S<'_>,
    character_id: String,
    slot_id: String,
) -> ApiResult<CharacterSnapshot> {
    state.replace_spell(&character_id, &slot_id)
}

#[tauri::command(async)]
pub fn set_spell_prepared(
    state: S<'_>,
    character_id: String,
    class_slot_id: String,
    spell_id: String,
    prepared: bool,
) -> ApiResult<CharacterSnapshot> {
    state.set_spell_prepared(&character_id, &class_slot_id, &spell_id, prepared)
}

/// The provenance trace of a statistic.
#[tauri::command(async)]
pub fn explain_stat(state: S<'_>, character_id: String, stat: String) -> ApiResult<StatTraceDto> {
    state.explain_stat(&character_id, &stat)
}

#[tauri::command(async)]
pub fn search_items(
    state: S<'_>,
    category: Option<String>,
    query: Option<String>,
    source: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    character_id: Option<String>,
) -> ApiResult<Page<ItemSummary>> {
    state.search_items(&ItemFilter {
        category,
        query,
        source,
        limit,
        offset,
        character_id,
    })
}

#[tauri::command(async)]
pub fn add_item(
    state: S<'_>,
    character_id: String,
    item_id: String,
    quantity: u32,
    buy: bool,
    base_item_id: Option<String>,
) -> ApiResult<CharacterSnapshot> {
    state.add_item(
        &character_id,
        &item_id,
        quantity,
        buy,
        base_item_id.as_deref(),
    )
}

#[tauri::command(async)]
pub fn remove_item(
    state: S<'_>,
    character_id: String,
    inventory_id: String,
    all: bool,
) -> ApiResult<CharacterSnapshot> {
    state.remove_item(&character_id, &inventory_id, all)
}

#[tauri::command(async)]
pub fn equip_item(
    state: S<'_>,
    character_id: String,
    inventory_id: String,
    slot: EquipRequest,
) -> ApiResult<CharacterSnapshot> {
    state.equip_item(&character_id, &inventory_id, slot)
}

#[tauri::command(async)]
pub fn unequip_item(
    state: S<'_>,
    character_id: String,
    inventory_id: String,
) -> ApiResult<CharacterSnapshot> {
    state.unequip_item(&character_id, &inventory_id)
}

#[tauri::command(async)]
pub fn set_attuned(
    state: S<'_>,
    character_id: String,
    inventory_id: String,
    attuned: bool,
) -> ApiResult<CharacterSnapshot> {
    state.set_attuned(&character_id, &inventory_id, attuned)
}

#[tauri::command(async)]
pub fn set_rolled_max(
    state: S<'_>,
    character_id: String,
    inventory_id: String,
    resource_id: String,
    value: u32,
) -> ApiResult<CharacterSnapshot> {
    state.set_rolled_max(&character_id, &inventory_id, &resource_id, value)
}

#[tauri::command(async)]
pub fn reroll_max(
    state: S<'_>,
    character_id: String,
    inventory_id: String,
    resource_id: String,
) -> ApiResult<CharacterSnapshot> {
    state.reroll_max(&character_id, &inventory_id, &resource_id)
}

#[tauri::command(async)]
pub fn store_item(
    state: S<'_>,
    character_id: String,
    inventory_id: String,
    storage: u8,
) -> ApiResult<CharacterSnapshot> {
    state.store_item(&character_id, &inventory_id, storage)
}

#[tauri::command(async)]
pub fn move_item(
    state: S<'_>,
    character_id: String,
    inventory_id: String,
    direction: String,
) -> ApiResult<CharacterSnapshot> {
    state.move_item(&character_id, &inventory_id, &direction)
}

#[tauri::command(async)]
pub fn update_item_details(
    state: S<'_>,
    character_id: String,
    inventory_id: String,
    patch: ItemPatch,
) -> ApiResult<CharacterSnapshot> {
    state.update_item_details(&character_id, &inventory_id, &patch)
}

#[tauri::command(async)]
pub fn extract_pack(
    state: S<'_>,
    character_id: String,
    inventory_id: String,
) -> ApiResult<CharacterSnapshot> {
    state.extract_pack(&character_id, &inventory_id)
}

#[tauri::command(async)]
pub fn adjust_coins(
    state: S<'_>,
    character_id: String,
    coin: String,
    delta: i64,
) -> ApiResult<CharacterSnapshot> {
    state.adjust_coins(&character_id, &coin, delta)
}

#[tauri::command(async)]
pub fn convert_coins(
    state: S<'_>,
    character_id: String,
    to: String,
) -> ApiResult<CharacterSnapshot> {
    state.convert_coins(&character_id, &to)
}

#[tauri::command(async)]
pub fn set_storage_names(
    state: S<'_>,
    character_id: String,
    names: [String; 2],
) -> ApiResult<CharacterSnapshot> {
    state.set_storage_names(&character_id, names)
}

#[tauri::command(async)]
pub fn render_sheet(
    state: S<'_>,
    character_id: String,
    options: Option<SheetOptionsDto>,
) -> ApiResult<SheetPreview> {
    state.render_sheet(&character_id, &options.unwrap_or_default())
}

/// Render the sheet and save it through the native dialog (desktop). On mobile, or without a
/// window, the bytes come back as base64. A cancelled dialog returns neither `path` nor bytes.
#[tauri::command(async)]
pub fn export_sheet(
    app: tauri::AppHandle,
    state: S<'_>,
    character_id: String,
    options: Option<SheetOptionsDto>,
) -> ApiResult<ExportResult> {
    let file = state.sheet_file(&character_id, &options.unwrap_or_default())?;
    save_sheet(&app, file)
}

#[cfg(desktop)]
fn save_sheet(app: &tauri::AppHandle, file: SheetFile) -> ApiResult<ExportResult> {
    use tauri_plugin_dialog::DialogExt;
    let ext = if file.mime == "text/html" {
        "html"
    } else {
        "pdf"
    };
    let picked = app
        .dialog()
        .file()
        .add_filter("Character sheet", &[ext])
        .set_file_name(&file.file_name)
        .blocking_save_file();
    let mut result = ExportResult {
        file_name: file.file_name.clone(),
        mime: file.mime.into(),
        path: None,
        data_base64: None,
    };
    let Some(picked) = picked else {
        return Ok(result);
    };
    let path = picked
        .into_path()
        .map_err(|_| ApiError::invalid("That location cannot be used."))?;
    std::fs::write(&path, &file.bytes).map_err(|e| ApiError::io("save the sheet", e))?;
    result.path = Some(path.display().to_string());
    Ok(result)
}

#[cfg(not(desktop))]
fn save_sheet(_: &tauri::AppHandle, file: SheetFile) -> ApiResult<ExportResult> {
    Ok(file.into_base64())
}

#[tauri::command(async)]
pub fn search_compendium(state: S<'_>, query: CompendiumQuery) -> ApiResult<Page<ElementSummary>> {
    state.search_compendium(&query)
}

#[tauri::command(async)]
pub fn get_element(state: S<'_>, element_id: String) -> ApiResult<ElementDetail> {
    state.get_element(&element_id)
}

#[tauri::command(async)]
pub fn compendium_facets(state: S<'_>) -> ApiResult<CompendiumFacets> {
    state.compendium_facets()
}
