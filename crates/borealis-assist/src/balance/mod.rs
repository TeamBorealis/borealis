//! Deterministic balance report for homebrew: the element's own numbers are measured from
//! its JSON and compared with baselines computed from the enabled SRD packs.
//!
//! The model never touches these numbers; it may only write prose about the flags.

mod baselines;
mod engine_check;
mod measure;
mod report;

use serde::{Deserialize, Serialize};

pub use baselines::{Baselines, FEAT_ASI_FLOOR, FEAT_PROF_FLOOR, MIN_SAMPLES, RARITY_BONUS_FLOOR};
pub use engine_check::{load_into_engine, CharacterCheck, EngineCheck};
pub use measure::{damage_average, dice_expressions};
pub use report::balance_report;

/// How far outside the SRD range a value sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Within the SRD range.
    Ok,
    /// Upper half of the range, or no baseline exists.
    Low,
    /// Beyond the SRD maximum.
    Medium,
    /// Far beyond the SRD maximum.
    High,
}

/// One compared metric.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BalanceFlag {
    /// Metric id such as `spell.damage_avg`.
    pub metric: String,
    /// The homebrew value.
    pub value: f64,
    /// SRD `[min, max]` for the comparison group.
    pub baseline_range: [f64; 2],
    /// SRD median for the comparison group.
    pub baseline_median: f64,
    /// Verdict.
    pub severity: Severity,
    /// What was compared, with the group and sample size.
    pub note: String,
}

/// All metrics for one element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BalanceReport {
    /// Element id.
    pub element_id: String,
    /// Element name.
    pub name: String,
    /// Element type (`Spell`, `Feat`, ...).
    pub element_type: String,
    /// Every compared metric (including those within range).
    pub flags: Vec<BalanceFlag>,
    /// Things that could not be compared.
    pub notes: Vec<String>,
}

impl BalanceReport {
    /// Highest severity across the metrics (`Ok` when there are none).
    pub fn max_severity(&self) -> Severity {
        self.flags
            .iter()
            .map(|f| f.severity)
            .max()
            .unwrap_or(Severity::Ok)
    }

    /// True when any metric is `High`.
    pub fn has_high(&self) -> bool {
        self.max_severity() == Severity::High
    }

    /// Metrics outside the SRD range or without a baseline.
    pub fn flagged(&self) -> impl Iterator<Item = &BalanceFlag> {
        self.flags.iter().filter(|f| f.severity > Severity::Ok)
    }
}

/// Min, median and max of a sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Summary {
    /// Number of samples.
    pub n: usize,
    /// Smallest sample.
    pub min: f64,
    /// Median sample.
    pub median: f64,
    /// Largest sample.
    pub max: f64,
}

impl Summary {
    /// Summarise `values` (all zero when empty).
    pub fn of(values: &[f64]) -> Self {
        let mut v = values.to_vec();
        v.sort_by(f64::total_cmp);
        let Some((&min, &max)) = v.first().zip(v.last()) else {
            return Self {
                n: 0,
                min: 0.0,
                median: 0.0,
                max: 0.0,
            };
        };
        let mid = v.len() / 2;
        let median = if v.len() % 2 == 1 {
            v[mid]
        } else {
            (v[mid - 1] + v[mid]) / 2.0
        };
        Self {
            n: v.len(),
            min,
            median,
            max,
        }
    }

    /// Raise the ceiling to at least `floor` (thin baselines).
    #[must_use]
    pub fn with_floor(mut self, floor: f64) -> Self {
        if self.n == 0 {
            self.min = 0.0;
        }
        self.max = self.max.max(floor);
        self.median = self.median.min(self.max);
        self
    }
}

/// Multiple of the SRD maximum from which a value is `High`.
const HIGH_FACTOR: f64 = 1.5;
/// Multiple of the SRD minimum below which a two-sided metric is flagged `Medium`.
const LOW_FACTOR: f64 = 0.5;
/// Multiple of the median above which a value in range is `Low`.
const UPPER_HALF_FACTOR: f64 = 1.5;

/// Grade `value` against `s`. Two-sided metrics also flag values far below the minimum.
pub fn grade(value: f64, s: &Summary, two_sided: bool) -> Severity {
    if s.n == 0 {
        return Severity::Low;
    }
    if value > s.max {
        return if value > s.max * HIGH_FACTOR && value >= s.max + 1.0 {
            Severity::High
        } else {
            Severity::Medium
        };
    }
    if two_sided && value < s.min * LOW_FACTOR {
        return Severity::Medium;
    }
    if value > s.median * UPPER_HALF_FACTOR && value > s.median {
        return Severity::Low;
    }
    Severity::Ok
}

/// Build a [`BalanceFlag`] from a graded comparison.
pub fn flag(metric: &str, value: f64, s: &Summary, two_sided: bool, context: &str) -> BalanceFlag {
    let severity = grade(value, s, two_sided);
    let note = if s.n == 0 {
        format!("no SRD baseline for {context}; value not graded")
    } else {
        format!("{context}: {} SRD sample(s), median {:.1}", s.n, s.median)
    };
    BalanceFlag {
        metric: metric.to_string(),
        value,
        baseline_range: [s.min, s.max],
        baseline_median: s.median,
        severity,
        note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grading_thresholds() {
        let s = Summary::of(&[2.0, 4.0, 6.0, 10.0]);
        assert_eq!(grade(5.0, &s, false), Severity::Ok);
        assert_eq!(grade(9.0, &s, false), Severity::Low);
        assert_eq!(grade(12.0, &s, false), Severity::Medium);
        assert_eq!(grade(40.0, &s, false), Severity::High);
        assert_eq!(grade(0.5, &s, true), Severity::Medium);
        assert_eq!(grade(1.0, &Summary::of(&[]), false), Severity::Low);
    }
}
