//! Experience, hit points and the point buy table.

use crate::profile::AbilityGeneration;
use crate::state::{AbilityMethod, ClassSlot, HpRolls};
use serde::{Deserialize, Serialize};

/// Average value of a hit die (`die / 2 + 1`).
#[must_use]
pub fn average_die(die: u32) -> u32 {
    die / 2 + 1
}

/// Hit points a class progression provides (without the Constitution modifier).
///
/// `average` selects the fixed average table, otherwise the persisted `rolls`. Entry 0 of the main
/// class is always the die maximum; missing rolled entries fall back to the average.
#[must_use]
pub fn class_hit_points(die: u32, level: u32, is_main: bool, average: bool, rolls: &[u32]) -> i32 {
    class_hit_points_with(die, level, is_main, average, rolls, None)
}

/// [`class_hit_points`] with the fixed per-level value a ruleset's profile tables give the class
/// (`None` = the die average).
#[must_use]
pub fn class_hit_points_with(
    die: u32,
    level: u32,
    is_main: bool,
    average: bool,
    rolls: &[u32],
    fixed: Option<u32>,
) -> i32 {
    let mut total = 0u32;
    for i in 0..level as usize {
        let avg = fixed.unwrap_or_else(|| average_die(die));
        let v = if i == 0 && is_main {
            die
        } else if average {
            avg
        } else {
            rolls.get(i).copied().unwrap_or(avg).clamp(1, die.max(1))
        };
        total += v;
    }
    i32::try_from(total).unwrap_or(i32::MAX)
}

/// Stored rolls for a class progression.
#[must_use]
pub fn rolls_for<'a>(all: &'a [HpRolls], class: &ClassSlot) -> &'a [u32] {
    all.iter()
        .find(|r| &r.class == class)
        .map_or(&[], |r| r.rolls.as_slice())
}

/// Point buy cost of a base score (<=8 free; 9..=20 table; +2 per point above 20).
#[must_use]
pub fn point_cost(score: i32) -> i32 {
    match score {
        i32::MIN..=8 => 0,
        9..=13 => score - 8,
        14 => 7,
        15 => 9,
        16 => 11,
        17 => 13,
        18 => 15,
        19 => 17,
        20 => 19,
        n => 19 + 2 * (n - 20),
    }
}

/// Points left from `budget` for a set of base scores (negative = overspent).
#[must_use]
pub fn points_remaining(scores: &[i32; 6], budget: i32) -> i32 {
    budget - scores.iter().map(|s| point_cost(*s)).sum::<i32>()
}

/// Whether the scores are a permutation of the standard array.
#[must_use]
pub fn is_standard_array(scores: &[i32; 6], standard: &[i32; 6]) -> bool {
    let mut a = *scores;
    let mut b = *standard;
    a.sort_unstable();
    b.sort_unstable();
    a == b
}

/// A rough check that scores fit a generation method (the UI generates, the engine validates).
#[must_use]
pub fn method_accepts(
    method: AbilityMethod,
    scores: &[i32; 6],
    generation: &AbilityGeneration,
    budget: i32,
) -> bool {
    match method {
        AbilityMethod::StandardArray => is_standard_array(scores, &generation.standard_array),
        AbilityMethod::PointBuy => {
            let (lo, hi) = generation.point_buy_range;
            points_remaining(scores, budget) >= 0 && scores.iter().all(|s| (lo..=hi).contains(s))
        }
        AbilityMethod::Roll3d6 => scores.iter().all(|s| (3..=18).contains(s)),
        AbilityMethod::Roll4d6 => scores.iter().all(|s| (3..=18).contains(s)),
        AbilityMethod::Manual => true,
    }
}

/// Level for an XP total given the thresholds (1..=20).
#[must_use]
pub fn level_for_xp(thresholds: &[u32], xp: u32) -> u32 {
    let n = thresholds.iter().take_while(|t| xp >= **t).count();
    u32::try_from(n.clamp(1, 20)).unwrap_or(1)
}

/// Summary of the point buy state for the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PointBuy {
    /// Budget.
    pub budget: i32,
    /// Points spent.
    pub spent: i32,
    /// Points left (negative when overspent).
    pub remaining: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tv22_point_buy() {
        assert_eq!(points_remaining(&[15, 15, 15, 8, 8, 8], 27), 0);
        assert_eq!(points_remaining(&[15, 14, 13, 12, 10, 8], 27), 0);
        assert_eq!(points_remaining(&[10; 6], 27), 15);
        assert_eq!(points_remaining(&[13; 6], 27), -3);
        assert_eq!(
            [17, 18, 8, 9, 20, 21].map(point_cost),
            [13, 15, 0, 1, 19, 21]
        );
        let g = crate::profile::profile_for(crate::profile::default_ruleset()).ability_generation();
        let sa = &g.standard_array;
        assert!(is_standard_array(&[8, 10, 12, 13, 14, 15], sa));
        assert!(!is_standard_array(&[15, 15, 13, 12, 10, 8], sa));
        assert!(method_accepts(
            AbilityMethod::PointBuy,
            &[15, 14, 13, 12, 10, 8],
            &g,
            27
        ));
        assert!(!method_accepts(AbilityMethod::PointBuy, &[13; 6], &g, 27));
    }

    #[test]
    fn hit_points_average_and_rolled() {
        // Barbarian d12 level 3
        assert_eq!(class_hit_points(12, 3, true, true, &[]), 12 + 7 + 7);
        assert_eq!(class_hit_points(12, 3, true, false, &[12, 3, 10]), 25);
        // multiclass level 1 is an average
        assert_eq!(class_hit_points(6, 3, false, true, &[]), 4 * 3);
        assert_eq!(average_die(8), 5);
        assert_eq!(average_die(20), 11);
        // rolled entries are clamped to the die and missing ones use the average
        assert_eq!(class_hit_points(8, 3, true, false, &[8, 99]), 8 + 8 + 5);
    }

    #[test]
    fn xp_levels() {
        let xp = crate::profile::profile_for(crate::profile::default_ruleset()).xp_thresholds();
        assert_eq!(level_for_xp(&xp, 0), 1);
        assert_eq!(level_for_xp(&xp, 299), 1);
        assert_eq!(level_for_xp(&xp, 300), 2);
        assert_eq!(level_for_xp(&xp, 999_999), 20);
    }
}
