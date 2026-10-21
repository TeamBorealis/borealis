//! Equipment, spell preparation and sheet export tools.

use super::{parse, view, wire_enum, Drive, DriveError, DriveResult};
use crate::dto::CharacterSnapshot;
use crate::dto_ext::{EquipRequest, ItemFilter, SheetOptionsDto};
use serde::Deserialize;
use serde_json::{json, Value};

const DEFAULT_ITEM_PAGE: usize = 25;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchArgs {
    pub id: Option<String>,
    pub query: Option<String>,
    pub category: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddArgs {
    pub id: String,
    pub item_id: String,
    pub quantity: Option<u32>,
    pub base_item_id: Option<String>,
    #[serde(default)]
    pub buy: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemArgs {
    pub id: String,
    pub uid: String,
    #[serde(default)]
    pub all: bool,
    pub slot: Option<String>,
    pub attuned: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellArgs {
    pub id: String,
    pub spell_id: String,
    pub class_slot_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportArgs {
    pub id: String,
    pub path: String,
    pub format: Option<String>,
}

/// Route a gear tool; `None` when the name is not one of them.
pub fn dispatch(d: &Drive, name: &str, args: &Value) -> Option<DriveResult<Value>> {
    Some(match name {
        "search_items" => parse(args).and_then(|a| search(d, &a)),
        "add_item" => parse(args).and_then(|a| add(d, &a)),
        "remove_item" => parse(args).and_then(|a: ItemArgs| {
            only(&a, &["all"])?;
            item_edit(d, &a, |app| app.remove_item(&a.id, &a.uid, a.all))
        }),
        "equip_item" => parse(args).and_then(|a: ItemArgs| {
            only(&a, &["slot"])?;
            let slot: EquipRequest = wire_enum(a.slot.as_deref().unwrap_or("auto"), "slot")?;
            item_edit(d, &a, |app| app.equip_item(&a.id, &a.uid, slot))
        }),
        "unequip_item" => parse(args).and_then(|a: ItemArgs| {
            only(&a, &[])?;
            item_edit(d, &a, |app| app.unequip_item(&a.id, &a.uid))
        }),
        "attune_item" => parse(args).and_then(|a: ItemArgs| {
            only(&a, &["attuned"])?;
            let on = a.attuned.unwrap_or(true);
            item_edit(d, &a, |app| app.set_attuned(&a.id, &a.uid, on))
        }),
        "prepare_spell" => parse(args).and_then(|a| prepare(d, &a, true)),
        "unprepare_spell" => parse(args).and_then(|a| prepare(d, &a, false)),
        "export_sheet" => parse(args).and_then(|a| export(d, &a)),
        _ => return None,
    })
}

/// The item tools share one argument struct; refuse the fields a tool does not take.
fn only(a: &ItemArgs, allowed: &[&str]) -> DriveResult<()> {
    let given = [
        ("all", a.all),
        ("slot", a.slot.is_some()),
        ("attuned", a.attuned.is_some()),
    ];
    match given.iter().find(|(k, set)| *set && !allowed.contains(k)) {
        Some((k, _)) => Err(DriveError::invalid(format!("unknown field `{k}`"))),
        None => Ok(()),
    }
}

fn item_edit(
    d: &Drive,
    a: &ItemArgs,
    f: impl FnOnce(&crate::state::AppState) -> crate::error::ApiResult<CharacterSnapshot>,
) -> DriveResult<Value> {
    let snap = d.edit(&a.id, |app| f(app).map(drop))?;
    with_inventory(&snap)
}

fn with_inventory(snap: &CharacterSnapshot) -> DriveResult<Value> {
    let mut out = view::summary(snap);
    out["inventory"] = view::section(snap, Some("inventory")).map_err(DriveError::invalid)?;
    Ok(out)
}

pub fn search(d: &Drive, a: &SearchArgs) -> DriveResult<Value> {
    if let Some(id) = &a.id {
        d.ensure_open(id)?;
    }
    let filter = ItemFilter {
        category: a.category.clone(),
        query: a.query.clone(),
        source: None,
        limit: Some(a.limit.unwrap_or(DEFAULT_ITEM_PAGE)),
        offset: a.offset,
        character_id: a.id.clone(),
    };
    let page = d.app().search_items(&filter)?;
    let items: Vec<Value> = page
        .items
        .iter()
        .map(|i| {
            json!({
                "id": i.summary.id, "name": i.summary.name, "category": i.category,
                "source": i.summary.source_name, "costCp": i.cost_cp, "weight": i.weight,
                "requiresAttunement": i.requires_attunement,
            })
        })
        .collect();
    Ok(json!({"total": page.total, "items": items}))
}

pub fn add(d: &Drive, a: &AddArgs) -> DriveResult<Value> {
    let qty = a.quantity.unwrap_or(1);
    let snap = d.edit(&a.id, |app| {
        app.add_item(&a.id, &a.item_id, qty, a.buy, a.base_item_id.as_deref())
            .map(drop)
    })?;
    with_inventory(&snap)
}

pub fn prepare(d: &Drive, a: &SpellArgs, prepared: bool) -> DriveResult<Value> {
    let class = a.class_slot_id.as_deref().unwrap_or("main");
    let snap = d.edit(&a.id, |app| {
        app.set_spell_prepared(&a.id, class, &a.spell_id, prepared)
            .map(drop)
    })?;
    let mut out = view::summary(&snap);
    out["spells"] = view::section(&snap, Some("spells")).map_err(DriveError::invalid)?;
    Ok(out)
}

/// Render the sheet and write it to a path inside the library; the extension must match the
/// format so an export can never overwrite a character or settings file.
pub fn export(d: &Drive, a: &ExportArgs) -> DriveResult<Value> {
    let format = a.format.as_deref().unwrap_or("pdf");
    if format != "pdf" && format != "html" {
        return Err(DriveError::invalid(format!(
            "unknown format {format}; use pdf or html"
        )));
    }
    let path = d.scoped_path(&a.path)?;
    let ext_ok = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(format));
    if !ext_ok {
        return Err(DriveError::invalid(format!(
            "the output file must end in .{format}"
        )));
    }
    d.ensure_open(&a.id)?;
    let file = d.app().sheet_file(
        &a.id,
        &SheetOptionsDto {
            format: Some(format.to_string()),
        },
    )?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| DriveError::new("io_error", format!("cannot create the folder: {e}")))?;
    }
    std::fs::write(&path, &file.bytes)
        .map_err(|e| DriveError::new("io_error", format!("cannot write the sheet: {e}")))?;
    Ok(json!({
        "id": a.id,
        "path": path.to_string_lossy(),
        "format": format,
        "bytes": file.bytes.len(),
    }))
}
