//! Play tab spell tracker (GitHub #2, decision U13): how many spell slots of each level and how
//! many Pact Magic slots the character has (from a derive), which of them are used, the spell it
//! concentrates on, and the user's own trackers. The used counts live in
//! [`crate::state::PlayState`]; everything shown is clamped to the current maximums, and
//! [`normalized`] writes that clamp back after a change (level down, respec).

use crate::engine::Engine;
use crate::snapshot::CharacterSnapshot;
use crate::state::{CharacterState, CustomTracker, PlayState, TrackerReset};
use serde::Serialize;
use std::collections::BTreeMap;

/// Spell levels that have slots.
pub const SPELL_LEVELS: u32 = 9;

/// Pact Magic slots: all of one level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PactPool {
    /// Slot level.
    pub level: u32,
    /// Number of slots.
    pub count: u32,
}

/// The slot maximums of a derived character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct SlotPools {
    /// Normal slots per spell level 1..=9: the multiclass table when the character pools two or
    /// more casters, else its one caster's table. Never holds Pact Magic slots.
    pub levels: [u32; 9],
    /// Pact Magic slots, a pool of their own.
    pub pact: Option<PactPool>,
}

impl SlotPools {
    /// Slots of spell level `level` (0 outside 1..=9).
    #[must_use]
    pub fn max_of(&self, level: u32) -> u32 {
        level
            .checked_sub(1)
            .and_then(|i| self.levels.get(i as usize))
            .copied()
            .unwrap_or(0)
    }
}

fn count(n: i32) -> u32 {
    u32::try_from(n).unwrap_or(0)
}

/// The slot maximums of `snap`. Without a pooled table at most one caster has normal slots, so
/// the sum is that caster's table (the Magic page reads the same numbers).
#[must_use]
pub fn slot_pools(snap: &CharacterSnapshot) -> SlotPools {
    let mut levels = [0u32; 9];
    match &snap.multiclass_slots {
        Some(m) => {
            for (t, n) in levels.iter_mut().zip(m.slots) {
                *t = count(n);
            }
        }
        None => {
            for c in &snap.spellcasting {
                for (t, n) in levels.iter_mut().zip(c.slots) {
                    *t += count(n);
                }
            }
        }
    }
    let pact = snap
        .spellcasting
        .iter()
        .filter_map(|c| c.pact)
        .map(|(n, level)| PactPool {
            level: count(level),
            count: count(n),
        })
        .reduce(|a, b| PactPool {
            level: a.level.max(b.level),
            count: a.count + b.count,
        })
        .filter(|p| p.count > 0);
    SlotPools { levels, pact }
}

/// Used and maximum slots of one pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SlotUse {
    /// Spell level of the slots.
    pub level: u32,
    /// Slots.
    pub max: u32,
    /// Used slots (at most `max`).
    pub used: u32,
}

/// A spell the character can concentrate on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConcentrationSpell {
    /// Spell id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Spell level (0 = cantrip).
    pub level: u32,
}

/// Everything the Play tab shows for the tracker, clamped to the current maximums.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct TrackerView {
    /// Normal slots, one row per spell level that has slots.
    pub spell_slots: Vec<SlotUse>,
    /// Pact Magic slots.
    pub pact_slots: Option<SlotUse>,
    /// The spell being concentrated on (dropped while it is no longer ready).
    pub concentration: Option<ConcentrationSpell>,
    /// Spells the character knows or has prepared that need concentration, by level and name.
    pub concentration_spells: Vec<ConcentrationSpell>,
    /// The user's trackers.
    pub trackers: Vec<CustomTracker>,
}

/// Concentration spells the character has ready: known (a known caster's spells, a spellbook,
/// granted spells) or prepared, in any of its spellcasting entries.
#[must_use]
pub fn concentration_spells(
    engine: &Engine,
    state: &CharacterState,
    snap: &CharacterSnapshot,
) -> Vec<ConcentrationSpell> {
    let uni = engine.universe(state.ruleset);
    let mut out: BTreeMap<String, ConcentrationSpell> = BTreeMap::new();
    for s in snap
        .spellcasting
        .iter()
        .flat_map(|c| &c.spells)
        .filter(|s| s.known || s.prepared || s.always_prepared)
    {
        let concentration = uni
            .get(&s.id)
            .and_then(|e| e.el.setter_value("isConcentration"))
            .is_some_and(|v| v.trim().eq_ignore_ascii_case("true"));
        if concentration {
            out.entry(s.id.clone())
                .or_insert_with(|| ConcentrationSpell {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    level: s.level,
                });
        }
    }
    let mut list: Vec<ConcentrationSpell> = out.into_values().collect();
    list.sort_by(|a, b| a.level.cmp(&b.level).then_with(|| a.name.cmp(&b.name)));
    list
}

/// The tracker as the Play tab shows it.
#[must_use]
pub fn tracker_view(
    engine: &Engine,
    state: &CharacterState,
    snap: &CharacterSnapshot,
) -> TrackerView {
    let pools = slot_pools(snap);
    let spells = concentration_spells(engine, state, snap);
    let play = clamped(&state.play, &pools, &spells);
    let spell_slots = (1..=SPELL_LEVELS)
        .filter(|l| pools.max_of(*l) > 0)
        .map(|level| SlotUse {
            level,
            max: pools.max_of(level),
            used: play.spell_slots_used.get(&level).copied().unwrap_or(0),
        })
        .collect();
    let pact_slots = pools.pact.map(|p| SlotUse {
        level: p.level,
        max: p.count,
        used: play.pact_slots_used,
    });
    let concentration = play
        .concentration
        .as_ref()
        .and_then(|id| spells.iter().find(|s| &s.id == id).cloned());
    TrackerView {
        spell_slots,
        pact_slots,
        concentration,
        concentration_spells: spells,
        trackers: play.trackers,
    }
}

/// `play` with every used count at most its maximum and a concentration spell the character no
/// longer has ready dropped.
fn clamped(play: &PlayState, pools: &SlotPools, ready: &[ConcentrationSpell]) -> PlayState {
    PlayState {
        spell_slots_used: play
            .spell_slots_used
            .iter()
            .map(|(l, n)| (*l, (*n).min(pools.max_of(*l))))
            .filter(|(_, n)| *n > 0)
            .collect(),
        pact_slots_used: play.pact_slots_used.min(pools.pact.map_or(0, |p| p.count)),
        concentration: play
            .concentration
            .clone()
            .filter(|id| ready.iter().any(|s| &s.id == id)),
        trackers: play
            .trackers
            .iter()
            .map(|t| CustomTracker {
                used: t.used.min(t.max),
                ..t.clone()
            })
            .collect(),
        ..play.clone()
    }
}

/// `state` with its tracker clamped to `snap` (a derive of `state`), or `None` when nothing
/// changes. Run after every change so a lower maximum lowers the saved used count too.
#[must_use]
pub fn normalized(
    engine: &Engine,
    state: &CharacterState,
    snap: &CharacterSnapshot,
) -> Option<CharacterState> {
    if !has_tracking(&state.play) {
        return None;
    }
    let play = clamped(
        &state.play,
        &slot_pools(snap),
        &concentration_spells(engine, state, snap),
    );
    (play != state.play).then(|| CharacterState {
        play,
        ..state.clone()
    })
}

/// Anything recorded that a clamp could change (callers skip the derive for [`normalized`]
/// without it).
#[must_use]
pub fn has_tracking(p: &PlayState) -> bool {
    !p.spell_slots_used.is_empty()
        || p.pact_slots_used > 0
        || p.concentration.is_some()
        || p.trackers.iter().any(|t| t.used > t.max)
}

/// Which rest was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rest {
    /// Short rest: Pact Magic slots and short-rest trackers come back.
    Short,
    /// Long rest: every slot and every short- or long-rest tracker comes back; concentration
    /// ends.
    Long,
}

/// The tracker part of `play` after a rest.
#[must_use]
pub fn after_rest(play: &PlayState, rest: Rest) -> PlayState {
    let long = rest == Rest::Long;
    let resets = |r: TrackerReset| match r {
        TrackerReset::ShortRest => true,
        TrackerReset::LongRest => long,
        TrackerReset::Never => false,
    };
    PlayState {
        spell_slots_used: if long {
            BTreeMap::new()
        } else {
            play.spell_slots_used.clone()
        },
        pact_slots_used: 0,
        concentration: if long {
            None
        } else {
            play.concentration.clone()
        },
        trackers: play
            .trackers
            .iter()
            .map(|t| CustomTracker {
                used: if resets(t.reset) { 0 } else { t.used },
                ..t.clone()
            })
            .collect(),
        ..play.clone()
    }
}
