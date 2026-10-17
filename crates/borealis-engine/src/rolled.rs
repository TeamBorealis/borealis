//! Rolled resource maximums (an item "has 1d6 + 1 charges"): a `resource` whose `max` is
//! `{"type": "rolled", "dice": "1d6+1"}`. The maximum is rolled (or entered by the player) once,
//! when the item is gained, and stored on the inventory line (`InventoryItem.rolled_max`, keyed
//! by resource id); re-rolling or editing it is a command. A line without a stored value (a
//! character saved before the item had one) uses the average, rounded down.

use crate::state::InventoryItem;
use crate::universe::{UElem, Universe};
use borealis_content::Rule;
use serde_json::Value;
use std::hash::BuildHasher;

/// `max.type` of a rolled maximum.
pub const ROLLED: &str = "rolled";
/// Most dice one expression may roll (guards against malformed content).
const MAX_DICE: u32 = 100;
/// Largest die.
const MAX_SIDES: u32 = 1000;

/// A dice expression `NdS+B` (`N` may be 0 for a flat number).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dice {
    /// Number of dice.
    pub count: u32,
    /// Sides of each die.
    pub sides: u32,
    /// Flat bonus (may be negative).
    pub bonus: i32,
}

impl Dice {
    /// Parse `1d6+1`, `1d6 + 1`, `2d4`, `d8`, `1d20-1` or a flat `4`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let t = t.to_ascii_lowercase();
        let (dice, bonus) = match t.find(['+', '-']) {
            Some(i) if i > 0 => {
                let b: i32 = t[i + 1..].parse().ok()?;
                (&t[..i], if &t[i..=i] == "-" { -b } else { b })
            }
            _ => (t.as_str(), 0),
        };
        let Some((n, s)) = dice.split_once('d') else {
            let flat: i32 = dice.parse().ok()?;
            return Some(Self {
                count: 0,
                sides: 0,
                bonus: flat + bonus,
            });
        };
        let count = if n.is_empty() { 1 } else { n.parse().ok()? };
        let sides: u32 = s.parse().ok()?;
        ((1..=MAX_DICE).contains(&count) && (1..=MAX_SIDES).contains(&sides)).then_some(Self {
            count,
            sides,
            bonus,
        })
    }

    /// Lowest total (never below 0).
    #[must_use]
    pub fn min(&self) -> u32 {
        clamp(i64::from(self.count) + i64::from(self.bonus))
    }

    /// Highest total.
    #[must_use]
    pub fn max(&self) -> u32 {
        clamp(i64::from(self.count) * i64::from(self.sides) + i64::from(self.bonus))
    }

    /// Average total, rounded down.
    #[must_use]
    pub fn average(&self) -> u32 {
        clamp((i64::from(self.count) * (i64::from(self.sides) + 1)) / 2 + i64::from(self.bonus))
    }

    /// Roll the dice.
    #[must_use]
    pub fn roll(&self) -> u32 {
        let state = std::collections::hash_map::RandomState::new();
        let total: i64 = (0..self.count)
            .map(|i| {
                let r = state.hash_one(i) % u64::from(self.sides);
                i64::try_from(r).unwrap_or(0) + 1
            })
            .sum();
        clamp(total + i64::from(self.bonus))
    }
}

fn clamp(n: i64) -> u32 {
    u32::try_from(n.max(0)).unwrap_or(u32::MAX)
}

/// The dice of a rolled `resource.max`, if it is one.
#[must_use]
pub fn dice_of(max: &Value) -> Option<Dice> {
    if max.get("type").and_then(Value::as_str) != Some(ROLLED) {
        return None;
    }
    max.get("dice")
        .and_then(Value::as_str)
        .and_then(Dice::parse)
}

/// `(resource id, dice)` of every rolled resource an element declares.
#[must_use]
pub fn rolled_resources(e: &UElem) -> Vec<(String, Dice)> {
    e.el.rules
        .iter()
        .filter_map(|r| match r {
            Rule::Custom(c) if c.kind == "resource" => {
                let id = c.fields.get("id")?.as_str()?;
                Some((id.to_string(), dice_of(c.fields.get("max")?)?))
            }
            _ => None,
        })
        .collect()
}

/// The rolled resources of an inventory line (base item, then its adorner).
#[must_use]
pub fn line_resources(uni: &Universe, item: &InventoryItem) -> Vec<(String, Dice)> {
    [Some(item.item.as_str()), item.adorner.as_deref()]
        .into_iter()
        .flatten()
        .filter_map(|id| uni.get(id))
        .flat_map(rolled_resources)
        .collect()
}

/// A copy of `item` with a rolled maximum for every rolled resource that has none yet (the
/// moment the item or its adorner is gained). Stored values of the line's resources are kept,
/// values of resources the line no longer has (a replaced adorner) are dropped.
#[must_use]
pub fn with_rolls(uni: &Universe, item: &InventoryItem) -> InventoryItem {
    let resources = line_resources(uni, item);
    let mut next = item.clone();
    next.rolled_max
        .retain(|id, _| resources.iter().any(|(r, _)| r == id));
    for (id, dice) in resources {
        next.rolled_max.entry(id).or_insert_with(|| dice.roll());
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dice_expressions_parse_with_their_bounds() {
        let d = Dice::parse("1d6 + 1").expect("parses");
        assert_eq!((d.count, d.sides, d.bonus), (1, 6, 1));
        assert_eq!((d.min(), d.average(), d.max()), (2, 4, 7));
        let d = Dice::parse("2d4").expect("parses");
        assert_eq!((d.min(), d.average(), d.max()), (2, 5, 8));
        assert_eq!(Dice::parse("d8").map(|d| d.max()), Some(8));
        assert_eq!(
            Dice::parse("1d20-1").map(|d| (d.min(), d.max())),
            Some((0, 19))
        );
        assert_eq!(
            Dice::parse("3").map(|d| (d.min(), d.max(), d.roll())),
            Some((3, 3, 3))
        );
        for bad in ["", "abc", "1d0", "0d6", "1d6+x", "1001d6"] {
            assert_eq!(Dice::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn rolls_stay_within_the_bounds_and_vary() {
        let d = Dice::parse("1d6+1").expect("parses");
        let rolls: Vec<u32> = (0..200).map(|_| d.roll()).collect();
        assert!(rolls.iter().all(|r| (2..=7).contains(r)));
        let distinct: std::collections::BTreeSet<u32> = rolls.into_iter().collect();
        assert!(distinct.len() > 3, "rolls vary: {distinct:?}");
    }
}
