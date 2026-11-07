//! Play tab hit point services (B-194, decision PL1): damage, healing, temporary hit points and
//! hit dice. Damage and healing are relative, like resource uses. Each command reads the hub's
//! cached derive of the current state, so a press makes one derive (the new state's, which the
//! snapshot reuses). Rests are the existing `short_rest` / `long_rest` services.

use crate::builder::apply;
use crate::dto::CharacterSnapshot;
use crate::error::ApiResult;
use crate::state::AppState;
use borealis_engine::commands;

impl AppState {
    /// Take `amount` damage (temporary hit points first, never below 0 hit points).
    ///
    /// # Errors
    /// `invalid_argument` for an amount outside 1..=9999.
    pub fn take_damage(&self, id: &str, amount: u32) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let snap = ctx.hub.derive(&d.character.build);
            apply(d, commands::take_damage(&snap, &d.character.build, amount)?);
            Ok(())
        })
    }

    /// Regain `amount` hit points (never above the maximum).
    ///
    /// # Errors
    /// `invalid_argument` for an amount outside 1..=9999.
    pub fn regain_hit_points(&self, id: &str, amount: u32) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let snap = ctx.hub.derive(&d.character.build);
            apply(
                d,
                commands::regain_hit_points(&snap, &d.character.build, amount)?,
            );
            Ok(())
        })
    }

    /// Set the temporary hit points (0 ends them; a new value replaces the old one).
    ///
    /// # Errors
    /// `invalid_argument` above 9999.
    pub fn set_temp_hit_points(&self, id: &str, amount: u32) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |_, d| {
            apply(d, commands::set_temp_hp(&d.character.build, amount)?);
            Ok(())
        })
    }

    /// Spend one hit die of a class that rolled `roll`; the character regains the roll plus its
    /// Constitution modifier.
    ///
    /// # Errors
    /// `not_found` for a class without hit dice, `requirement_failed` when none is left,
    /// `invalid_argument` for a roll the die cannot show.
    pub fn spend_hit_die(
        &self,
        id: &str,
        class_id: &str,
        roll: u32,
    ) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let snap = ctx.hub.derive(&d.character.build);
            apply(
                d,
                commands::spend_hit_die(&snap, &d.character.build, class_id, roll)?,
            );
            Ok(())
        })
    }

    /// Give back `uses` spent hit dice of a class (heals nothing).
    ///
    /// # Errors
    /// `not_found` for a class without hit dice.
    pub fn restore_hit_die(
        &self,
        id: &str,
        class_id: &str,
        uses: u32,
    ) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let snap = ctx.hub.derive(&d.character.build);
            apply(
                d,
                commands::restore_hit_dice(&snap, &d.character.build, class_id, uses)?,
            );
            Ok(())
        })
    }
}
