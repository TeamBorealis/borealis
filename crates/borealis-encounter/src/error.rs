//! Error type.

use thiserror::Error;

/// Everything that can go wrong building a report.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EncounterError {
    /// No characters supplied.
    #[error("party is empty")]
    EmptyParty,
    /// Character level outside 1..=20.
    #[error("character level {0} is outside 1..=20")]
    InvalidLevel(u8),
    /// Unparseable challenge rating.
    #[error("invalid challenge rating {0:?}")]
    InvalidCr(String),
    /// 2014 thresholds/multipliers are not part of SRD 5.1.
    #[error("2014 XP thresholds and multipliers are not in SRD 5.1; supply Tables2014")]
    Tables2014NotInSrd,
    /// The band has no budget in this ruleset (e.g. Extreme in 2024).
    #[error("band {0:?} has no budget in this ruleset")]
    NoBudgetForBand(crate::Band),
    /// Caller-supplied 2014 tables are malformed.
    #[error("invalid 2014 tables: {0}")]
    InvalidTables(#[from] crate::Tables2014Error),
}
