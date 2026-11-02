//! Companion services (GitHub #13, decision U16(c)): add and remove companions by hand, and
//! change any companion's name, picture, creature, notes, traits and bonuses. The granted
//! companion (id `granted`) keeps its name in `details.companion_name` and its picture in the UI
//! extras, as before; everything else lives in `Character::companions`.

use crate::companion_list::GRANTED_ID;
use crate::dto::CharacterSnapshot;
use crate::error::{ApiError, ApiResult};
use crate::extras::{portrait_from_data_url, set_ui_extras, ui_extras};
use crate::session::Draft;
use crate::state::{AppState, ContentHub};
use borealis_character::companions::validated_name;
use borealis_character::{AddedCompanion, CompanionEdits, CompanionError, CompanionTrait};
use serde_json::{Map, Value};

/// Longest granted-companion name accepted (the field took any text before; this keeps it bounded).
const MAX_GRANTED_NAME: usize = 20_000;
/// Element types a companion's creature can be.
const CREATURE_TYPES: [&str; 2] = ["Companion", "Monster"];

fn api_error(e: CompanionError) -> ApiError {
    match e {
        CompanionError::Invalid(_) => ApiError::invalid(e.to_string()),
        CompanionError::NotFound(_) => ApiError::not_found(e.to_string()),
        CompanionError::TooMany => ApiError::requirement(e.to_string()),
    }
}

fn text(key: &str, value: &Value) -> ApiResult<String> {
    value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| ApiError::invalid(format!("\"{key}\" must be text.")))
}

fn bonus(key: &str, value: &Value) -> ApiResult<i32> {
    value
        .as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .ok_or_else(|| ApiError::invalid(format!("\"{key}\" must be a whole number.")))
}

fn portrait(value: &Value) -> ApiResult<Option<String>> {
    match value {
        Value::Null => Ok(None),
        Value::String(url) => {
            portrait_from_data_url(url).map_err(ApiError::invalid)?;
            Ok(Some(url.clone()))
        }
        _ => Err(ApiError::invalid("\"portrait\" must be an image or null.")),
    }
}

/// A creature id the installed content defines as a `Companion` or `Monster`.
fn creature(hub: &ContentHub, value: &Value) -> ApiResult<String> {
    let id = match value {
        Value::Null => return Ok(String::new()),
        Value::String(s) if s.trim().is_empty() => return Ok(String::new()),
        Value::String(s) => s.trim(),
        _ => return Err(ApiError::invalid("\"creatureId\" must be text or null.")),
    };
    match hub.element(id) {
        Some(el) if CREATURE_TYPES.contains(&el.ty.as_str()) => Ok(id.to_string()),
        Some(_) => Err(ApiError::invalid("That element is not a creature.")),
        None => Err(ApiError::not_found(
            "That creature is not in the installed content.",
        )),
    }
}

/// The edits after one patch key, or `None` when the key is not an edit key.
fn edit(old: &CompanionEdits, key: &str, value: &Value) -> ApiResult<Option<CompanionEdits>> {
    let new = match key {
        "notes" => CompanionEdits {
            notes: text(key, value)?,
            ..old.clone()
        },
        "traits" => CompanionEdits {
            traits: serde_json::from_value::<Vec<CompanionTrait>>(value.clone()).map_err(|_| {
                ApiError::invalid("\"traits\" must be a list of { name, text } entries.")
            })?,
            ..old.clone()
        },
        "armorClassBonus" => CompanionEdits {
            armor_class_bonus: bonus(key, value)?,
            ..old.clone()
        },
        "hitPointsBonus" => CompanionEdits {
            hit_points_bonus: bonus(key, value)?,
            ..old.clone()
        },
        "speedBonus" => CompanionEdits {
            speed_bonus: bonus(key, value)?,
            ..old.clone()
        },
        _ => return Ok(None),
    };
    Ok(Some(new))
}

fn unknown(key: &str) -> ApiError {
    ApiError::invalid(format!("Unknown companion field \"{key}\"."))
}

/// The granted companion after a patch (its name and picture stay where they always were).
fn patch_granted(d: &mut Draft, patch: &Map<String, Value>) -> ApiResult<()> {
    let mut extras = ui_extras(&d.character);
    let mut edits = d.character.companions.granted.clone();
    for (key, value) in patch {
        match key.as_str() {
            "name" => {
                let name = text(key, value)?;
                if name.chars().count() > MAX_GRANTED_NAME {
                    return Err(ApiError::invalid("\"name\" is too long."));
                }
                d.character.details.companion_name = name.trim().to_string();
            }
            "portrait" => extras.companion_portrait = portrait(value)?,
            _ => edits = edit(&edits, key, value)?.ok_or_else(|| unknown(key))?,
        }
    }
    d.character.companions.granted = edits.validated().map_err(api_error)?;
    set_ui_extras(&mut d.character, &extras);
    Ok(())
}

/// An added companion after a patch.
fn patched(
    hub: &ContentHub,
    old: &AddedCompanion,
    patch: &Map<String, Value>,
) -> ApiResult<AddedCompanion> {
    let mut new = old.clone();
    for (key, value) in patch {
        match key.as_str() {
            "name" => new.name = validated_name(&text(key, value)?).map_err(api_error)?,
            "portrait" => new.portrait = portrait(value)?,
            "creatureId" => new.creature = creature(hub, value)?,
            _ => new.edits = edit(&new.edits, key, value)?.ok_or_else(|| unknown(key))?,
        }
    }
    let edits = new.edits.validated().map_err(api_error)?;
    Ok(AddedCompanion { edits, ..new })
}

impl AppState {
    /// Add a companion by hand: a creature (a `Companion` or `Monster` element, or none) and an
    /// optional name.
    ///
    /// # Errors
    /// `invalid_argument` for a bad name or a non-creature element, `not_found` for an unknown
    /// creature, `requirement_failed` at the limit of added companions.
    pub fn add_companion(
        &self,
        id: &str,
        creature_id: Option<&str>,
        name: Option<&str>,
    ) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let creature = creature(ctx.hub, &creature_id.map_or(Value::Null, Value::from))?;
            let (companions, _) = d
                .character
                .companions
                .with_added(&creature, name.unwrap_or(""))
                .map_err(api_error)?;
            d.character.companions = companions;
            Ok(())
        })
    }

    /// Remove a companion added by hand. The granted one is removed by clearing its select.
    ///
    /// # Errors
    /// `requirement_failed` for the granted companion, `not_found` for an unknown id.
    pub fn remove_companion(&self, id: &str, companion_id: &str) -> ApiResult<CharacterSnapshot> {
        if companion_id == GRANTED_ID {
            return Err(ApiError::requirement(
                "This companion comes from a feature: change it on the builder's Companion tab.",
            ));
        }
        self.mutate(id, |_, d| {
            d.character.companions = d
                .character
                .companions
                .without(companion_id)
                .map_err(api_error)?;
            Ok(())
        })
    }

    /// Change a companion: `name`, `portrait` (`null` clears it), `notes`, `traits`,
    /// `armorClassBonus`, `hitPointsBonus`, `speedBonus`, and for an added one `creatureId`.
    /// Without a `companion_id` the granted companion is changed, as before.
    ///
    /// # Errors
    /// `invalid_argument` for bad values or unknown fields, `not_found` for an unknown companion.
    pub fn update_companion(
        &self,
        id: &str,
        companion_id: Option<&str>,
        patch: &Value,
    ) -> ApiResult<CharacterSnapshot> {
        let map = patch
            .as_object()
            .ok_or_else(|| ApiError::invalid("The companion patch must be an object."))?;
        let target = companion_id.unwrap_or(GRANTED_ID);
        self.mutate(id, |ctx, d| {
            if target == GRANTED_ID {
                return patch_granted(d, map);
            }
            let old = d
                .character
                .companions
                .added(target)
                .ok_or_else(|| api_error(CompanionError::NotFound(target.to_string())))?;
            let new = patched(ctx.hub, old, map)?;
            d.character.companions = d
                .character
                .companions
                .with_changed(target, |_| Ok(new))
                .map_err(api_error)?;
            Ok(())
        })
    }
}
