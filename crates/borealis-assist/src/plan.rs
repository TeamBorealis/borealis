//! Build plans as data, and the step-by-step engine replay that validates them.
//!
//! A [`Plan`] says, level by level, which class gains the level, which element goes into which
//! open slot (named by its description, e.g. `Martial Archetype`) and which ability score
//! improvement or feat is taken. Nothing in a plan is trusted: [`replay_step`] runs the real
//! engine commands on a cloned state and refuses every step that is not legal at that point.

use borealis_engine::commands;
use borealis_engine::snapshot::{CharacterSnapshot, SlotSnapshot};
use borealis_engine::{CharacterState, Choice, Engine};
use serde::{Deserialize, Serialize};

use crate::error::AssistError;
use crate::options::{
    newly_open_slots, open_slots, parse_improvement_slot, take_level, ImprovementSpec, LevelPath,
};
use crate::summary::{level_summary, LevelSummary};

/// How many allowed ids are quoted back in a refusal.
const MAX_QUOTED_IDS: usize = 25;

/// One choice in an open slot, addressed by the slot's description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanChoice {
    /// Slot title as shown by the engine (`Martial Archetype`, `Fighting Style`, ...).
    pub slot: String,
    /// Which of several identically named slots (1-based); first open one when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ordinal: Option<u32>,
    /// The chosen element id.
    pub element: String,
}

/// What happens at one character level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStep {
    /// The character level reached by this step (previous level + 1).
    pub level: u32,
    /// Which class gains the level.
    pub path: LevelPath,
    /// Slot choices, applied in order.
    #[serde(default)]
    pub choices: Vec<PlanChoice>,
    /// Ability score improvement or feat, when the level grants one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub improvement: Option<ImprovementSpec>,
    /// Why (prose; cleared when it contains numbers the engine did not compute).
    #[serde(default)]
    pub rationale: String,
}

/// An ordered list of steps; plain data the UI can store and replay.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// Steps for consecutive levels.
    pub steps: Vec<PlanStep>,
}

/// A required choice that a step left open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenChoice {
    /// Slot title.
    pub slot: String,
    /// How many options it offers.
    pub options: usize,
}

/// The result of a successfully replayed step.
#[derive(Debug, Clone, PartialEq)]
pub struct StepOutcome {
    /// State after the step.
    pub state: CharacterState,
    /// Engine digest of the new level.
    pub summary: LevelSummary,
    /// Required slots that this level opened and the step did not fill.
    pub open_choices: Vec<OpenChoice>,
}

fn quote_ids(slot: &SlotSnapshot) -> String {
    let mut ids: Vec<&str> = slot.options.iter().map(|o| o.id.as_str()).collect();
    let more = ids.len().saturating_sub(MAX_QUOTED_IDS);
    ids.truncate(MAX_QUOTED_IDS);
    let mut s = ids.join(", ");
    if more > 0 {
        s.push_str(&format!(" (+{more} more)"));
    }
    s
}

fn open_names(snap: &CharacterSnapshot) -> String {
    let mut names: Vec<String> = open_slots(snap).iter().map(|s| s.name.clone()).collect();
    names.sort();
    names.dedup();
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join("; ")
    }
}

fn apply_choice(
    engine: &Engine,
    state: &CharacterState,
    c: &PlanChoice,
) -> Result<CharacterState, String> {
    let snap = engine.derive(state);
    let wanted = c.slot.trim().to_lowercase();
    let named: Vec<&SlotSnapshot> = snap
        .slots
        .iter()
        .filter(|s| s.rejected.is_none() && s.name.to_lowercase() == wanted)
        .collect();
    if named.is_empty() {
        return Err(format!(
            "there is no slot called \"{}\" at this point (open slots: {})",
            c.slot,
            open_names(&snap)
        ));
    }
    let slot = match c.ordinal {
        Some(o) => named.iter().find(|s| s.key.ordinal == o).copied(),
        None => named
            .iter()
            .find(|s| s.chosen.is_none() && s.options.iter().any(|o| o.id == c.element))
            .or_else(|| {
                named
                    .iter()
                    .find(|s| s.options.iter().any(|o| o.id == c.element))
            })
            .copied(),
    };
    let Some(slot) = slot else {
        return Err(format!(
            "\"{}\" is not a legal choice for \"{}\"; legal ids: {}",
            c.element,
            c.slot,
            named.first().map_or_else(String::new, |s| quote_ids(s))
        ));
    };
    if !slot.options.iter().any(|o| o.id == c.element) {
        return Err(format!(
            "\"{}\" is not a legal choice for \"{}\"; legal ids: {}",
            c.element,
            c.slot,
            quote_ids(slot)
        ));
    }
    commands::select(engine, state, &slot.key, Choice::Element(c.element.clone()))
        .map(|o| o.state)
        .map_err(|e| format!("\"{}\" for \"{}\": {e}", c.element, c.slot))
}

fn apply_improvement(
    engine: &Engine,
    state: &CharacterState,
    what: &ImprovementSpec,
) -> Result<CharacterState, String> {
    let snap = engine.derive(state);
    let slot = open_slots(&snap)
        .into_iter()
        .find(|s| parse_improvement_slot(&s.name).is_some());
    let Some((class, level)) = slot.and_then(|s| parse_improvement_slot(&s.name)) else {
        return Err(format!(
            "this level grants no ability score improvement or feat (open slots: {})",
            open_names(&snap)
        ));
    };
    let engine_what = what.to_engine().map_err(|e| e.to_string())?;
    commands::choose_improvement(engine, state, &class, level, &engine_what)
        .map(|o| o.state)
        .map_err(|e| format!("improvement refused: {e}"))
}

/// Replay one step on a clone of `state`.
///
/// All problems of the step are collected (a failed level-up stops early); the state is only
/// advanced when there are none.
///
/// # Errors
/// The list of reasons the step is not legal at this point.
pub fn replay_step(
    engine: &Engine,
    state: &CharacterState,
    step: &PlanStep,
) -> Result<StepOutcome, Vec<String>> {
    let expected = state.level() + 1;
    if step.level != expected {
        return Err(vec![format!(
            "the step says level {} but the next level is {expected}",
            step.level
        )]);
    }
    let before = engine.derive(state);
    let mut cur = take_level(engine, state, &step.path)
        .map_err(|e| vec![format!("cannot take the level: {e}")])?
        .state;
    let mut issues = Vec::new();
    for c in &step.choices {
        match apply_choice(engine, &cur, c) {
            Ok(s) => cur = s,
            Err(e) => issues.push(e),
        }
    }
    if let Some(what) = &step.improvement {
        match apply_improvement(engine, &cur, what) {
            Ok(s) => cur = s,
            Err(e) => issues.push(e),
        }
    }
    if !issues.is_empty() {
        return Err(issues);
    }
    let after = engine.derive(&cur);
    let open_choices = newly_open_slots(&before, &after)
        .into_iter()
        .filter(|s| s.requires_selection)
        .map(|s| OpenChoice {
            slot: s.name.clone(),
            options: s.options.len(),
        })
        .collect();
    Ok(StepOutcome {
        summary: level_summary(Some(&before), &after),
        state: cur,
        open_choices,
    })
}

/// Apply a plan to `state`, stopping after character level `up_to_level`.
///
/// # Errors
/// [`AssistError::Invalid`] naming the first step the engine refuses.
pub fn apply_plan(
    engine: &Engine,
    state: &CharacterState,
    plan: &Plan,
    up_to_level: u32,
) -> Result<CharacterState, AssistError> {
    let mut cur = state.clone();
    for step in plan
        .steps
        .iter()
        .filter(|s| s.level > state.level() && s.level <= up_to_level)
    {
        match replay_step(engine, &cur, step) {
            Ok(o) => cur = o.state,
            Err(issues) => {
                return Err(AssistError::Invalid(format!(
                    "step for level {} is not legal: {}",
                    step.level,
                    issues.join("; ")
                )))
            }
        }
    }
    Ok(cur)
}
