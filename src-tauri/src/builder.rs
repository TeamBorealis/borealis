//! Builder services: creation, selections, levels, abilities, options and the ruleset profile.

use crate::dto::{
    AbilityMethodDto, CharacterSnapshot, ElementSummary, NewCharacterRequest, OptionsPatch, Page,
    RollRecord, Ruleset, RulesetProfileDto, SlotOption, Terminology,
};
use crate::elements::summary;
use crate::error::{ApiError, ApiResult};
use crate::extras::{array_assignment, set_ui_extras, ui_extras};
use crate::ids::{ability_index, decode_class_slot, decode_slot, ui_type, ABILITY_KEYS};
use crate::session::{fill_locked, new_id, Ctx, Draft};
use crate::state::AppState;
use borealis_character::Character;
use borealis_engine::commands::{self, CommandOutcome};
use borealis_engine::ids as engine_ids;
use borealis_engine::profile::{profile_for, AbilityScoreSource};
use borealis_engine::state::AbilityMethod;
use borealis_engine::{CharacterState, Choice, ClassSlot};

const MAX_LEVEL: u32 = 20;
const DEFAULT_PAGE: usize = 200;
const MAX_PAGE: usize = 1000;
const MAX_XP: i64 = 10_000_000;
const DEFAULT_GROUP: &str = "Characters";
const MAX_NAME_CHARS: usize = 120;

/// Apply a command outcome to a draft.
pub(crate) fn apply(d: &mut Draft, outcome: CommandOutcome) {
    d.character.build = outcome.state;
    d.warnings.extend(outcome.warnings);
}

/// Apply a command result and then select forced (locked, single-candidate) slots.
fn apply_filled(ctx: &Ctx<'_>, d: &mut Draft, outcome: CommandOutcome) {
    apply(d, outcome);
    d.character.build = fill_locked(ctx.hub, &d.character.build);
}

fn ability_of(key: &str) -> ApiResult<usize> {
    ability_index(key).ok_or_else(|| ApiError::invalid("Unknown ability."))
}

fn apply_options(ctx: &Ctx<'_>, d: &mut Draft, patch: &OptionsPatch) -> ApiResult<()> {
    let engine = &ctx.hub.engine;
    let toggles = [
        (engine_ids::OPT_MULTICLASSING, patch.multiclassing),
        (engine_ids::OPT_FEATS, patch.feats),
        (engine_ids::OPT_AVERAGE_HP, patch.average_hp),
    ];
    for (id, value) in toggles {
        if let Some(on) = value {
            let out = commands::set_option(engine, &d.character.build, id, on)?;
            apply(d, out);
        }
    }
    for (id, on) in patch.custom.iter().flatten() {
        let out = commands::set_option(engine, &d.character.build, id, *on)?;
        apply(d, out);
    }
    if let Some(limit) = patch.attunement_limit {
        d.character.build.house_rules.attunement_limit = (limit > 0).then_some(limit);
    }
    if let Some(unrestricted) = patch.point_buy_unrestricted {
        let mut extras = ui_extras(&d.character);
        extras.point_buy_unrestricted = unrestricted;
        set_ui_extras(&mut d.character, &extras);
    }
    Ok(())
}

impl AppState {
    /// Create a character of `ruleset`, save it into the library and open it.
    ///
    /// # Errors
    /// `invalid_argument` for a bad name, ruleset or level; `io_error` if saving fails.
    pub fn create_character(
        &self,
        ruleset: Ruleset,
        request: &NewCharacterRequest,
    ) -> ApiResult<CharacterSnapshot> {
        let name = request.name.trim();
        if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
            return Err(ApiError::invalid(
                "A character name must be 1 to 120 characters.",
            ));
        }
        if ruleset == Ruleset::Any {
            return Err(ApiError::invalid("Choose the 2014 or the 2024 rules."));
        }
        if !(1..=MAX_LEVEL).contains(&request.starting_level) {
            return Err(ApiError::invalid("The starting level must be 1 to 20."));
        }
        let hub = self.hub()?;
        let settings = self.settings()?;
        let engine = &hub.engine;
        let mut state = commands::new_character(engine, ruleset);
        state = commands::set_ability_method(&state, request.ability_method.into());
        let mut character = Character::with_ruleset(name, ruleset);
        character.build = state;
        character.meta.gender.clone_from(&request.gender);
        character.meta.player.clone_from(&settings.player_name);
        character.meta.group = request
            .group
            .as_deref()
            .map(str::trim)
            .filter(|g| !g.is_empty())
            .unwrap_or(DEFAULT_GROUP)
            .to_string();
        if let Some(url) = &request.portrait {
            character.portrait =
                Some(crate::extras::portrait_from_data_url(url).map_err(ApiError::invalid)?);
        }
        let ctx = Ctx {
            hub: &hub,
            settings: &settings,
            app: self,
        };
        let mut draft = Draft {
            character,
            rolls: Vec::new(),
            warnings: Vec::new(),
        };
        apply_options(&ctx, &mut draft, &request.options)?;
        for _ in 1..request.starting_level {
            let out = commands::level_up_main(engine, &draft.character.build, None)?;
            apply(&mut draft, out);
        }
        draft.character.build = fill_locked(ctx.hub, &draft.character.build);
        let id = new_id();
        let path = self.character_path(&id)?;
        draft.character.save(&path)?;
        self.insert_session(&id, draft.character, path, false)
    }

    /// Valid candidates of a slot, filtered by name and paged.
    ///
    /// # Errors
    /// `not_found` if the character or slot does not exist.
    pub fn get_slot_options(
        &self,
        id: &str,
        slot_id: &str,
        query: Option<&str>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> ApiResult<Page<SlotOption>> {
        let key = decode_slot(slot_id)?;
        self.read(id, |ctx, session| {
            let snap = ctx.hub.derive(&session.character.build);
            let slot = snap
                .slots
                .iter()
                .find(|s| s.key == key && s.rejected.is_none())
                .ok_or_else(|| ApiError::not_found("That selection slot does not exist."))?;
            let needle = query
                .map(str::trim)
                .filter(|q| !q.is_empty())
                .map(str::to_lowercase);
            let matches = |name: &str| {
                needle
                    .as_ref()
                    .map_or(true, |n| name.to_lowercase().contains(n))
            };
            let rows: Vec<SlotOption> = if slot.ty == "List" {
                slot.list_items
                    .iter()
                    .filter(|(_, text)| matches(text))
                    .map(|(item, text)| list_option(*item, text, &slot.owner.source))
                    .collect()
            } else {
                slot.options
                    .iter()
                    .filter(|o| matches(&o.name))
                    .filter_map(|o| ctx.hub.element(&o.id))
                    .map(|el| SlotOption {
                        summary: summary(&ctx.hub.store, el),
                        disabled_reason: None,
                    })
                    .collect()
            };
            let total = rows.len();
            let take = limit.unwrap_or(DEFAULT_PAGE).min(MAX_PAGE);
            let items = rows
                .into_iter()
                .skip(offset.unwrap_or(0))
                .take(take)
                .collect();
            Ok(Page { total, items })
        })
    }

    /// Put an element into a slot (re-validated by the engine).
    ///
    /// # Errors
    /// `not_found`, or `requirement_failed` if the element is not a valid choice any more.
    pub fn apply_selection(
        &self,
        id: &str,
        slot_id: &str,
        element_id: &str,
    ) -> ApiResult<CharacterSnapshot> {
        let key = decode_slot(slot_id)?;
        self.mutate(id, |ctx, d| {
            let engine = &ctx.hub.engine;
            let snap = ctx.hub.derive(&d.character.build);
            let slot = snap
                .slots
                .iter()
                .find(|s| s.key == key)
                .ok_or_else(|| ApiError::not_found("That selection slot does not exist."))?;
            let choice = if slot.ty == "List" {
                let item = element_id
                    .parse()
                    .map_err(|_| ApiError::invalid("Malformed list choice."))?;
                Choice::ListItem(item)
            } else {
                Choice::Element(element_id.to_string())
            };
            let out = commands::select(engine, &d.character.build, &key, choice)?;
            apply_filled(ctx, d, out);
            Ok(())
        })
    }

    /// Empty a slot.
    ///
    /// # Errors
    /// `not_found` if the character does not exist.
    pub fn clear_selection(&self, id: &str, slot_id: &str) -> ApiResult<CharacterSnapshot> {
        let key = decode_slot(slot_id)?;
        self.mutate(id, |ctx, d| {
            let out = commands::clear(&ctx.hub.engine, &d.character.build, &key)?;
            apply(d, out);
            Ok(())
        })
    }

    /// Set experience points (never levels up by itself).
    ///
    /// # Errors
    /// `invalid_argument` for negative or absurd XP.
    pub fn set_xp(&self, id: &str, xp: i64) -> ApiResult<CharacterSnapshot> {
        if !(0..=MAX_XP).contains(&xp) {
            return Err(ApiError::invalid(
                "Experience must be between 0 and 10,000,000.",
            ));
        }
        let xp = u32::try_from(xp).map_err(|_| ApiError::invalid("Invalid experience value."))?;
        self.mutate(id, |_, d| {
            d.character.build = commands::set_experience(&d.character.build, xp);
            Ok(())
        })
    }

    /// Add a level to the given class progression.
    ///
    /// # Errors
    /// `requirement_failed` at level 20 or for an unknown multiclass.
    pub fn level_up(&self, id: &str, class_slot_id: &str) -> ApiResult<CharacterSnapshot> {
        let slot = decode_class_slot(class_slot_id)?;
        self.mutate(id, |ctx, d| {
            let engine = &ctx.hub.engine;
            let state = &d.character.build;
            let out = match slot {
                ClassSlot::Main => commands::level_up_main(engine, state, None)?,
                ClassSlot::Multi { starting_level } => {
                    commands::level_up_multiclass(engine, state, starting_level, None)?
                }
            };
            apply_filled(ctx, d, out);
            Ok(())
        })
    }

    /// Remove the most recent level.
    ///
    /// # Errors
    /// `requirement_failed` at level 1.
    pub fn level_down(&self, id: &str) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let out = commands::level_down(&ctx.hub.engine, &d.character.build)?;
            apply(d, out);
            Ok(())
        })
    }

    /// Start a new multiclass (adds one level in it).
    ///
    /// # Errors
    /// `requirement_failed` if multiclassing is unavailable.
    pub fn add_multiclass(&self, id: &str) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let out = commands::level_up_new_multiclass(&ctx.hub.engine, &d.character.build, None)?;
            apply_filled(ctx, d, out);
            Ok(())
        })
    }

    /// Record a hit point roll for a class level.
    ///
    /// # Errors
    /// `invalid_argument` if the value is outside the hit die.
    pub fn set_hp_roll(
        &self,
        id: &str,
        class_slot_id: &str,
        level: u32,
        value: u32,
    ) -> ApiResult<CharacterSnapshot> {
        let slot = decode_class_slot(class_slot_id)?;
        self.mutate(id, |ctx, d| {
            let snap = ctx.hub.engine.derive_with(&d.character.build, false);
            let class = snap
                .classes
                .iter()
                .find(|c| c.slot == slot)
                .ok_or_else(|| ApiError::not_found("That class progression does not exist."))?;
            if level == 0 || level > class.level {
                return Err(ApiError::invalid("The character has no such class level."));
            }
            let die = class.hit_die.unwrap_or(0);
            if value == 0 || value > die {
                return Err(ApiError::invalid(format!(
                    "A hit point roll must be between 1 and {die}."
                )));
            }
            let out = commands::set_hp_roll(&d.character.build, &slot, level, value)?;
            apply(d, out);
            Ok(())
        })
    }

    /// Switch the ability generation method (resets the base scores where the method defines them).
    ///
    /// # Errors
    /// `not_found` if the character is not open.
    pub fn set_ability_method(
        &self,
        id: &str,
        method: AbilityMethodDto,
    ) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |_, d| {
            d.character.build = commands::set_ability_method(&d.character.build, method.into());
            d.rolls.clear();
            clear_array_assignment(d);
            Ok(())
        })
    }

    /// Set one base score, enforcing the active method's rules.
    ///
    /// # Errors
    /// `budget_exceeded` over the point buy budget, `invalid_argument` outside the allowed range.
    pub fn set_base_score(
        &self,
        id: &str,
        ability: &str,
        value: i32,
    ) -> ApiResult<CharacterSnapshot> {
        let index = ability_of(ability)?;
        self.mutate(id, |ctx, d| {
            let state = &d.character.build;
            match state.abilities.method {
                AbilityMethod::PointBuy => {
                    let (lo, hi) = profile_for(state.ruleset)
                        .ability_generation()
                        .point_buy_range;
                    if !ui_extras(&d.character).point_buy_unrestricted
                        && !(lo..=hi).contains(&value)
                    {
                        return Err(ApiError::invalid(format!(
                            "Point buy scores must be between {lo} and {hi}."
                        )));
                    }
                }
                AbilityMethod::StandardArray => {
                    return assign_array(d, index, Some(value));
                }
                _ => {}
            }
            let out = commands::set_base_score(&ctx.hub.engine, state, index, value)?;
            apply(d, out);
            Ok(())
        })
    }

    /// Roll all abilities, or one, with the backend RNG.
    ///
    /// # Errors
    /// `requirement_failed` unless the method is a rolling one.
    pub fn roll_abilities(&self, id: &str, ability: Option<&str>) -> ApiResult<CharacterSnapshot> {
        let target = ability.map(ability_of).transpose()?;
        self.mutate(id, |ctx, d| {
            let method = d.character.build.abilities.method;
            let dice_count = match method {
                AbilityMethod::Roll3d6 => 3,
                AbilityMethod::Roll4d6 => 4,
                _ => {
                    return Err(ApiError::requirement(
                        "Rolling is only available for the 3d6 and 4d6 methods.",
                    ))
                }
            };
            let indices: Vec<usize> = target.map_or_else(|| (0..6).collect(), |i| vec![i]);
            for i in indices {
                let mut dice = Vec::new();
                for _ in 0..dice_count {
                    dice.push(ctx.app.roll_die(6)?);
                }
                let total = if method == AbilityMethod::Roll4d6 {
                    dice.iter().sum::<u32>() - dice.iter().min().copied().unwrap_or(0)
                } else {
                    dice.iter().sum()
                };
                let score =
                    i32::try_from(total).map_err(|_| ApiError::internal("roll overflow"))?;
                let out = commands::set_base_score(&ctx.hub.engine, &d.character.build, i, score)?;
                apply(d, out);
                d.rolls.push(RollRecord {
                    ability: ABILITY_KEYS[i].to_string(),
                    dice,
                    total,
                });
            }
            Ok(())
        })
    }

    /// Assign (or swap) a standard array value; `None` unassigns the ability.
    ///
    /// # Errors
    /// `requirement_failed` if the method is not the standard array.
    pub fn assign_standard_array(
        &self,
        id: &str,
        ability: &str,
        value: Option<i32>,
    ) -> ApiResult<CharacterSnapshot> {
        let index = ability_of(ability)?;
        self.mutate(id, |_, d| assign_array(d, index, value))
    }

    /// Reset the base scores to the method's starting point.
    ///
    /// # Errors
    /// `not_found` if the character is not open.
    pub fn reset_abilities(&self, id: &str) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |_, d| {
            let method = d.character.build.abilities.method;
            let mut next = commands::set_ability_method(&d.character.build, method);
            if !matches!(
                method,
                AbilityMethod::PointBuy | AbilityMethod::StandardArray
            ) {
                next.abilities.base = [10; 6];
            }
            d.character.build = next;
            d.rolls.clear();
            clear_array_assignment(d);
            Ok(())
        })
    }

    /// Update character options (multiclassing, feats, average HP, house rules, custom options).
    ///
    /// # Errors
    /// `not_found` for an unknown custom option.
    pub fn set_character_options(
        &self,
        id: &str,
        patch: &OptionsPatch,
    ) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| apply_options(ctx, d, patch))
    }
}

fn clear_array_assignment(d: &mut Draft) {
    let mut extras = ui_extras(&d.character);
    if extras.array_assignment.take().is_some() {
        set_ui_extras(&mut d.character, &extras);
    }
}

fn assign_array(d: &mut Draft, index: usize, value: Option<i32>) -> ApiResult<()> {
    let state: &CharacterState = &d.character.build;
    if state.abilities.method != AbilityMethod::StandardArray {
        return Err(ApiError::requirement(
            "The standard array method is not active.",
        ));
    }
    let array = profile_for(state.ruleset)
        .ability_generation()
        .standard_array;
    let low = array.iter().copied().min().unwrap_or(8);
    let mut assigned = array_assignment(&d.character, array);
    if let Some(v) = value {
        if !array.contains(&v) {
            return Err(ApiError::invalid(format!(
                "{v} is not a standard array value."
            )));
        }
        let previous = assigned[index];
        if let Some(holder) = assigned.iter().position(|a| *a == Some(v)) {
            assigned[holder] = previous;
        }
        assigned[index] = Some(v);
    } else {
        assigned[index] = None;
    }
    let mut next = state.clone();
    for (slot, a) in next.abilities.base.iter_mut().zip(assigned) {
        *slot = a.unwrap_or(low);
    }
    d.character.build = next;
    let mut extras = ui_extras(&d.character);
    extras.array_assignment = Some(assigned.to_vec());
    set_ui_extras(&mut d.character, &extras);
    Ok(())
}

fn list_option(item: i64, text: &str, source: &str) -> SlotOption {
    SlotOption {
        summary: ElementSummary {
            id: item.to_string(),
            name: text.to_string(),
            ty: ui_type("List").to_string(),
            source: source.to_string(),
            source_name: source.to_string(),
            short_description: String::new(),
            prerequisites: None,
            rulesets: vec![Ruleset::Dnd2014, Ruleset::Dnd2024],
            level: None,
            school: None,
            spell: None,
            armor: None,
            weapon: None,
        },
        disabled_reason: None,
    }
}

/// Edition profile for the UI.
#[must_use]
pub fn ruleset_profile(ruleset: Ruleset) -> RulesetProfileDto {
    let ruleset = if ruleset == Ruleset::Any {
        Ruleset::Dnd2014
    } else {
        ruleset
    };
    let p = profile_for(ruleset);
    let modern = ruleset == Ruleset::Dnd2024;
    let (species, armor, inspiration, srd) = if modern {
        (
            "Species",
            "Armor Training",
            "Heroic Inspiration",
            "SRD 5.2.1",
        )
    } else {
        ("Race", "Armor Proficiency", "Inspiration", "SRD 5.1")
    };
    RulesetProfileDto {
        id: ruleset,
        label: p.label().to_string(),
        srd_source: srd.to_string(),
        terminology: Terminology {
            species: species.into(),
            armor_training: armor.into(),
            inspiration: inspiration.into(),
        },
        ability_methods: vec![
            AbilityMethodDto::PointBuy,
            AbilityMethodDto::StandardArray,
            AbilityMethodDto::Roll4d6,
            AbilityMethodDto::Roll3d6,
            AbilityMethodDto::Manual,
        ],
        ability_cap: 20,
        point_buy_budget: p.ability_generation().point_buy_budget,
        weapon_mastery: p.weapon_mastery(),
        origin_feats: p.feat_at_level_1(),
        background_asi: p
            .ability_score_sources()
            .contains(&AbilityScoreSource::Background),
    }
}
