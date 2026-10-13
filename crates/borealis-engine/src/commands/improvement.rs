//! Ability score improvement / feat choices at class levels (`engine.md` 11.2).

use super::selection::slot_key;
use super::{CmdResult, CommandError};
use crate::engine::Engine;
use crate::ids::ABILITIES;
use crate::state::{CharacterState, Choice};
use crate::universe::sanitize_id;

/// What a character takes at an improvement level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Improvement {
    /// +2 to one ability (index into STR..CHA).
    PlusTwo(usize),
    /// +1 to two different abilities.
    PlusOneEach(usize, usize),
    /// A feat (requires the Feats option).
    Feat(String),
}

fn err(m: impl Into<String>) -> CommandError {
    CommandError::NotAllowed(m.into())
}

/// Apply an improvement at `level` of the class named `class`. Existing picks for that level are
/// replaced.
///
/// # Errors
/// [`CommandError::UnknownSlot`] when the class has no such improvement slot yet, or the errors of
/// [`super::select`] for illegal choices.
pub fn choose_improvement(
    engine: &Engine,
    state: &CharacterState,
    class: &str,
    level: u32,
    what: &Improvement,
) -> CmdResult {
    let snap = engine.derive_with(state, false);
    let slot_name = format!("Improvement Option ({class} {level})");
    let slot = snap
        .slots
        .iter()
        .find(|s| s.name == slot_name)
        .ok_or(CommandError::UnknownSlot)?;
    let up = sanitize_id(class);
    let (kind, picks): (&str, Vec<String>) = match what {
        Improvement::PlusTwo(a) => ("ASI", vec![asi_id(*a)?; 2]),
        Improvement::PlusOneEach(a, b) if a != b => ("ASI", vec![asi_id(*a)?, asi_id(*b)?]),
        Improvement::PlusOneEach(..) => return Err(err("choose two different abilities")),
        Improvement::Feat(f) => ("FEAT", vec![f.clone()]),
    };
    let feature = format!("ID_INTERNAL_CLASS_FEATURE_{kind}_{level}_{up}");
    let mut cur = super::select(
        engine,
        state,
        &slot.key.clone(),
        Choice::Element(feature.clone()),
    )?;
    let sn = engine.derive_with(&cur.state, false);
    let (inner_name, inner_ty) = if kind == "ASI" {
        (
            format!("Ability Score Increase ({class} {level})"),
            "Ability Score Improvement",
        )
    } else {
        (format!("Feat ({class} {level})"), "Feat")
    };
    for (i, id) in picks.iter().enumerate() {
        let key = slot_key(
            &sn,
            &feature,
            &inner_name,
            u32::try_from(i + 1).unwrap_or(1),
        )
        .ok_or(CommandError::UnknownSlot)?;
        let _ = inner_ty;
        cur = super::select(engine, &cur.state, &key, Choice::Element(id.clone()))?;
    }
    Ok(cur)
}

fn asi_id(ability: usize) -> Result<String, CommandError> {
    ABILITIES
        .get(ability)
        .map(|a| format!("ID_INTERNAL_ASI_{}", a.to_uppercase()))
        .ok_or_else(|| err("unknown ability"))
}
