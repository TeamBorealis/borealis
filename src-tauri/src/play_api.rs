//! 2024 play-state services (resources, rests, toggles, exhaustion, inspiration, masteries,
//! Wild Shape forms, equipment packages) and "Convert to 2024".

use crate::builder::apply;
use crate::dto::CharacterSnapshot;
use crate::error::{ApiError, ApiResult};
use crate::play_dto::{conversion_preview, ConversionApplied, ConversionPreviewDto};
use crate::session::new_id;
use crate::state::AppState;
use borealis_character::{apply_conversion, convert_to_2024_with, Character, ConvertError};
use borealis_engine::commands;
use borealis_engine::{Choice, SlotKey};

/// Weapon Mastery slots are `Weapon` selects named after the feature.
fn is_mastery_slot(ty: &str, name: &str) -> bool {
    ty == "Weapon" && name.to_lowercase().contains("mastery")
}

impl AppState {
    /// Expend `uses` of a limited-use resource.
    ///
    /// # Errors
    /// `not_found` for an unknown resource, `requirement_failed` when too few uses are left.
    pub fn spend_resource(
        &self,
        id: &str,
        resource: &str,
        uses: u32,
    ) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let out =
                commands::spend_resource(&ctx.hub.engine, &d.character.build, resource, uses)?;
            apply(d, out);
            Ok(())
        })
    }

    /// Give back `uses` of a resource (capped at its maximum).
    ///
    /// # Errors
    /// `not_found` for an unknown resource.
    pub fn restore_resource(
        &self,
        id: &str,
        resource: &str,
        uses: u32,
    ) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let out =
                commands::restore_resource(&ctx.hub.engine, &d.character.build, resource, uses)?;
            apply(d, out);
            Ok(())
        })
    }

    /// Take a Short Rest.
    ///
    /// # Errors
    /// Never in practice; the signature matches the other services.
    pub fn short_rest(&self, id: &str) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let out = commands::short_rest(&ctx.hub.engine, &d.character.build)?;
            apply(d, out);
            Ok(())
        })
    }

    /// Take a Long Rest (resources refill, toggles end, exhaustion drops by 1 in 2024).
    ///
    /// # Errors
    /// Never in practice.
    pub fn long_rest(&self, id: &str) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let out = commands::long_rest(&ctx.hub.engine, &d.character.build)?;
            apply(d, out);
            Ok(())
        })
    }

    /// Switch a toggle (Rage, ...) on or off.
    ///
    /// # Errors
    /// `not_found` for an unknown toggle.
    pub fn set_toggle(&self, id: &str, toggle: &str, on: bool) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let out = commands::set_toggle(&ctx.hub.engine, &d.character.build, toggle, on)?;
            apply(d, out);
            Ok(())
        })
    }

    /// Set the exhaustion level.
    ///
    /// # Errors
    /// `requirement_failed` above the maximum level.
    pub fn set_exhaustion(&self, id: &str, level: u32) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let out = commands::set_exhaustion(&ctx.hub.engine, &d.character.build, level)?;
            apply(d, out);
            Ok(())
        })
    }

    /// Give or take Heroic Inspiration.
    ///
    /// # Errors
    /// Never in practice.
    pub fn set_heroic_inspiration(&self, id: &str, has: bool) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |_, d| {
            apply(d, commands::set_inspiration(&d.character.build, has));
            Ok(())
        })
    }

    /// Replace all Weapon Mastery picks with `weapon_ids` (in slot order; fewer picks leave slots
    /// open).
    ///
    /// # Errors
    /// `requirement_failed` when the character has no mastery slots, more weapons are named than
    /// there are slots, or a weapon does not qualify; `invalid_argument` for duplicates.
    pub fn choose_weapon_masteries(
        &self,
        id: &str,
        weapon_ids: &[String],
    ) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let engine = &ctx.hub.engine;
            let snap = engine.derive(&d.character.build);
            let keys: Vec<SlotKey> = snap
                .slots
                .iter()
                .filter(|s| is_mastery_slot(&s.ty, &s.name))
                .map(|s| s.key.clone())
                .collect();
            if keys.is_empty() {
                return Err(ApiError::requirement(
                    "This character has no Weapon Mastery to choose.",
                ));
            }
            if weapon_ids.len() > keys.len() {
                return Err(ApiError::requirement(format!(
                    "Only {} weapons can be mastered at this level.",
                    keys.len()
                )));
            }
            let mut seen = std::collections::BTreeSet::new();
            if !weapon_ids.iter().all(|w| seen.insert(w)) {
                return Err(ApiError::invalid("Each weapon can be chosen once."));
            }
            let mut state = d.character.build.clone();
            for key in &keys {
                state = commands::clear(engine, &state, key)?.state;
            }
            for (key, weapon) in keys.iter().zip(weapon_ids) {
                let out = commands::select(engine, &state, key, Choice::Element(weapon.clone()))?;
                state = out.state;
            }
            d.character.build = state;
            Ok(())
        })
    }

    /// Replace the learned Wild Shape forms.
    ///
    /// # Errors
    /// `requirement_failed` without Wild Shape, with too many forms, or for a Beast that does not
    /// fit the limits.
    pub fn set_wild_shape_forms(&self, id: &str, forms: &[String]) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let out = commands::set_wild_shape_forms(&ctx.hub.engine, &d.character.build, forms)?;
            apply(d, out);
            Ok(())
        })
    }

    /// Take a starting-equipment option of a class or background package.
    ///
    /// # Errors
    /// `not_found` for an unknown package, `requirement_failed` when already taken.
    pub fn choose_equipment_package(
        &self,
        id: &str,
        package: &str,
        option: &str,
    ) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let out = commands::choose_equipment_package(
                &ctx.hub.engine,
                &d.character.build,
                package,
                option,
            )?;
            apply(d, out);
            Ok(())
        })
    }

    fn source_character(&self, id: &str) -> ApiResult<Character> {
        match self.read(id, |_, s| Ok(s.character.clone())) {
            Ok(c) => Ok(c),
            Err(_) => {
                let path = self.character_path(id)?;
                if !path.is_file() {
                    return Err(ApiError::not_found("That character no longer exists."));
                }
                Ok(Character::load(&path)?)
            }
        }
    }

    /// What converting a 2014 character to 2024 would keep, map, drop or ask for. Nothing is saved.
    ///
    /// # Errors
    /// `not_found`, or `requirement_failed` when the character is not 2014.
    pub fn preview_convert_to_2024(&self, id: &str) -> ApiResult<ConversionPreviewDto> {
        let source = self.source_character(id)?;
        let hub = self.hub()?;
        let preview =
            convert_to_2024_with(&hub.store, &hub.engine, &source).map_err(convert_err)?;
        Ok(conversion_preview(id, &preview))
    }

    /// Save the converted copy as a new library character and open it. The original is untouched.
    ///
    /// # Errors
    /// As [`Self::preview_convert_to_2024`], plus `io_error` when the copy cannot be saved.
    pub fn apply_convert_to_2024(&self, id: &str) -> ApiResult<ConversionApplied> {
        let source = self.source_character(id)?;
        let hub = self.hub()?;
        let preview =
            convert_to_2024_with(&hub.store, &hub.engine, &source).map_err(convert_err)?;
        let mut copy = apply_conversion(&preview);
        let new = new_id();
        copy.meta.id.clone_from(&new);
        copy.meta.converted_from = Some(id.to_string());
        let path = self.character_path(&new)?;
        copy.save(&path)?;
        let snapshot = self.insert_session(&new, copy, path, false)?;
        Ok(ConversionApplied {
            new_character_id: new,
            snapshot,
        })
    }
}

fn convert_err(e: ConvertError) -> ApiError {
    ApiError::requirement(format!("{e}."))
}
