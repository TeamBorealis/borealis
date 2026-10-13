//! Play-time commands: resource uses, toggles, rests, exhaustion and Heroic Inspiration. They
//! only touch the runtime entry of the state (see [`crate::runtime`]).

use super::{CmdResult, CommandError, CommandOutcome};
use crate::engine::Engine;
use crate::runtime::Runtime;
use crate::snapshot::{CharacterSnapshot, ResourceSnapshot};
use crate::state::CharacterState;

fn resource<'a>(
    snap: &'a CharacterSnapshot,
    id: &str,
) -> Result<&'a ResourceSnapshot, CommandError> {
    snap.resources
        .iter()
        .find(|r| r.id == id)
        .ok_or_else(|| CommandError::UnknownElement(id.to_string()))
}

fn done(rt: &Runtime, state: &CharacterState) -> CmdResult {
    Ok(CommandOutcome::new(rt.apply(state)))
}

/// Expend `n` uses of a resource.
///
/// # Errors
/// Unknown resource or fewer than `n` uses left.
pub fn spend_resource(engine: &Engine, state: &CharacterState, id: &str, n: u32) -> CmdResult {
    let snap = engine.derive_with(state, false);
    let r = resource(&snap, id)?;
    let n_i = i32::try_from(n).unwrap_or(i32::MAX);
    if r.current < n_i {
        return Err(CommandError::Level(format!(
            "{} has {} of {} uses left",
            r.name, r.current, r.max
        )));
    }
    let mut rt = Runtime::from_state(state);
    let used = rt.used_of(id) + n;
    rt.used.insert(id.to_string(), used);
    done(&rt, state)
}

/// Give back `n` expended uses of a resource (never above the maximum).
///
/// # Errors
/// Unknown resource.
pub fn restore_resource(engine: &Engine, state: &CharacterState, id: &str, n: u32) -> CmdResult {
    let snap = engine.derive_with(state, false);
    resource(&snap, id)?;
    let mut rt = Runtime::from_state(state);
    let left = rt.used_of(id).saturating_sub(n);
    rt.used.insert(id.to_string(), left);
    done(&rt, state)
}

/// Switch a toggle (Rage) on or off.
///
/// # Errors
/// Unknown toggle.
pub fn set_toggle(engine: &Engine, state: &CharacterState, id: &str, on: bool) -> CmdResult {
    let snap = engine.derive_with(state, false);
    if !snap.toggles.iter().any(|t| t.id == id) {
        return Err(CommandError::UnknownElement(id.to_string()));
    }
    let mut rt = Runtime::from_state(state);
    if on {
        rt.toggles.insert(id.to_string());
    } else {
        rt.toggles.remove(id);
    }
    done(&rt, state)
}

/// Finish a Short Rest: resources regain what their rule says; toggles end.
///
/// # Errors
/// None today; the result type matches the other commands.
pub fn short_rest(engine: &Engine, state: &CharacterState) -> CmdResult {
    let snap = engine.derive_with(state, false);
    let mut rt = Runtime::from_state(state);
    for r in &snap.resources {
        if let Some(back) = r.short_rest_regain {
            let left = rt
                .used_of(&r.id)
                .saturating_sub(u32::try_from(back).unwrap_or(0));
            rt.used.insert(r.id.clone(), left);
        }
    }
    rt.toggles.clear();
    done(&rt, state)
}

/// Finish a Long Rest: resources refill, toggles end, exhaustion drops by one level (linear
/// model) and Resourceful-style features grant Inspiration.
///
/// # Errors
/// None today; the result type matches the other commands.
pub fn long_rest(engine: &Engine, state: &CharacterState) -> CmdResult {
    let snap = engine.derive_with(state, false);
    let mut rt = Runtime::from_state(state);
    for r in snap.resources.iter().filter(|r| r.long_rest_regain_all) {
        rt.used.remove(&r.id);
    }
    rt.toggles.clear();
    if snap.exhaustion.linear {
        rt.exhaustion = rt.exhaustion.saturating_sub(1);
    }
    if snap
        .inspiration_sources
        .iter()
        .any(|s| s.trigger == "long_rest")
    {
        rt.inspiration = true;
    }
    done(&rt, state)
}

/// Set the exhaustion level.
///
/// # Errors
/// The level is above the highest level of the profile.
pub fn set_exhaustion(engine: &Engine, state: &CharacterState, level: u32) -> CmdResult {
    let snap = engine.derive_with(state, false);
    if level > snap.exhaustion.max_levels {
        return Err(CommandError::Level(format!(
            "exhaustion goes up to level {}",
            snap.exhaustion.max_levels
        )));
    }
    let mut rt = Runtime::from_state(state);
    rt.exhaustion = level;
    done(&rt, state)
}

/// Gain or spend (Heroic) Inspiration.
#[must_use]
pub fn set_inspiration(state: &CharacterState, has: bool) -> CommandOutcome {
    let mut rt = Runtime::from_state(state);
    rt.inspiration = has;
    CommandOutcome::new(rt.apply(state))
}
