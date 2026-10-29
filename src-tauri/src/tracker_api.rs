//! Play tab spell tracker services (GitHub #2, decision U13): spell slot and Pact Magic slot
//! uses, concentration, and the user's own trackers. Rests are the existing `short_rest` /
//! `long_rest` services; every change goes through [`AppState::mutate`], which also clamps the
//! tracker to the character's current maximums.

use crate::builder::apply;
use crate::dto::CharacterSnapshot;
use crate::error::ApiResult;
use crate::state::AppState;
use borealis_engine::commands::{self, SlotPool, TrackerInput};
use borealis_engine::state::TrackerReset;

impl AppState {
    /// Mark `used` spell slots of `level` (or, with `pact`, Pact Magic slots) as used.
    ///
    /// # Errors
    /// `requirement_failed` when the character has no such slots or fewer than `used`.
    pub fn set_spell_slots_used(
        &self,
        id: &str,
        level: u32,
        used: u32,
        pact: bool,
    ) -> ApiResult<CharacterSnapshot> {
        let pool = if pact {
            SlotPool::Pact
        } else {
            SlotPool::Level(level)
        };
        self.mutate(id, |ctx, d| {
            let out =
                commands::set_spell_slots_used(&ctx.hub.engine, &d.character.build, pool, used)?;
            apply(d, out);
            Ok(())
        })
    }

    /// Concentrate on a spell (replacing any other), or end concentration with `None`.
    ///
    /// # Errors
    /// `requirement_failed` for a spell that is not a concentration spell the character knows or
    /// has prepared.
    pub fn set_concentration(
        &self,
        id: &str,
        spell_id: Option<&str>,
    ) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let out = commands::set_concentration(&ctx.hub.engine, &d.character.build, spell_id)?;
            apply(d, out);
            Ok(())
        })
    }

    /// Add a custom tracker.
    ///
    /// # Errors
    /// `invalid_argument` for a bad name or maximum, or too many trackers.
    pub fn add_tracker(
        &self,
        id: &str,
        name: &str,
        max: u32,
        reset: TrackerReset,
    ) -> ApiResult<CharacterSnapshot> {
        let input = TrackerInput {
            name: name.to_string(),
            max,
            reset,
        };
        self.mutate(id, |_, d| {
            apply(d, commands::add_tracker(&d.character.build, &input)?);
            Ok(())
        })
    }

    /// Rename a custom tracker or change its maximum or reset.
    ///
    /// # Errors
    /// `not_found` for an unknown tracker, `invalid_argument` for a bad name or maximum.
    pub fn update_tracker(
        &self,
        id: &str,
        tracker_id: &str,
        name: &str,
        max: u32,
        reset: TrackerReset,
    ) -> ApiResult<CharacterSnapshot> {
        let input = TrackerInput {
            name: name.to_string(),
            max,
            reset,
        };
        self.mutate(id, |_, d| {
            apply(
                d,
                commands::update_tracker(&d.character.build, tracker_id, &input)?,
            );
            Ok(())
        })
    }

    /// Delete a custom tracker.
    ///
    /// # Errors
    /// `not_found` for an unknown tracker.
    pub fn remove_tracker(&self, id: &str, tracker_id: &str) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |_, d| {
            apply(d, commands::remove_tracker(&d.character.build, tracker_id)?);
            Ok(())
        })
    }

    /// Set how many pips of a custom tracker are used.
    ///
    /// # Errors
    /// `not_found` for an unknown tracker, `invalid_argument` above its maximum.
    pub fn set_tracker_used(
        &self,
        id: &str,
        tracker_id: &str,
        used: u32,
    ) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |_, d| {
            apply(
                d,
                commands::set_tracker_used(&d.character.build, tracker_id, used)?,
            );
            Ok(())
        })
    }
}
