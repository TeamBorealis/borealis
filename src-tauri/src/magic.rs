//! Spell services: preparing spells, swapping a learned spell, and statistic traces.

use crate::builder::apply;
use crate::dto::CharacterSnapshot;
use crate::dto_ext::StatTraceDto;
use crate::error::{ApiError, ApiResult};
use crate::ids::{decode_class_slot, decode_slot};
use crate::state::AppState;
use borealis_engine::commands;
use borealis_engine::snapshot::CharacterSnapshot as EngineSnapshot;
use borealis_engine::ClassSlot;

/// Name of the spellcasting class progression `slot` refers to.
fn class_name(snap: &EngineSnapshot, slot: &ClassSlot) -> ApiResult<String> {
    snap.classes
        .iter()
        .find(|k| &k.slot == slot)
        .and_then(|k| k.class.as_ref())
        .map(|c| c.name.clone())
        .ok_or_else(|| ApiError::not_found("That class does not exist."))
}

impl AppState {
    /// Mark a spell prepared or not for a class (the engine enforces the limit).
    ///
    /// # Errors
    /// `not_found` for an unknown class, `requirement_failed` for cantrips, always-prepared
    /// spells or a full list.
    pub fn set_spell_prepared(
        &self,
        id: &str,
        class_slot_id: &str,
        spell_id: &str,
        prepared: bool,
    ) -> ApiResult<CharacterSnapshot> {
        let slot = decode_class_slot(class_slot_id)?;
        self.mutate(id, |ctx, d| {
            let engine = &ctx.hub.engine;
            let snap = engine.derive_with(&d.character.build, false);
            let class = class_name(&snap, &slot)?;
            let out =
                commands::set_prepared(engine, &d.character.build, &class, spell_id, prepared)?;
            apply(d, out);
            Ok(())
        })
    }

    /// Replace the spell in a filled slot: the old pick is unregistered and
    /// unprepared, recorded in the slot's history, and the slot reopens at the current level.
    ///
    /// # Errors
    /// `not_found` for an unknown slot, `invalid_argument` when the slot is not a spell slot or is
    /// empty, `requirement_failed` when the swap rule forbids it (class, cantrip, rule setting).
    pub fn replace_spell(&self, id: &str, slot_id: &str) -> ApiResult<CharacterSnapshot> {
        let key = decode_slot(slot_id)?;
        self.mutate(id, |ctx, d| {
            let engine = &ctx.hub.engine;
            let snap = engine.derive_with(&d.character.build, false);
            let slot = snap
                .slots
                .iter()
                .find(|s| s.key == key)
                .ok_or_else(|| ApiError::not_found("That selection slot does not exist."))?;
            if slot.spellcasting.is_none() {
                return Err(ApiError::invalid("That slot does not hold a spell."));
            }
            if slot.chosen.is_none() {
                return Err(ApiError::invalid("No spell is chosen in that slot."));
            }
            let out = commands::replace_spell(engine, &d.character.build, &key)?;
            apply(d, out);
            Ok(())
        })
    }

    /// The provenance trace of a statistic.
    ///
    /// # Errors
    /// `not_found` if the character is not open, `invalid_argument` for an empty name.
    pub fn explain_stat(&self, id: &str, stat: &str) -> ApiResult<StatTraceDto> {
        if stat.trim().is_empty() {
            return Err(ApiError::invalid("Name the statistic to explain."));
        }
        self.read(id, |ctx, s| {
            let trace = ctx.hub.engine.explain(&s.character.build, stat);
            Ok(StatTraceDto::from(&trace))
        })
    }
}
