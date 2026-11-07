//! Play tab hit point commands (B-194, decision PL1): damage, healing, temporary hit points, and
//! spending and giving back hit dice. Damage and healing are relative ("7 damage"), like resource
//! uses, so quick presses are never lost. Each reads a derive of `state` the caller already has
//! (the host's cached one) for the maximum, the classes and the Constitution modifier. The rules
//! are in [`crate::hit_points`]; the Long Rest part is in [`super::long_rest`].

use super::{CmdResult, CommandError, CommandOutcome};
use crate::hit_points::{
    after_damage, after_heal, con_modifier, hit_dice, hit_die_healing, max_hp, HitDiceUse,
    MAX_HP_CHANGE,
};
use crate::profile::profile_for;
use crate::snapshot::CharacterSnapshot;
use crate::state::{CharacterState, PlayState};

fn with_play(state: &CharacterState, play: PlayState) -> CmdResult {
    Ok(CommandOutcome::new(CharacterState {
        play,
        ..state.clone()
    }))
}

/// `amount`, refused outside 1..=[`MAX_HP_CHANGE`].
fn checked(amount: u32, what: &str) -> Result<u32, CommandError> {
    if (1..=MAX_HP_CHANGE).contains(&amount) {
        Ok(amount)
    } else {
        Err(CommandError::HitPoints(format!(
            "{what} is a whole number from 1 to {MAX_HP_CHANGE}"
        )))
    }
}

/// The hit dice of the class `class_id`.
fn class_dice(
    state: &CharacterState,
    snap: &CharacterSnapshot,
    class_id: &str,
) -> Result<HitDiceUse, CommandError> {
    hit_dice(&state.play, snap)
        .into_iter()
        .find(|d| d.class_id == class_id)
        .ok_or_else(|| CommandError::UnknownElement(class_id.chars().take(64).collect()))
}

/// `play` with `class_id`'s spent hit dice set to `used` (a zero entry is not stored).
fn with_dice_used(play: PlayState, class_id: &str, used: u32) -> PlayState {
    let mut hit_dice_used = play.hit_dice_used.clone();
    if used == 0 {
        hit_dice_used.remove(class_id);
    } else {
        hit_dice_used.insert(class_id.to_string(), used);
    }
    PlayState {
        hit_dice_used,
        ..play
    }
}

/// Take `amount` damage: temporary hit points first, then hit points, never below 0. `snap` is a
/// derive of `state`.
///
/// # Errors
/// [`CommandError::HitPoints`] for an amount outside 1..=[`MAX_HP_CHANGE`].
pub fn take_damage(snap: &CharacterSnapshot, state: &CharacterState, amount: u32) -> CmdResult {
    let amount = checked(amount, "damage")?;
    with_play(state, after_damage(&state.play, max_hp(snap), amount))
}

/// Regain `amount` hit points, never above the maximum. `snap` is a derive of `state`.
///
/// # Errors
/// [`CommandError::HitPoints`] for an amount outside 1..=[`MAX_HP_CHANGE`].
pub fn regain_hit_points(
    snap: &CharacterSnapshot,
    state: &CharacterState,
    amount: u32,
) -> CmdResult {
    let amount = checked(amount, "healing")?;
    with_play(state, after_heal(&state.play, max_hp(snap), amount))
}

/// Set the temporary hit points to `amount` (0 ends them). They don't stack: the player picks
/// which to keep, and entering a value is that pick, so it replaces the old one.
///
/// # Errors
/// [`CommandError::HitPoints`] above [`MAX_HP_CHANGE`].
pub fn set_temp_hp(state: &CharacterState, amount: u32) -> CmdResult {
    if amount > MAX_HP_CHANGE {
        return Err(CommandError::HitPoints(format!(
            "temporary hit points are a whole number from 0 to {MAX_HP_CHANGE}"
        )));
    }
    let play = PlayState {
        temp_hp: amount,
        ..state.play.clone()
    };
    with_play(state, play)
}

/// Spend one hit die of the class `class_id` that rolled `roll`: the character regains the roll
/// plus its Constitution modifier (at least the edition's minimum), never above the maximum.
/// `snap` is a derive of `state`.
///
/// # Errors
/// [`CommandError::UnknownElement`] for a class without hit dice, [`CommandError::Level`] when
/// none is left, [`CommandError::HitPoints`] for a roll the die cannot show.
pub fn spend_hit_die(
    snap: &CharacterSnapshot,
    state: &CharacterState,
    class_id: &str,
    roll: u32,
) -> CmdResult {
    let dice = class_dice(state, snap, class_id)?;
    if dice.used >= dice.total {
        return Err(CommandError::Level(format!(
            "{} has no hit dice left",
            dice.class_name
        )));
    }
    if !(1..=dice.die).contains(&roll) {
        return Err(CommandError::HitPoints(format!(
            "a d{} rolls 1 to {}",
            dice.die, dice.die
        )));
    }
    let rules = profile_for(state.ruleset).hit_dice();
    let healed = hit_die_healing(roll, con_modifier(snap), rules);
    let play = after_heal(&state.play, max_hp(snap), healed);
    with_play(state, with_dice_used(play, class_id, dice.used + 1))
}

/// Give back `n` spent hit dice of the class `class_id` (never below none spent). Heals nothing.
/// `snap` is a derive of `state`.
///
/// # Errors
/// [`CommandError::UnknownElement`] for a class without hit dice.
pub fn restore_hit_dice(
    snap: &CharacterSnapshot,
    state: &CharacterState,
    class_id: &str,
    n: u32,
) -> CmdResult {
    let dice = class_dice(state, snap, class_id)?;
    let play = with_dice_used(state.play.clone(), class_id, dice.used.saturating_sub(n));
    with_play(state, play)
}
