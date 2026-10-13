//! Deterministic encounter math for DMs (D&D 2014 and 2024 rules).
//!
//! Sourcing (clean-room): only the CC-BY SRDs were consulted.
//!
//! * SRD 5.1 contains the XP-by-CR table (p. 258) but **no** encounter
//!   building rules: the 2014 XP thresholds, encounter multipliers and party
//!   size adjustment live in the 2014 DMG, not in SRD 5.1. They are therefore
//!   *not* built in; the 2014 maths is implemented generically over
//!   caller-supplied [`Tables2014`] (see [`edition2014`]).
//! * SRD 5.2.1 contains the full 2024 method: the XP Budget per Character
//!   table (p. 202) and the troubleshooting guidance (p. 203), built in here.

pub mod cr;
pub mod edition2014;
pub mod edition2024;
pub mod error;
pub mod report;

pub use cr::{Cr, CR0_ALT_XP};
pub use edition2014::{
    evaluate_encounter_2014, suggest_budget_2014, Multiplier, Tables2014, Tables2014Error,
};
pub use error::EncounterError;
pub use report::{Band, EncounterReport, Remaining, Ruleset, Thresholds, Warning};

/// A monster group: challenge rating and how many of them.
pub type MonsterGroup = (Cr, u32);

/// Highest character level accepted.
pub const MAX_LEVEL: u8 = 20;

/// Evaluate an encounter. 2024 uses the built-in SRD 5.2.1 tables; 2014
/// returns [`EncounterError::Tables2014NotInSrd`] (use
/// [`evaluate_encounter_2014`] with your own tables).
pub fn evaluate_encounter(
    ruleset: Ruleset,
    party: &[u8],
    monsters: &[MonsterGroup],
) -> Result<EncounterReport, EncounterError> {
    match ruleset {
        Ruleset::Edition2024 => edition2024::evaluate(party, monsters),
        Ruleset::Edition2014 => Err(EncounterError::Tables2014NotInSrd),
    }
}

/// XP to spend for `target` (2024 only; see [`suggest_budget_2014`] for 2014).
pub fn suggest_budget(ruleset: Ruleset, party: &[u8], target: Band) -> Result<u64, EncounterError> {
    match ruleset {
        Ruleset::Edition2024 => edition2024::suggest_budget(party, target),
        Ruleset::Edition2014 => Err(EncounterError::Tables2014NotInSrd),
    }
}

pub(crate) fn validate_party(party: &[u8]) -> Result<(), EncounterError> {
    if party.is_empty() {
        return Err(EncounterError::EmptyParty);
    }
    match party.iter().find(|l| !(1..=MAX_LEVEL).contains(l)) {
        Some(&bad) => Err(EncounterError::InvalidLevel(bad)),
        None => Ok(()),
    }
}

/// Sum of XP for groups (saturating).
pub(crate) fn raw_xp(monsters: &[MonsterGroup]) -> u64 {
    monsters
        .iter()
        .map(|(cr, n)| cr.xp().saturating_mul(u64::from(*n)))
        .fold(0, u64::saturating_add)
}

pub(crate) fn monster_count(monsters: &[MonsterGroup]) -> u32 {
    monsters
        .iter()
        .map(|(_, n)| *n)
        .fold(0, u32::saturating_add)
}
