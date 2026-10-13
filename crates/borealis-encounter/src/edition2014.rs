//! 2014 encounter maths over caller-supplied tables.
//!
//! SRD 5.1 does not publish the XP thresholds by character level, the
//! encounter multipliers or the party-size adjustment (they are DMG content),
//! so no values are built in. This module implements the algorithm only:
//! sum thresholds over the party, multiply monster XP by a count-based
//! multiplier shifted one step by party size, compare adjusted XP against the
//! thresholds. Monster XP comes from the SRD 5.1 table (p. 258) via `Cr`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::error::EncounterError;
use crate::report::{
    common_warnings, signed_gap, Band, EncounterReport, Remaining, Ruleset, Thresholds,
};
use crate::{monster_count, raw_xp, validate_party, MonsterGroup};

/// Malformed [`Tables2014`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Tables2014Error {
    /// Thresholds must have exactly 20 rows.
    #[error("expected 20 threshold rows, got {0}")]
    ThresholdRows(usize),
    /// Each row must be ascending (easy <= medium <= hard <= deadly).
    #[error("threshold row for level {0} is not ascending")]
    ThresholdOrder(usize),
    /// Multiplier ladder empty or inconsistent.
    #[error("multiplier ladder is empty or inconsistent")]
    Ladder,
}

/// Which multiplier step a monster count lands on (informational).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Multiplier {
    /// Index into `steps` after the party-size shift.
    pub step: usize,
    /// Multiplier in tenths (15 = x1.5).
    pub tenths: u32,
}

/// Caller-supplied 2014 tables.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tables2014 {
    /// Row `level - 1`: `[easy, medium, hard, deadly]` per character.
    pub thresholds: Vec<[u64; 4]>,
    /// Multiplier ladder in tenths, ascending (a shift moves along it).
    pub steps: Vec<u32>,
    /// Ascending monster-count breakpoints; the first is normally 1.
    pub breakpoints: Vec<u32>,
    /// Ladder index used by the first breakpoint.
    pub first_breakpoint_step: usize,
    /// Parties smaller than this shift one step up the ladder.
    pub small_party_below: usize,
    /// Parties of this size or more shift one step down the ladder.
    pub large_party_from: usize,
}

impl Tables2014 {
    /// Check structural validity.
    pub fn validate(&self) -> Result<(), Tables2014Error> {
        if self.thresholds.len() != 20 {
            return Err(Tables2014Error::ThresholdRows(self.thresholds.len()));
        }
        for (i, r) in self.thresholds.iter().enumerate() {
            if !r.windows(2).all(|w| w[0] <= w[1]) {
                return Err(Tables2014Error::ThresholdOrder(i + 1));
            }
        }
        let sorted = self.breakpoints.windows(2).all(|w| w[0] < w[1]);
        let reach = self.first_breakpoint_step + self.breakpoints.len();
        if self.steps.is_empty()
            || self.breakpoints.is_empty()
            || !sorted
            || self.first_breakpoint_step >= self.steps.len()
            || reach > self.steps.len()
        {
            return Err(Tables2014Error::Ladder);
        }
        Ok(())
    }

    /// Multiplier for a monster count and party size (assumes validated).
    pub fn multiplier(&self, monsters: u32, party_size: usize) -> Multiplier {
        let passed = self.breakpoints.iter().filter(|&&b| b <= monsters).count();
        let base = self.first_breakpoint_step + passed.saturating_sub(1);
        let last = self.steps.len() - 1;
        let step = if party_size < self.small_party_below {
            (base + 1).min(last)
        } else if party_size >= self.large_party_from {
            base.saturating_sub(1)
        } else {
            base
        };
        Multiplier {
            step,
            tenths: self.steps[step],
        }
    }

    fn party_thresholds(&self, party: &[u8]) -> Thresholds {
        let mut sum = [0u64; 4];
        for &lvl in party {
            let row = self.thresholds[usize::from(lvl) - 1];
            for (acc, v) in sum.iter_mut().zip(row) {
                *acc += v;
            }
        }
        Thresholds {
            low: sum[0],
            moderate: sum[1],
            high: sum[2],
            extreme: Some(sum[3]),
        }
    }
}

/// Highest threshold reached (`>=`); below Easy is Trivial.
pub fn band_for(adjusted: u64, t: &Thresholds) -> Band {
    let deadly = t.extreme.unwrap_or(u64::MAX);
    if adjusted >= deadly {
        Band::Extreme
    } else if adjusted >= t.high {
        Band::High
    } else if adjusted >= t.moderate {
        Band::Moderate
    } else if adjusted >= t.low {
        Band::Low
    } else {
        Band::Trivial
    }
}

/// Evaluate an encounter under the 2014 rules with the supplied tables.
pub fn evaluate_encounter_2014(
    tables: &Tables2014,
    party: &[u8],
    monsters: &[MonsterGroup],
) -> Result<EncounterReport, EncounterError> {
    tables.validate()?;
    validate_party(party)?;
    let thresholds = tables.party_thresholds(party);
    let total_xp = raw_xp(monsters);
    let count = monster_count(monsters);
    let mult = tables.multiplier(count, party.len());
    let adjusted = total_xp.saturating_mul(u64::from(mult.tenths)) / 10;
    let deadly = thresholds.extreme.unwrap_or(0);
    Ok(EncounterReport {
        ruleset: Ruleset::Edition2014,
        party_size: party.len(),
        monster_count: count,
        total_xp,
        adjusted_xp: adjusted,
        multiplier_tenths: Some(mult.tenths),
        thresholds,
        band: band_for(adjusted, &thresholds),
        remaining: Remaining {
            low: signed_gap(thresholds.low, adjusted),
            moderate: signed_gap(thresholds.moderate, adjusted),
            high: signed_gap(thresholds.high, adjusted),
            extreme: Some(signed_gap(deadly, adjusted)),
        },
        warnings: common_warnings(party, monsters, count),
    })
}

/// Adjusted-XP budget for a target band. Raw monster XP to buy is lower,
/// because the multiplier depends on the final monster count.
pub fn suggest_budget_2014(
    tables: &Tables2014,
    party: &[u8],
    target: Band,
) -> Result<u64, EncounterError> {
    tables.validate()?;
    validate_party(party)?;
    tables
        .party_thresholds(party)
        .for_band(target)
        .ok_or(EncounterError::NoBudgetForBand(target))
}
