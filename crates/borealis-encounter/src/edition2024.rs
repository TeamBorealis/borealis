//! 2024 encounter maths: SRD 5.2.1 "Combat Encounter Difficulty" (p. 202).

use crate::error::EncounterError;
use crate::report::{
    common_warnings, signed_gap, Band, EncounterReport, Remaining, Ruleset, Thresholds,
};
use crate::{monster_count, raw_xp, validate_party, MonsterGroup};

/// SRD 5.2.1 p. 202, "XP Budget per Character": `[Low, Moderate, High]` for
/// party level 1..=20 (row 0 = level 1).
pub const XP_BUDGET_PER_CHARACTER: [[u64; 3]; 20] = [
    [50, 75, 100],
    [100, 150, 200],
    [150, 225, 400],
    [250, 375, 500],
    [500, 750, 1_100],
    [600, 1_000, 1_400],
    [750, 1_300, 1_700],
    [1_000, 1_700, 2_100],
    [1_300, 2_000, 2_600],
    [1_600, 2_300, 3_100],
    [1_900, 2_900, 4_100],
    [2_200, 3_700, 4_700],
    [2_600, 4_200, 5_400],
    [2_900, 4_900, 6_200],
    [3_300, 5_400, 7_800],
    [3_800, 6_100, 9_800],
    [4_500, 7_200, 11_700],
    [5_000, 8_700, 14_200],
    [5_500, 10_700, 17_200],
    [6_400, 13_200, 22_000],
];

/// Per-character budget for a level (1..=20), `[low, moderate, high]`.
pub fn budget_per_character(level: u8) -> Option<[u64; 3]> {
    let idx = usize::from(level).checked_sub(1)?;
    XP_BUDGET_PER_CHARACTER.get(idx).copied()
}

/// Party budgets: the sum of each character's per-character budget (the SRD
/// multiplies by party size; summing generalises to mixed levels).
pub fn party_thresholds(party: &[u8]) -> Result<Thresholds, EncounterError> {
    validate_party(party)?;
    let mut sum = [0u64; 3];
    for &level in party {
        let row = budget_per_character(level).ok_or(EncounterError::InvalidLevel(level))?;
        for (acc, v) in sum.iter_mut().zip(row) {
            *acc += v;
        }
    }
    Ok(Thresholds {
        low: sum[0],
        moderate: sum[1],
        high: sum[2],
        extreme: None,
    })
}

/// Band for a total: the lowest tier whose budget covers it (0 XP is
/// Trivial); beyond the High budget is [`Band::Extreme`].
pub fn band_for(total_xp: u64, t: &Thresholds) -> Band {
    if total_xp == 0 {
        Band::Trivial
    } else if total_xp <= t.low {
        Band::Low
    } else if total_xp <= t.moderate {
        Band::Moderate
    } else if total_xp <= t.high {
        Band::High
    } else {
        Band::Extreme
    }
}

/// Evaluate an encounter under the 2024 rules.
pub fn evaluate(
    party: &[u8],
    monsters: &[MonsterGroup],
) -> Result<EncounterReport, EncounterError> {
    let thresholds = party_thresholds(party)?;
    let total_xp = raw_xp(monsters);
    let count = monster_count(monsters);
    Ok(EncounterReport {
        ruleset: Ruleset::Edition2024,
        party_size: party.len(),
        monster_count: count,
        total_xp,
        adjusted_xp: total_xp,
        multiplier_tenths: None,
        thresholds,
        band: band_for(total_xp, &thresholds),
        remaining: Remaining {
            low: signed_gap(thresholds.low, total_xp),
            moderate: signed_gap(thresholds.moderate, total_xp),
            high: signed_gap(thresholds.high, total_xp),
            extreme: None,
        },
        warnings: common_warnings(party, monsters, count),
    })
}

/// XP to spend for a target band (Trivial = 0; Extreme has no budget).
pub fn suggest_budget(party: &[u8], target: Band) -> Result<u64, EncounterError> {
    let t = party_thresholds(party)?;
    t.for_band(target)
        .ok_or(EncounterError::NoBudgetForBand(target))
}
