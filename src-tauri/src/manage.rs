//! Manage services: details, appearance and notes, manual attacks, companion, source restrictions,
//! cross-edition mode and the compatibility report.

use crate::content_dto::{
    AffectedPick, AttackInput, CompatEntry, CompatibilityReport, MoveDirection, RestrictionPreview,
};
use crate::dto::{CharacterSnapshot, ElementRef};
use crate::error::{ApiError, ApiResult};
use crate::extras::{portrait_from_data_url, set_ui_extras, ui_extras};
use crate::ids::{ability_index, encode_slot, ui_type};
use crate::session::Draft;
use crate::state::AppState;
use borealis_character::ManualAttack;
use borealis_content::is_unrestrictable_source;
use serde_json::{Map, Value};

const MAX_TEXT_CHARS: usize = 20_000;
const MAX_MANUAL_ATTACKS: usize = 100;
const MANUAL_PREFIX: &str = "manual-";
const ABILITY_NAMES: [&str; 6] = [
    "Strength",
    "Dexterity",
    "Constitution",
    "Intelligence",
    "Wisdom",
    "Charisma",
];

fn text_of(key: &str, value: &Value) -> ApiResult<String> {
    let s = value
        .as_str()
        .ok_or_else(|| ApiError::invalid(format!("\"{key}\" must be text.")))?;
    if s.chars().count() > MAX_TEXT_CHARS {
        return Err(ApiError::invalid(format!("\"{key}\" is too long.")));
    }
    Ok(s.to_string())
}

/// Apply one `Partial<CharacterDetails>` patch to a draft.
fn apply_details(d: &mut Draft, patch: &Map<String, Value>) -> ApiResult<()> {
    let mut extras = ui_extras(&d.character);
    let c = &mut d.character;
    for (key, value) in patch {
        let text = text_of(key, value)?;
        match key.as_str() {
            "gender" => c.meta.gender = text,
            "playerName" => c.meta.player = text,
            "alignment" => extras.alignment = text,
            "deity" => extras.deity = text,
            "treasure" => extras.treasure = text,
            "questItems" => extras.quest_items = text,
            "age" => c.details.appearance.age = text,
            "height" => c.details.appearance.height = text,
            "weight" => c.details.appearance.weight = text,
            "eyes" => c.details.appearance.eyes = text,
            "skin" => c.details.appearance.skin = text,
            "hair" => c.details.appearance.hair = text,
            "features" => c.details.additional_features = text,
            "backstory" => c.details.backstory = text,
            "personalityTraits" => c.details.personality.traits = text,
            "ideal" => c.details.personality.ideals = text,
            "bond" => c.details.personality.bonds = text,
            "flaw" => c.details.personality.flaws = text,
            "trinket" => c.details.personality.trinket = text,
            "notesLeft" => c.details.notes.left = text,
            "notesRight" => c.details.notes.right = text,
            "organization" => c.details.organization.name = text,
            "allies" => c.details.organization.allies = text,
            "combatNotes" => c.details.attacks_text = text,
            "sheetAcNote" => c.details.conditional_ac = text,
            other => {
                return Err(ApiError::invalid(format!(
                    "Unknown detail field \"{other}\"."
                )))
            }
        }
    }
    set_ui_extras(c, &extras);
    Ok(())
}

fn manual_index(id: &str, len: usize) -> ApiResult<usize> {
    id.strip_prefix(MANUAL_PREFIX)
        .and_then(|n| n.parse::<usize>().ok())
        .filter(|i| *i < len)
        .ok_or_else(|| ApiError::requirement("Only attacks you added yourself can be edited."))
}

fn to_manual(input: &AttackInput) -> ApiResult<ManualAttack> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(ApiError::invalid("An attack needs a name."));
    }
    let ability = match input.ability.as_deref().filter(|a| !a.is_empty()) {
        None => String::new(),
        Some(a) => ability_index(a)
            .map(|i| ABILITY_NAMES[i].to_string())
            .ok_or_else(|| ApiError::invalid("Unknown ability."))?,
    };
    Ok(ManualAttack {
        item_uid: None,
        name: name.to_string(),
        range: input.range.clone(),
        attack: input.attack_bonus.clone(),
        damage: input.damage.clone(),
        displayed: input.show_in_combat,
        ability,
        description: input.description.clone(),
        as_card: input.as_card,
    })
}

impl AppState {
    /// Merge a partial details patch (identity, appearance, personality, notes).
    ///
    /// # Errors
    /// `invalid_argument` for unknown fields, non-text values or overlong text.
    pub fn update_details(&self, id: &str, patch: &Value) -> ApiResult<CharacterSnapshot> {
        let map = patch
            .as_object()
            .ok_or_else(|| ApiError::invalid("The details patch must be an object."))?;
        self.mutate(id, |_, d| apply_details(d, map))
    }

    /// Add a manual attack (no `id`) or replace one.
    ///
    /// # Errors
    /// `invalid_argument` for a nameless attack; `requirement_failed` for automatic attacks.
    pub fn upsert_attack(&self, id: &str, attack: &AttackInput) -> ApiResult<CharacterSnapshot> {
        let manual = to_manual(attack)?;
        self.mutate(id, |_, d| {
            let list = &mut d.character.details.attacks;
            match attack.id.as_deref().filter(|i| !i.is_empty()) {
                Some(attack_id) if !attack_id.starts_with(MANUAL_PREFIX) => {
                    // an automatic row: only its card flag can change
                    let card = attack.as_card.ok_or_else(|| {
                        ApiError::requirement("Only attacks you added yourself can be edited.")
                    })?;
                    d.character
                        .details
                        .auto_attack_cards
                        .insert(attack_id.to_string(), card);
                }
                Some(attack_id) => {
                    let at = manual_index(attack_id, list.len())?;
                    list[at] = manual;
                }
                None if list.len() >= MAX_MANUAL_ATTACKS => {
                    return Err(ApiError::requirement("There are too many attacks."));
                }
                None => list.push(manual),
            }
            Ok(())
        })
    }

    /// Delete a manual attack.
    ///
    /// # Errors
    /// `requirement_failed` for automatic or unknown attacks.
    pub fn remove_attack(&self, id: &str, attack_id: &str) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |_, d| {
            let list = &mut d.character.details.attacks;
            let at = manual_index(attack_id, list.len())?;
            list.remove(at);
            Ok(())
        })
    }

    /// Move a manual attack one place up or down among the manual attacks.
    ///
    /// # Errors
    /// `requirement_failed` for automatic or unknown attacks.
    pub fn move_attack(
        &self,
        id: &str,
        attack_id: &str,
        direction: MoveDirection,
    ) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |_, d| {
            let list = &mut d.character.details.attacks;
            let at = manual_index(attack_id, list.len())?;
            let to = match direction {
                MoveDirection::Up => at.checked_sub(1),
                MoveDirection::Down => Some(at + 1).filter(|n| *n < list.len()),
            };
            if let Some(to) = to {
                list.swap(at, to);
            }
            Ok(())
        })
    }

    /// Set the companion's name and/or portrait (`null` clears the portrait).
    ///
    /// # Errors
    /// `invalid_argument` for bad names or images.
    pub fn update_companion(&self, id: &str, patch: &Value) -> ApiResult<CharacterSnapshot> {
        let map = patch
            .as_object()
            .ok_or_else(|| ApiError::invalid("The companion patch must be an object."))?;
        self.mutate(id, |_, d| {
            let mut extras = ui_extras(&d.character);
            for (key, value) in map {
                match (key.as_str(), value) {
                    ("name", v) => {
                        let name = text_of("name", v)?;
                        d.character.details.companion_name = name.trim().to_string();
                    }
                    ("portrait", Value::Null) => extras.companion_portrait = None,
                    ("portrait", Value::String(url)) => {
                        portrait_from_data_url(url).map_err(ApiError::invalid)?;
                        extras.companion_portrait = Some(url.clone());
                    }
                    (other, _) => {
                        return Err(ApiError::invalid(format!(
                            "Unknown companion field \"{other}\"."
                        )));
                    }
                }
            }
            set_ui_extras(&mut d.character, &extras);
            Ok(())
        })
    }

    /// Which current picks come from a source the restriction would switch off (nothing is changed).
    /// The engine never unregisters a stored pick: they stay, and only option lists narrow.
    ///
    /// # Errors
    /// `not_found` if the character is not open.
    pub fn preview_source_restrictions(
        &self,
        id: &str,
        enabled: Option<&[String]>,
    ) -> ApiResult<RestrictionPreview> {
        self.read(id, |ctx, session| {
            let restricted = restricted_from(ctx.hub, enabled);
            let snap = ctx.hub.engine.derive_with(&session.character.build, false);
            let affected = snap
                .slots
                .iter()
                .filter_map(|slot| {
                    let chosen = slot.chosen.as_ref()?;
                    restricted.contains(&chosen.source).then(|| AffectedPick {
                        slot_id: encode_slot(&slot.key),
                        slot_title: slot.name.clone(),
                        element: chosen.name.clone(),
                    })
                })
                .collect();
            Ok(RestrictionPreview { affected })
        })
    }

    /// Restrict the character to `enabled` sources (`None` lifts the restriction); stored picks stay.
    ///
    /// # Errors
    /// `not_found` if the character is not open.
    pub fn set_source_restrictions(
        &self,
        id: &str,
        enabled: Option<&[String]>,
    ) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            d.character.build.restricted_sources = restricted_from(ctx.hub, enabled);
            Ok(())
        })
    }

    /// Allow (`Some(true)`), forbid (`Some(false)`) or reset to the ruleset default (`None`)
    /// foreign-edition content for new picks. Stored choices are never unregistered.
    ///
    /// # Errors
    /// `not_found` if the character is not open.
    pub fn set_allow_cross_edition(
        &self,
        id: &str,
        allow: Option<bool>,
    ) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |_, d| {
            d.character.build.allow_cross_edition = allow;
            Ok(())
        })
    }

    /// The foreign-edition elements in use and whether strict mode is on.
    ///
    /// # Errors
    /// `not_found` if the character is not open.
    pub fn get_compatibility_report(&self, id: &str) -> ApiResult<CompatibilityReport> {
        self.read(id, |ctx, session| {
            let report = ctx
                .hub
                .engine
                .derive_with(&session.character.build, false)
                .compatibility;
            Ok(CompatibilityReport {
                strict: report.strict,
                entries: report
                    .entries
                    .into_iter()
                    .map(|e| CompatEntry {
                        element: ElementRef {
                            id: e.element.id,
                            name: e.element.name,
                            ty: ui_type(&e.element.ty).to_string(),
                            source: e.element.source,
                        },
                        from: e.from,
                        converted: e.converted,
                        notes: e.notes,
                    })
                    .collect(),
            })
        })
    }
}

fn restricted_from(hub: &crate::state::ContentHub, enabled: Option<&[String]>) -> Vec<String> {
    enabled.map_or_else(Vec::new, |on| {
        hub.store
            .source_names()
            .into_iter()
            .filter(|s| !on.iter().any(|e| e == s) && !is_unrestrictable_source(s))
            .map(str::to_string)
            .collect()
    })
}
