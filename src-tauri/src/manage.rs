//! Manage services: details, appearance and notes, manual attacks, source restrictions,
//! cross-edition mode and the compatibility report.

use crate::attack_rows::MANUAL_PREFIX;
use crate::content_dto::{
    AffectedPick, AttackInput, CompatEntry, CompatibilityReport, MoveDirection, RestrictionPreview,
};
use crate::dto::{CharacterSnapshot, ElementRef};
use crate::error::{ApiError, ApiResult};
use crate::extras::{set_ui_extras, ui_extras, UiExtras};
use crate::ids::{ability_index, encode_slot, ui_type};
use crate::origin_details::{self, ALIGNMENT, DEITY};
use crate::session::{Ctx, Draft};
use crate::state::AppState;
use borealis_character::{AttackEdit, Character, ManualAttack};
use borealis_content::is_unrestrictable_source;
use borealis_engine::commands;
use borealis_engine::ids as engine_ids;
use borealis_engine::snapshot::AttackSnapshot;
use borealis_engine::state::CharacterState;
use serde_json::{Map, Value};

const MAX_TEXT_CHARS: usize = 20_000;
const MAX_MANUAL_ATTACKS: usize = 100;
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

/// Apply one `Partial<CharacterDetails>` patch to a draft. Alignment and deity select the
/// element they name in their slot, else keep the text as custom text (B-191, [`origin_details`]).
fn apply_details(ctx: &Ctx<'_>, d: &mut Draft, patch: &Map<String, Value>) -> ApiResult<()> {
    let mut extras = ui_extras(&d.character);
    for (key, value) in patch {
        let text = text_of(key, value)?;
        match key.as_str() {
            "alignment" => extras.alignment = origin_details::bind(ctx, d, ALIGNMENT, &text)?,
            "deity" => extras.deity = origin_details::bind(ctx, d, DEITY, &text)?,
            _ => apply_text(&mut d.character, &mut extras, key, text)?,
        }
    }
    set_ui_extras(&mut d.character, &extras);
    Ok(())
}

/// Store one plain-text detail.
fn apply_text(c: &mut Character, extras: &mut UiExtras, key: &str, text: String) -> ApiResult<()> {
    let d = &mut c.details;
    match key {
        "gender" => c.meta.gender = text,
        "playerName" => c.meta.player = text,
        // The native fields the Aurora importer fills and the sheet prints (B-218); the text an
        // older Manage kept in the UI extras goes, so emptying the box empties the sheet.
        "treasure" => {
            d.treasure = text;
            extras.treasure.clear();
        }
        "questItems" => {
            d.quest = text;
            extras.quest_items.clear();
        }
        "age" => d.appearance.age = text,
        "height" => d.appearance.height = text,
        "weight" => d.appearance.weight = text,
        "eyes" => d.appearance.eyes = text,
        "skin" => d.appearance.skin = text,
        "hair" => d.appearance.hair = text,
        "features" => d.additional_features = text,
        "backstory" => d.backstory = text,
        "personalityTraits" => d.personality.traits = text,
        "ideal" => d.personality.ideals = text,
        "bond" => d.personality.bonds = text,
        "flaw" => d.personality.flaws = text,
        "trinket" => d.personality.trinket = text,
        "notesLeft" => d.notes.left = text,
        "notesRight" => d.notes.right = text,
        "organization" => d.organization.name = text,
        "allies" => d.organization.allies = text,
        "combatNotes" => d.attacks_text = text,
        "sheetAcNote" => d.conditional_ac = text,
        other => {
            return Err(ApiError::invalid(format!(
                "Unknown detail field \"{other}\"."
            )))
        }
    }
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
    let calculated = input.calculated && !ability.is_empty();
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
        calculated,
    })
}

/// The engine's automatic row `uid` of `state`.
fn auto_line(ctx: &Ctx<'_>, state: &CharacterState, uid: &str) -> ApiResult<AttackSnapshot> {
    ctx.hub
        .derive(state)
        .attacks
        .iter()
        .find(|a| a.uid == uid)
        .cloned()
        .ok_or_else(|| ApiError::not_found("That attack is not in the list."))
}

/// `typed` when it differs from the worked-out value, else blank (keep it automatic). The value
/// worked out before an ability change (`was`) counts too: the editor may send it back untouched.
fn edited(typed: &str, now: &str, was: &str) -> String {
    let t = typed.trim();
    if t == now.trim() || t == was.trim() {
        String::new()
    } else {
        t.to_string()
    }
}

/// Save the attack editor's fields for an automatic row (Aurora's `AttackSectionItem`: a linked
/// ability for the automatic calculation, and fillable texts over the worked-out ones). A pick
/// that matches the rules' own choice is not kept, so the line still follows the rules later.
fn edit_auto_row(ctx: &Ctx<'_>, d: &mut Draft, uid: &str, input: &AttackInput) -> ApiResult<()> {
    let engine = &ctx.hub.engine;
    let was = auto_line(ctx, &d.character.build, uid)?;
    let line = &was;
    let picked = input.ability.as_deref().filter(|a| !a.is_empty());
    if let (false, Some(key)) = (line.ability_lines.is_empty(), picked) {
        let ability = ability_index(key)
            .map(|i| engine_ids::ABILITIES[i])
            .ok_or_else(|| ApiError::invalid("Unknown ability."))?;
        let rules = commands::set_attack_ability(engine, &d.character.build, uid, None)?.state;
        let by_rules = auto_line(ctx, &rules, uid)?.ability == ability;
        d.character.build = if by_rules {
            rules
        } else {
            commands::set_attack_ability(engine, &rules, uid, Some(ability))?.state
        };
    }
    let line = auto_line(ctx, &d.character.build, uid)?;
    let edit = AttackEdit {
        name: edited(&input.name, &line.name, &was.name),
        range: edited(&input.range, &line.range, &was.range),
        attack: edited(
            &input.attack_bonus,
            &format!("{:+}", line.attack),
            &format!("{:+}", was.attack),
        ),
        damage: edited(&input.damage, &line.damage, &was.damage),
        description: input.description.trim().to_string(),
        hidden: !input.show_in_combat,
    };
    let details = &mut d.character.details;
    if edit.is_empty() {
        details.auto_attack_edits.remove(uid);
    } else {
        details.auto_attack_edits.insert(uid.to_string(), edit);
    }
    if let Some(card) = input.as_card {
        details.auto_attack_cards.insert(uid.to_string(), card);
    }
    Ok(())
}

/// Take a listed weapon's row off the attacks list (a held weapon keeps its row).
fn unlist_auto_row(ctx: &Ctx<'_>, d: &mut Draft, uid: &str) -> ApiResult<()> {
    if !d
        .character
        .build
        .inventory
        .listed_attacks
        .iter()
        .any(|u| u == uid)
    {
        return Err(ApiError::requirement(
            "This attack comes from what the character holds or knows. Hide it from the sheet instead.",
        ));
    }
    d.character.build =
        commands::list_attack(&ctx.hub.engine, &d.character.build, uid, false)?.state;
    Ok(())
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
        self.mutate(id, |ctx, d| apply_details(ctx, d, map))
    }

    /// Add a manual attack (no `id`) or replace one; for an automatic row, save the user's
    /// changes to it (its ability, texts, sheet and card flags: GitHub #27).
    ///
    /// # Errors
    /// `invalid_argument` for a nameless manual attack or an unknown ability; `not_found` for an
    /// unknown automatic row; `requirement_failed` for an unknown manual row.
    pub fn upsert_attack(&self, id: &str, attack: &AttackInput) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            match attack.id.as_deref().filter(|i| !i.is_empty()) {
                Some(attack_id) if !attack_id.starts_with(MANUAL_PREFIX) => {
                    edit_auto_row(ctx, d, attack_id, attack)
                }
                Some(attack_id) => {
                    let manual = to_manual(attack)?;
                    let list = &mut d.character.details.attacks;
                    let at = manual_index(attack_id, list.len())?;
                    list[at] = manual;
                    Ok(())
                }
                None if d.character.details.attacks.len() >= MAX_MANUAL_ATTACKS => {
                    Err(ApiError::requirement("There are too many attacks."))
                }
                None => {
                    let manual = to_manual(attack)?;
                    d.character.details.attacks.push(manual);
                    Ok(())
                }
            }
        })
    }

    /// Delete a manual attack, or take a weapon added to the attacks (Aurora's "Add to Attacks &
    /// Spellcasting", GitHub #28) off the list again.
    ///
    /// # Errors
    /// `requirement_failed` for unknown attacks and automatic rows that are not on the list (a
    /// held weapon, the Unarmed Strike, a cantrip): those are hidden from the sheet instead.
    pub fn remove_attack(&self, id: &str, attack_id: &str) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            if !attack_id.starts_with(MANUAL_PREFIX) {
                return unlist_auto_row(ctx, d, attack_id);
            }
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
            // the next row the user sees: an item-linked Aurora row is shown as its weapon's
            // automatic row, not among these
            let shown = |n: &usize| list[*n].item_uid.is_none();
            let to = match direction {
                MoveDirection::Up => (0..at).rev().find(shown),
                MoveDirection::Down => (at + 1..list.len()).find(shown),
            };
            if let Some(to) = to {
                list.swap(at, to);
            }
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
            let restricted =
                borealis_content::SourceRestriction::excluding(restricted_from(ctx.hub, enabled));
            let snap = ctx.hub.engine.derive_with(&session.character.build, false);
            let affected = snap
                .slots
                .iter()
                .filter_map(|slot| {
                    let chosen = slot.chosen.as_ref()?;
                    restricted
                        .is_restricted_element(&chosen.source, &chosen.ty)
                        .then(|| AffectedPick {
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
