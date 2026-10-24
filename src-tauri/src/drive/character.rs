//! Library and builder tools: characters, slots, options, picks, levels, ability scores.

use super::{parse, ruleset, view, wire_enum, Drive, DriveError, DriveResult};
use crate::dto::{AbilityMethodDto, NewCharacterRequest, OptionsPatch};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const DEFAULT_OPTION_PAGE: usize = 50;
const TRASH_DIR: &str = "trash";
const ABILITIES: [&str; 6] = ["str", "dex", "con", "int", "wis", "cha"];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdArgs {
    pub id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateArgs {
    pub name: String,
    pub ruleset: String,
    pub level: Option<u32>,
    pub ability_method: Option<String>,
    pub feats: Option<bool>,
    pub multiclassing: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetArgs {
    pub id: String,
    pub section: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveArgs {
    pub id: String,
    pub as_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteArgs {
    pub id: String,
    #[serde(default)]
    pub permanent: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotsArgs {
    pub id: String,
    #[serde(default)]
    pub open_only: bool,
    pub tab: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionsArgs {
    pub id: String,
    pub slot_id: String,
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PickArgs {
    pub id: String,
    pub slot_id: String,
    pub element_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnpickArgs {
    pub id: String,
    pub slot_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelUpArgs {
    pub id: String,
    pub class_slot_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilitiesArgs {
    pub id: String,
    pub method: Option<String>,
    #[serde(default)]
    pub scores: BTreeMap<String, i32>,
}

/// Route a library or builder tool; `None` when the name is not one of them.
pub fn dispatch(d: &Drive, name: &str, args: &Value) -> Option<DriveResult<Value>> {
    Some(match name {
        "list_characters" => list(d),
        "create_character" => parse(args).and_then(|a| create(d, &a)),
        "open_character" => parse(args).and_then(|a: IdArgs| open(d, &a.id)),
        "get_character" => parse(args).and_then(|a| get(d, &a)),
        "save_character" => parse(args).and_then(|a| save(d, &a)),
        "close_character" => parse(args).and_then(|a: IdArgs| close(d, &a.id)),
        "delete_character" => parse(args).and_then(|a| delete(d, &a)),
        "list_slots" => parse(args).and_then(|a| slots(d, &a)),
        "list_options" => parse(args).and_then(|a| options(d, &a)),
        "pick" => parse(args).and_then(|a| pick(d, &a)),
        "unpick" => parse(args).and_then(|a| unpick(d, &a)),
        "level_up" => parse(args).and_then(|a| level_up(d, &a)),
        "add_multiclass" => parse(args).and_then(|a: IdArgs| add_multiclass(d, &a.id)),
        "set_abilities" => parse(args).and_then(|a| set_abilities(d, &a)),
        _ => return None,
    })
}

pub fn list(d: &Drive) -> DriveResult<Value> {
    let rows: Vec<Value> = d
        .app()
        .list_characters()?
        .into_iter()
        .map(|c| {
            json!({
                "id": c.id, "name": c.name, "build": c.build,
                "level": c.level, "ruleset": view::ruleset_label(c.ruleset),
            })
        })
        .collect();
    Ok(json!({"count": rows.len(), "characters": rows}))
}

pub fn create(d: &Drive, a: &CreateArgs) -> DriveResult<Value> {
    let rs = ruleset(&a.ruleset)?;
    let method: AbilityMethodDto = wire_enum(
        a.ability_method.as_deref().unwrap_or("manual"),
        "ability method",
    )?;
    let request = NewCharacterRequest {
        name: a.name.clone(),
        ruleset: rs,
        starting_level: a.level.unwrap_or(1),
        gender: String::new(),
        ability_method: method,
        options: OptionsPatch {
            feats: a.feats,
            multiclassing: a.multiclassing,
            ..OptionsPatch::default()
        },
        portrait: None,
        group: None,
    };
    let snap = d.app().create_character(rs, &request)?;
    let snap = d.persist(&snap.id)?;
    Ok(view::summary(&snap))
}

pub fn open(d: &Drive, id: &str) -> DriveResult<Value> {
    Ok(view::summary(&d.app().load_character(id)?))
}

pub fn get(d: &Drive, a: &GetArgs) -> DriveResult<Value> {
    d.ensure_open(&a.id)?;
    let snap = d.app().get_snapshot(&a.id)?;
    view::section(&snap, a.section.as_deref()).map_err(DriveError::invalid)
}

pub fn save(d: &Drive, a: &SaveArgs) -> DriveResult<Value> {
    d.ensure_open(&a.id)?;
    let snap = match &a.as_name {
        // A copy becomes a new library character; the original stays saved as it was.
        Some(name) => {
            d.persist(&a.id)?;
            let copy = d.app().save_character_as(&a.id, Some(name), None)?;
            d.persist(&copy.id)?
        }
        None => d.persist(&a.id)?,
    };
    Ok(view::summary(&snap))
}

pub fn close(d: &Drive, id: &str) -> DriveResult<Value> {
    d.app().close_character(id, false)?;
    Ok(json!({"id": id, "closed": true}))
}

/// Move to `<library>/trash/` unless `permanent` (then the app's own delete).
pub fn delete(d: &Drive, a: &DeleteArgs) -> DriveResult<Value> {
    let path = d.app().character_path(&a.id)?;
    d.app().close_character(&a.id, false)?;
    if !path.is_file() {
        return Err(DriveError::new(
            "not_found",
            "That character no longer exists.",
        ));
    }
    if a.permanent {
        d.app().delete_character(&a.id)?;
        return Ok(json!({"id": a.id, "deleted": true}));
    }
    let trash = d.root().join(TRASH_DIR);
    std::fs::create_dir_all(&trash)
        .map_err(|e| DriveError::new("io_error", format!("cannot create the trash: {e}")))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |t| t.as_secs());
    let target = trash.join(format!("{}-{stamp}.json", a.id));
    std::fs::copy(&path, &target)
        .map_err(|e| DriveError::new("io_error", format!("cannot move to the trash: {e}")))?;
    d.app().delete_character(&a.id)?;
    let shown = target.strip_prefix(d.root()).unwrap_or(&target);
    Ok(json!({"id": a.id, "trashed": shown.to_string_lossy()}))
}

pub fn slots(d: &Drive, a: &SlotsArgs) -> DriveResult<Value> {
    d.ensure_open(&a.id)?;
    let snap = d.app().get_snapshot(&a.id)?;
    let rows: Vec<Value> = snap
        .slots
        .iter()
        .filter(|s| !a.open_only || s.chosen.is_none())
        .filter(|s| a.tab.as_ref().is_none_or(|t| s.tab.eq_ignore_ascii_case(t)))
        .map(view::slot)
        .collect();
    Ok(json!({"count": rows.len(), "slots": rows}))
}

pub fn options(d: &Drive, a: &OptionsArgs) -> DriveResult<Value> {
    d.ensure_open(&a.id)?;
    let page = d.app().get_slot_options(
        &a.id,
        &a.slot_id,
        a.query.as_deref(),
        Some(a.limit.unwrap_or(DEFAULT_OPTION_PAGE)),
        a.offset,
    )?;
    let items: Vec<Value> = page
        .items
        .iter()
        .map(|o| {
            json!({
                "id": o.summary.id, "name": o.summary.name, "type": o.summary.ty,
                "source": o.summary.source_name, "prerequisites": o.summary.prerequisites,
                "disabledReason": o.disabled_reason,
            })
        })
        .collect();
    Ok(json!({"total": page.total, "options": items}))
}

pub fn pick(d: &Drive, a: &PickArgs) -> DriveResult<Value> {
    let snap = d.edit(&a.id, |app| {
        app.apply_selection(&a.id, &a.slot_id, &a.element_id)
            .map(drop)
    })?;
    Ok(view::summary(&snap))
}

pub fn unpick(d: &Drive, a: &UnpickArgs) -> DriveResult<Value> {
    let snap = d.edit(&a.id, |app| {
        app.clear_selection(&a.id, &a.slot_id).map(drop)
    })?;
    Ok(view::summary(&snap))
}

pub fn level_up(d: &Drive, a: &LevelUpArgs) -> DriveResult<Value> {
    let slot = a.class_slot_id.as_deref().unwrap_or("main");
    let snap = d.edit(&a.id, |app| app.level_up(&a.id, slot).map(drop))?;
    Ok(view::summary(&snap))
}

pub fn add_multiclass(d: &Drive, id: &str) -> DriveResult<Value> {
    d.ensure_open(id)?;
    let before: Vec<String> = d
        .app()
        .get_snapshot(id)?
        .slots
        .iter()
        .map(|s| s.id.clone())
        .collect();
    let snap = d.edit(id, |app| app.add_multiclass(id).map(drop))?;
    let added: Vec<Value> = snap
        .slots
        .iter()
        .filter(|s| !before.contains(&s.id))
        .map(view::slot)
        .collect();
    let mut out = view::summary(&snap);
    out["addedSlots"] = json!(added);
    Ok(out)
}

pub fn set_abilities(d: &Drive, a: &AbilitiesArgs) -> DriveResult<Value> {
    if let Some(key) = a.scores.keys().find(|k| !ABILITIES.contains(&k.as_str())) {
        return Err(DriveError::invalid(format!(
            "unknown ability {key}; use str, dex, con, int, wis, cha"
        )));
    }
    let method: Option<AbilityMethodDto> = a
        .method
        .as_deref()
        .map(|m| wire_enum(m, "ability method"))
        .transpose()?;
    let snap = d.edit(&a.id, |app| {
        let current = match method {
            Some(m) => app.set_ability_method(&a.id, m)?.abilities.method,
            None => app.get_snapshot(&a.id)?.abilities.method,
        };
        let rolls = matches!(
            current,
            AbilityMethodDto::Roll3d6 | AbilityMethodDto::Roll4d6
        );
        if a.scores.is_empty() && rolls {
            app.roll_abilities(&a.id, None)?;
        }
        for key in ABILITIES {
            let Some(value) = a.scores.get(key) else {
                continue;
            };
            if current == AbilityMethodDto::StandardArray {
                app.assign_standard_array(&a.id, key, Some(*value))?;
            } else {
                app.set_base_score(&a.id, key, *value)?;
            }
        }
        Ok(())
    })?;
    let mut out = view::summary(&snap);
    out["base"] = serde_json::to_value(snap.abilities.base).unwrap_or(Value::Null);
    Ok(out)
}
