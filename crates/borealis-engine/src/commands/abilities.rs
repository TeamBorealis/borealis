//! Ability score commands: generation methods, point buy budget and the standard array.

use super::{CmdResult, CommandError, CommandOutcome};
use crate::engine::Engine;
use crate::levels;
use crate::profile::{profile_for, AbilityGeneration};
use crate::state::{AbilityMethod, CharacterState};

fn generation(state: &CharacterState) -> AbilityGeneration {
    profile_for(state.ruleset).ability_generation()
}

/// Generation rules with the content tables applied.
fn table_generation(engine: &Engine, state: &CharacterState) -> AbilityGeneration {
    engine
        .universe(state.ruleset)
        .tables()
        .generation(generation(state))
}

fn budget(engine: &Engine, state: &CharacterState) -> i32 {
    state
        .house_rules
        .point_buy_budget
        .or(engine.settings().point_buy_budget)
        .unwrap_or(table_generation(engine, state).point_buy_budget)
}

fn check_range(engine: &Engine, scores: &[i32; 6]) -> Result<(), CommandError> {
    let (lo, hi) = (
        engine.settings().min_base_score,
        engine.settings().max_base_score,
    );
    match scores.iter().find(|s| **s < lo || **s > hi) {
        Some(s) => Err(CommandError::Ability(format!(
            "base score {s} is outside {lo}..={hi}"
        ))),
        None => Ok(()),
    }
}

/// Change the generation method. Point buy resets to all 8s and the standard array to its
/// default order so the new method starts from a legal assignment.
#[must_use]
pub fn set_ability_method(state: &CharacterState, method: AbilityMethod) -> CharacterState {
    let mut next = state.clone();
    next.abilities.method = method;
    match method {
        AbilityMethod::PointBuy => next.abilities.base = [8; 6],
        AbilityMethod::StandardArray => next.abilities.base = generation(state).standard_array,
        _ => {}
    }
    next
}

/// Set all six base scores (rolled or manual entry).
///
/// # Errors
/// [`CommandError::Ability`] if a score is out of range, or violates the active method (point buy
/// budget, standard array permutation).
pub fn set_base_scores(engine: &Engine, state: &CharacterState, scores: [i32; 6]) -> CmdResult {
    check_range(engine, &scores)?;
    let b = budget(engine, state);
    match state.abilities.method {
        AbilityMethod::PointBuy if levels::points_remaining(&scores, b) < 0 => {
            return Err(CommandError::Ability(format!(
                "point buy budget of {b} exceeded"
            )));
        }
        AbilityMethod::StandardArray
            if !levels::is_standard_array(
                &scores,
                &table_generation(engine, state).standard_array,
            ) =>
        {
            return Err(CommandError::Ability(
                "the standard array values must each be used exactly once".into(),
            ));
        }
        _ => {}
    }
    let mut next = state.clone();
    next.abilities.base = scores;
    Ok(CommandOutcome::new(next))
}

/// Set one base score. Under point buy the change is refused if it would overspend.
///
/// # Errors
/// [`CommandError::Ability`] for out-of-range input or budget overrun.
pub fn set_base_score(
    engine: &Engine,
    state: &CharacterState,
    ability: usize,
    value: i32,
) -> CmdResult {
    let mut scores = state.abilities.base;
    *scores
        .get_mut(ability)
        .ok_or_else(|| CommandError::Ability("unknown ability".into()))? = value;
    if state.abilities.method == AbilityMethod::StandardArray {
        return pick_standard_array_value(state, ability, value);
    }
    set_base_scores(engine, state, scores)
}

/// Assign one standard array value to an ability; the ability that held the value swaps with the
/// one being changed so the array stays a permutation.
///
/// # Errors
/// [`CommandError::Ability`] if the value is not part of the array.
pub fn pick_standard_array_value(state: &CharacterState, ability: usize, value: i32) -> CmdResult {
    let array = generation(state).standard_array;
    if !array.contains(&value) || ability >= 6 {
        return Err(CommandError::Ability(format!(
            "{value} is not a standard array value"
        )));
    }
    let mut scores = if levels::is_standard_array(&state.abilities.base, &array) {
        state.abilities.base
    } else {
        array
    };
    if let Some(other) = scores.iter().position(|v| *v == value) {
        scores[other] = scores[ability];
    }
    scores[ability] = value;
    let mut next = state.clone();
    next.abilities.base = scores;
    Ok(CommandOutcome::new(next))
}
