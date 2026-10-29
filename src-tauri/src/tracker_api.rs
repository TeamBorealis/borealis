//! Play tab spell tracker services (GitHub #2, decision U13): spend and restore spell slot and
//! Pact Magic slot uses, concentration, and the user's own trackers. Uses are relative (one more,
//! one fewer), like resources. Rests are the existing `short_rest` / `long_rest` services; every
//! change goes through [`AppState::mutate`], which also clamps the tracker to the character's
//! current maximums. The slot and concentration commands read the hub's cached derive of the
//! current state, so a press makes one derive (the new state's, which the snapshot reuses).

use crate::builder::apply;
use crate::dto::CharacterSnapshot;
use crate::error::ApiResult;
use crate::state::AppState;
use borealis_engine::commands::{self, SlotPool, TrackerInput};
use borealis_engine::state::TrackerReset;

fn pool(level: u32, pact: bool) -> SlotPool {
    if pact {
        SlotPool::Pact
    } else {
        SlotPool::Level(level)
    }
}

/// What the user entered in the tracker dialog.
pub struct TrackerFields<'a> {
    /// Name.
    pub name: &'a str,
    /// Maximum.
    pub max: u32,
    /// When it resets.
    pub reset: TrackerReset,
    /// Shown counting up.
    pub counts_up: bool,
}

impl TrackerFields<'_> {
    fn input(&self) -> TrackerInput {
        TrackerInput {
            name: self.name.to_string(),
            max: self.max,
            reset: self.reset.clone(),
            counts_up: self.counts_up,
        }
    }
}

impl AppState {
    /// Mark `uses` more slots of `level` (or, with `pact`, Pact Magic slots) used.
    ///
    /// # Errors
    /// `requirement_failed` when the character has no such slots or fewer than `uses` left.
    pub fn spend_spell_slot(
        &self,
        id: &str,
        level: u32,
        pact: bool,
        uses: u32,
    ) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let snap = ctx.hub.derive(&d.character.build);
            let out =
                commands::spend_spell_slots(&snap, &d.character.build, pool(level, pact), uses)?;
            apply(d, out);
            Ok(())
        })
    }

    /// Give back `uses` used slots of `level` (or of the Pact Magic pool), never below none used.
    ///
    /// # Errors
    /// `requirement_failed` when the character has no such slots.
    pub fn restore_spell_slot(
        &self,
        id: &str,
        level: u32,
        pact: bool,
        uses: u32,
    ) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let snap = ctx.hub.derive(&d.character.build);
            let out =
                commands::restore_spell_slots(&snap, &d.character.build, pool(level, pact), uses)?;
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
            let snap = ctx.hub.derive(&d.character.build);
            let out =
                commands::set_concentration(&ctx.hub.engine, &d.character.build, &snap, spell_id)?;
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
        fields: &TrackerFields<'_>,
    ) -> ApiResult<CharacterSnapshot> {
        let input = fields.input();
        self.mutate(id, |_, d| {
            apply(d, commands::add_tracker(&d.character.build, &input)?);
            Ok(())
        })
    }

    /// Rename a custom tracker or change its maximum, reset or direction.
    ///
    /// # Errors
    /// `not_found` for an unknown tracker, `invalid_argument` for a bad name or maximum.
    pub fn update_tracker(
        &self,
        id: &str,
        tracker_id: &str,
        fields: &TrackerFields<'_>,
    ) -> ApiResult<CharacterSnapshot> {
        let input = fields.input();
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

    /// Mark `uses` more pips of a custom tracker used (gained, for one that counts up).
    ///
    /// # Errors
    /// `not_found` for an unknown tracker, `requirement_failed` when fewer than `uses` are left.
    pub fn spend_tracker(
        &self,
        id: &str,
        tracker_id: &str,
        uses: u32,
    ) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |_, d| {
            apply(
                d,
                commands::spend_tracker(&d.character.build, tracker_id, uses)?,
            );
            Ok(())
        })
    }

    /// Give back `uses` pips of a custom tracker, never below none used.
    ///
    /// # Errors
    /// `not_found` for an unknown tracker.
    pub fn restore_tracker(
        &self,
        id: &str,
        tracker_id: &str,
        uses: u32,
    ) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |_, d| {
            apply(
                d,
                commands::restore_tracker(&d.character.build, tracker_id, uses)?,
            );
            Ok(())
        })
    }
}
