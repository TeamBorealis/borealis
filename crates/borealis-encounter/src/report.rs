//! Report types shared by both rulesets.

use serde::{Deserialize, Serialize};

use crate::cr::Cr;
use crate::MonsterGroup;

/// Which edition's maths to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Ruleset {
    /// 2014 rules (needs caller-supplied tables; not in SRD 5.1).
    Edition2014,
    /// 2024 rules (SRD 5.2.1 XP Budget per Character).
    Edition2024,
}

/// Difficulty band. 2014 mapping: Low = Easy, Moderate = Medium, High = Hard,
/// Extreme = Deadly. 2024: Low/Moderate/High per SRD 5.2.1; Extreme means
/// "over the High budget" (the SRD names no tier above High).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Band {
    /// Below the lowest tier (or empty).
    Trivial,
    /// Easy / Low.
    Low,
    /// Medium / Moderate.
    Moderate,
    /// Hard / High.
    High,
    /// Deadly (2014) or beyond the High budget (2024).
    Extreme,
}

/// Party-total XP thresholds for each band (`extreme` is 2014 only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thresholds {
    /// Low / Easy.
    pub low: u64,
    /// Moderate / Medium.
    pub moderate: u64,
    /// High / Hard.
    pub high: u64,
    /// Deadly (2014); `None` in 2024.
    pub extreme: Option<u64>,
}

impl Thresholds {
    /// Threshold for a band, if the ruleset has one.
    pub fn for_band(&self, band: Band) -> Option<u64> {
        match band {
            Band::Trivial => Some(0),
            Band::Low => Some(self.low),
            Band::Moderate => Some(self.moderate),
            Band::High => Some(self.high),
            Band::Extreme => self.extreme,
        }
    }
}

/// Signed XP left until each threshold (negative = already past it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Remaining {
    /// To Low / Easy.
    pub low: i64,
    /// To Moderate / Medium.
    pub moderate: i64,
    /// To High / Hard.
    pub high: i64,
    /// To Deadly (2014 only).
    pub extreme: Option<i64>,
}

/// Something the DM should know about the encounter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Warning {
    /// SRD 5.2.1 p. 203 "Powerful Creatures": CR above the party's level.
    MonsterAbovePartyLevel {
        /// The offending CR.
        cr: Cr,
        /// Highest level in the party.
        highest_level: u8,
    },
    /// SRD 5.2.1 p. 203 "Many Creatures": more than two per character.
    ManyCreatures {
        /// Monster count.
        monsters: u32,
        /// Party size.
        party_size: usize,
    },
    /// SRD 5.2.1 p. 203 "Number of Stat Blocks": more than three kinds.
    ManyStatBlocks {
        /// Distinct CR groups supplied (proxy for stat blocks).
        groups: usize,
    },
    /// SRD 5.2.1 p. 203 "CR 0 Creatures": worth 0 XP, use sparingly.
    ZeroCrCreatures,
    /// HEURISTIC (not an SRD rule): the sides' action counts are lopsided.
    ActionEconomyHeuristic {
        /// Monster count.
        monsters: u32,
        /// Party size.
        party_size: usize,
    },
}

impl Warning {
    /// True when this warning is our own rule of thumb, not SRD text.
    pub fn is_heuristic(&self) -> bool {
        matches!(self, Self::ActionEconomyHeuristic { .. })
    }
}

/// Result of evaluating an encounter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncounterReport {
    /// Ruleset used.
    pub ruleset: Ruleset,
    /// Number of characters.
    pub party_size: usize,
    /// Total monsters.
    pub monster_count: u32,
    /// Sum of monster XP.
    pub total_xp: u64,
    /// 2014: XP after multiplier (tenths applied); 2024: equals `total_xp`.
    pub adjusted_xp: u64,
    /// 2014 multiplier in tenths (15 = x1.5); `None` for 2024.
    pub multiplier_tenths: Option<u32>,
    /// Party thresholds / budgets.
    pub thresholds: Thresholds,
    /// Resulting band.
    pub band: Band,
    /// XP left to each threshold, measured against `adjusted_xp`.
    pub remaining: Remaining,
    /// Warnings, SRD-based first, heuristics last.
    pub warnings: Vec<Warning>,
}

/// Warnings common to both rulesets.
pub(crate) fn common_warnings(party: &[u8], monsters: &[MonsterGroup], count: u32) -> Vec<Warning> {
    let size = party.len();
    let highest = party.iter().copied().max().unwrap_or(1);
    let mut out = Vec::new();
    let mut seen = Vec::new();
    for MonsterGroup { cr, .. } in monsters.iter().filter(|g| g.count > 0) {
        if cr.exceeds_level(highest) && !seen.contains(cr) {
            seen.push(*cr);
            out.push(Warning::MonsterAbovePartyLevel {
                cr: *cr,
                highest_level: highest,
            });
        }
    }
    if usize::try_from(count).unwrap_or(usize::MAX) > size.saturating_mul(2) {
        out.push(Warning::ManyCreatures {
            monsters: count,
            party_size: size,
        });
    }
    let groups = monsters.iter().filter(|g| g.count > 0).count();
    if groups > 3 {
        out.push(Warning::ManyStatBlocks { groups });
    }
    if monsters.iter().any(|g| g.cr == Cr::Zero && g.count > 0) {
        out.push(Warning::ZeroCrCreatures);
    }
    // Heuristic: fewer than half as many monsters as PCs, or more than twice.
    let c = usize::try_from(count).unwrap_or(usize::MAX);
    if count > 0 && (c.saturating_mul(2) < size || c > size.saturating_mul(2)) {
        out.push(Warning::ActionEconomyHeuristic {
            monsters: count,
            party_size: size,
        });
    }
    out
}

/// Signed difference `threshold - xp`.
pub(crate) fn signed_gap(threshold: u64, xp: u64) -> i64 {
    let t = i128::from(threshold) - i128::from(xp);
    i64::try_from(t).unwrap_or(if t < 0 { i64::MIN } else { i64::MAX })
}
