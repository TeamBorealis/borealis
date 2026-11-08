//! B2a (#34): dividing a character's levels between its classes while building, without
//! levelling up one at a time. The engine keeps the level list in the shape levelling up builds
//! (`borealis_engine::commands::split_new_multiclass`, `set_multiclass_levels`); defaults of the
//! slots the moved levels open are filled as after a level up.

use crate::builder::apply_filled;
use crate::dto::CharacterSnapshot;
use crate::error::{ApiError, ApiResult};
use crate::ids::decode_class_slot;
use crate::state::AppState;
use borealis_engine::{commands, ClassSlot};

impl AppState {
    /// B2a: move `levels` of the main class's levels into a new multiclass; the total level stays.
    /// With `element_id` (a `Multiclass` element) the class is picked in its `Multiclass (Level n)`
    /// slot, prerequisites checked as for any pick there; without it the slot waits for the picker.
    ///
    /// # Errors
    /// `requirement_failed` if multiclassing is unavailable or the main class would have no level
    /// left; the pick's errors.
    pub fn split_multiclass(
        &self,
        id: &str,
        levels: u32,
        element_id: Option<&str>,
    ) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let engine = &ctx.hub.engine;
            let out =
                commands::split_new_multiclass(engine, &d.character.build, levels, element_id)?;
            apply_filled(ctx, d, out);
            Ok(())
        })
    }

    /// B2a: give a multiclass exactly `levels` levels, moving them from or back to the main
    /// class (0 removes the multiclass); the total level stays.
    ///
    /// # Errors
    /// `invalid_argument` for the main class slot; `requirement_failed` for an unknown multiclass
    /// or when the main class would have no level left.
    pub fn set_class_levels(
        &self,
        id: &str,
        class_slot_id: &str,
        levels: u32,
    ) -> ApiResult<CharacterSnapshot> {
        let ClassSlot::Multi { starting_level } = decode_class_slot(class_slot_id)? else {
            return Err(ApiError::invalid(
                "Set the levels of a multiclass: the main class keeps the rest.",
            ));
        };
        self.mutate(id, |ctx, d| {
            let engine = &ctx.hub.engine;
            let out = commands::set_multiclass_levels(
                engine,
                &d.character.build,
                starting_level,
                levels,
            )?;
            apply_filled(ctx, d, out);
            Ok(())
        })
    }
}
