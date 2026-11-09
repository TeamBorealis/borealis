//! Tauri wrappers for magic, equipment and currency, sheet, compendium and traces.

use crate::dto::{CharacterSnapshot, ElementSummary, ExportResult, Page};
use crate::dto_ext::{
    CompendiumFacets, CompendiumQuery, ElementDetail, EquipRequest, ItemCategory, ItemFilter,
    ItemPatch, ItemSummary, SheetOptionsDto, SheetPreview, StatTraceDto,
};
// Only the desktop save dialog builds errors here (phones save through `sheet_save`).
#[cfg(desktop)]
use crate::error::ApiError;
use crate::error::ApiResult;
use crate::search_api::{SearchHit, SearchPassage, SearchQuery};
use crate::search_service::IndexStatus;
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

/// The equipment screen's item categories for a character, in Aurora's order, with counts.
#[tauri::command(async)]
pub fn list_item_categories(state: S<'_>, character_id: String) -> ApiResult<Vec<ItemCategory>> {
    state.list_item_categories(&character_id)
}

/// The base items a generic magic weapon or armor can be added on (empty: it needs none).
#[tauri::command(async)]
pub fn list_base_items(
    state: S<'_>,
    character_id: String,
    item_id: String,
) -> ApiResult<Vec<ItemSummary>> {
    state.list_base_items(&character_id, &item_id)
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
pub fn set_item_requirements_override(
    state: S<'_>,
    character_id: String,
    inventory_id: String,
    on: bool,
) -> ApiResult<CharacterSnapshot> {
    state.set_item_requirements_override(&character_id, &inventory_id, on)
}

#[tauri::command(async)]
pub fn add_to_attacks(
    state: S<'_>,
    character_id: String,
    inventory_id: String,
) -> ApiResult<CharacterSnapshot> {
    state.add_to_attacks(&character_id, &inventory_id)
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
    save_sheet(&app, &state, file)
}

/// Phones (TV2, B-144): open the system share sheet with the sheet PDF (print, mail, files).
/// Android opens it from the backend; elsewhere `shared` is false and the bytes come back for the
/// web view's own share sheet (B-148: `export_sheet` is Save on phones, so it no longer serves them).
#[tauri::command(async)]
pub fn share_sheet(
    app: tauri::AppHandle,
    state: S<'_>,
    character_id: String,
    options: Option<SheetOptionsDto>,
) -> ApiResult<crate::dto_ext::SheetShare> {
    let options = SheetOptionsDto {
        format: Some("pdf".into()),
        ..options.unwrap_or_default()
    };
    let file = state.sheet_file(&character_id, &options)?;
    let shared = if cfg!(target_os = "android") {
        let dir = state.paths.data.join("sheet-share");
        let tmp = crate::sheet_save::temp_copy(&dir, &file.bytes)?;
        crate::native_share::share_file(
            &app,
            &crate::native_share::ShareFileArgs {
                path: &tmp.path().to_string_lossy(),
                file_name: &file.file_name,
                mime: file.mime,
                title: "Character sheet",
            },
        )?
    } else {
        false
    };
    Ok(crate::dto_ext::SheetShare::new(file, shared))
}

#[cfg(desktop)]
fn save_sheet(app: &tauri::AppHandle, _: &AppState, file: SheetFile) -> ApiResult<ExportResult> {
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

/// B-148: Save on Android: the "Save to" picker, then the PDF viewer (`sheet_save`).
#[cfg(target_os = "android")]
fn save_sheet(
    app: &tauri::AppHandle,
    state: &AppState,
    file: SheetFile,
) -> ApiResult<ExportResult> {
    crate::sheet_save::save(app, &file, &state.paths.data)
}

/// B-148: Save on iOS: the Files export picker (`sheet_save`).
#[cfg(target_os = "ios")]
fn save_sheet(app: &tauri::AppHandle, _: &AppState, file: SheetFile) -> ApiResult<ExportResult> {
    crate::sheet_save::save(app, &file)
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

#[tauri::command(async)]
pub fn search(state: S<'_>, request: SearchQuery) -> ApiResult<Page<SearchHit>> {
    state.search(&request)
}

#[tauri::command]
pub fn search_index_status(state: S<'_>) -> IndexStatus {
    state.search_index_status()
}

#[tauri::command(async)]
pub fn get_search_passage(state: S<'_>, doc_key: String) -> ApiResult<SearchPassage> {
    state.get_search_passage(&doc_key)
}
