//! Current and temporary hit points and hit dice on the Play tab (B-194, decision PL1; absorbs
//! B-105, GitHub #1). The saved part is in [`PlayState`]: hit points lost, temporary hit points
//! and spent hit dice per class. Everything shown is read against a derive (the maximum, the
//! classes and their dice), so a level gained or lost moves the numbers with it.
//!
//! The rules are the same in both editions except where [`HitDiceRules`] says otherwise:
//! damage comes off temporary hit points first and hit points never drop below 0 (SRD 5.1 and
//! 5.2.1 "Temporary Hit Points"); healing never goes above the maximum; temporary hit points don't
//! stack, the player picks which to keep (so a new value replaces the old one); a Long Rest
//! restores every hit point and ends temporary hit points.

use crate::profile::HitDiceRules;
use crate::snapshot::CharacterSnapshot;
use crate::state::PlayState;
use serde::Serialize;
use std::collections::BTreeMap;

/// Largest damage, healing or temporary hit points one change takes.
pub const MAX_HP_CHANGE: u32 = 9_999;
/// Ability index of Constitution.
const CON: usize = 2;

/// Hit points as the Play tab shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct HitPointsView {
    /// Hit point maximum (the derive's).
    pub max: u32,
    /// Current hit points, 0..=max.
    pub current: u32,
    /// Temporary hit points.
    pub temp: u32,
}

/// The hit dice of one class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HitDiceUse {
    /// Class element id (the key of [`PlayState::hit_dice_used`]).
    pub class_id: String,
    /// Class name.
    pub class_name: String,
    /// Faces of the die.
    pub die: u32,
    /// Dice the class gives (its level).
    pub total: u32,
    /// Spent dice, at most `total`.
    pub used: u32,
}

/// The hit point maximum of `snap` (0 for a negative one).
#[must_use]
pub fn max_hp(snap: &CharacterSnapshot) -> u32 {
    u32::try_from(snap.hit_points).unwrap_or(0)
}

/// The Constitution modifier of `snap`.
#[must_use]
pub fn con_modifier(snap: &CharacterSnapshot) -> i32 {
    snap.abilities.get(CON).map_or(0, |a| a.modifier)
}

/// Current, maximum and temporary hit points of `play` against `snap`.
#[must_use]
pub fn hit_points(play: &PlayState, snap: &CharacterSnapshot) -> HitPointsView {
    let max = max_hp(snap);
    HitPointsView {
        max,
        current: max.saturating_sub(play.hp_lost),
        temp: play.temp_hp,
    }
}

/// Hit dice per class with a hit die, the spent count clamped to the class level.
#[must_use]
pub fn hit_dice(play: &PlayState, snap: &CharacterSnapshot) -> Vec<HitDiceUse> {
    snap.classes
        .iter()
        .filter_map(|c| {
            let die = c.hit_die.filter(|d| *d > 0)?;
            let class = c.class.as_ref()?;
            (c.level > 0).then(|| HitDiceUse {
                class_id: class.id.clone(),
                class_name: class.name.clone(),
                die,
                total: c.level,
                used: play
                    .hit_dice_used
                    .get(&class.id)
                    .copied()
                    .unwrap_or(0)
                    .min(c.level),
            })
        })
        .collect()
}

/// `play` after `amount` damage: temporary hit points go first, the rest comes off hit points,
/// which stop at 0.
#[must_use]
pub fn after_damage(play: &PlayState, max: u32, amount: u32) -> PlayState {
    let from_temp = amount.min(play.temp_hp);
    let lost = play
        .hp_lost
        .min(max)
        .saturating_add(amount - from_temp)
        .min(max);
    PlayState {
        temp_hp: play.temp_hp - from_temp,
        hp_lost: lost,
        ..play.clone()
    }
}

/// `play` after `amount` healing, never above the maximum. Temporary hit points are untouched.
#[must_use]
pub fn after_heal(play: &PlayState, max: u32, amount: u32) -> PlayState {
    PlayState {
        hp_lost: play.hp_lost.min(max).saturating_sub(amount),
        ..play.clone()
    }
}

/// Hit points one spent hit die heals: the roll plus the Constitution modifier, at least the
/// edition's minimum (and never below 0).
#[must_use]
pub fn hit_die_healing(roll: u32, con: i32, rules: HitDiceRules) -> u32 {
    let total = i64::from(roll) + i64::from(con);
    u32::try_from(total.max(i64::from(rules.spend_minimum)).max(0)).unwrap_or(0)
}

/// The hit point part of `play` after a Long Rest: every hit point back, temporary hit points
/// gone, and the spent hit dice the edition gives back (all in 2024; up to half the total, at
/// least one, in 2014).
#[must_use]
pub fn after_long_rest(
    play: &PlayState,
    snap: &CharacterSnapshot,
    rules: HitDiceRules,
) -> PlayState {
    let hit_dice_used = if rules.long_rest_regains_all {
        BTreeMap::new()
    } else {
        regain_half(&hit_dice(play, snap))
    };
    PlayState {
        hp_lost: 0,
        temp_hp: 0,
        hit_dice_used,
        ..play.clone()
    }
}

/// Spent dice left after a 2014 Long Rest: up to half the total number of hit dice (at least one)
/// come back, the largest dice first (the player chooses; the largest heal the most), then by
/// class name. Spent counts of classes the character no longer has are dropped.
fn regain_half(dice: &[HitDiceUse]) -> BTreeMap<String, u32> {
    let total: u32 = dice.iter().map(|d| d.total).sum();
    let mut left = (total / 2).max(1);
    let mut order: Vec<&HitDiceUse> = dice.iter().collect();
    order.sort_by(|a, b| {
        b.die
            .cmp(&a.die)
            .then_with(|| a.class_name.cmp(&b.class_name))
    });
    let mut out = BTreeMap::new();
    for d in order {
        let back = d.used.min(left);
        left -= back;
        if d.used > back {
            out.insert(d.class_id.clone(), d.used - back);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULES_2014: HitDiceRules = HitDiceRules {
        long_rest_regains_all: false,
        spend_minimum: 0,
    };
    const RULES_2024: HitDiceRules = HitDiceRules {
        long_rest_regains_all: true,
        spend_minimum: 1,
    };

    fn play(lost: u32, temp: u32) -> PlayState {
        PlayState {
            hp_lost: lost,
            temp_hp: temp,
            ..PlayState::default()
        }
    }

    fn die(id: &str, die: u32, total: u32, used: u32) -> HitDiceUse {
        HitDiceUse {
            class_id: id.into(),
            class_name: id.into(),
            die,
            total,
            used,
        }
    }

    #[test]
    fn damage_takes_temporary_hit_points_first() {
        // SRD: 5 temporary hit points and 7 damage lose those points and then 2 hit points
        let p = after_damage(&play(0, 5), 20, 7);
        assert_eq!((p.temp_hp, p.hp_lost), (0, 2));
        let p = after_damage(&play(3, 5), 20, 4);
        assert_eq!((p.temp_hp, p.hp_lost), (1, 3));
    }

    #[test]
    fn hit_points_stop_at_zero_and_healing_at_the_maximum() {
        let p = after_damage(&play(15, 0), 20, 100);
        assert_eq!(p.hp_lost, 20);
        let p = after_damage(&play(25, 0), 20, 1);
        assert_eq!(
            p.hp_lost, 20,
            "a loss saved above a lower maximum is clamped"
        );
        assert_eq!(after_heal(&play(5, 3), 20, 2).hp_lost, 3);
        let healed = after_heal(&play(5, 3), 20, 50);
        assert_eq!((healed.hp_lost, healed.temp_hp), (0, 3));
        assert_eq!(after_heal(&play(30, 0), 20, 1).hp_lost, 19);
    }

    #[test]
    fn a_spent_die_heals_its_roll_and_the_constitution_modifier_with_the_edition_minimum() {
        assert_eq!(hit_die_healing(6, 2, RULES_2014), 8);
        assert_eq!(hit_die_healing(1, -1, RULES_2014), 0);
        assert_eq!(hit_die_healing(1, -3, RULES_2014), 0);
        assert_eq!(hit_die_healing(1, -1, RULES_2024), 1);
        assert_eq!(hit_die_healing(1, -3, RULES_2024), 1);
        assert_eq!(hit_die_healing(4, 0, RULES_2024), 4);
    }

    #[test]
    fn a_2014_long_rest_regains_half_the_total_largest_dice_first() {
        // SRD 5.1: "if a character has eight Hit Dice, he or she can regain four"
        let back = regain_half(&[die("fighter", 10, 8, 8)]);
        assert_eq!(back.get("fighter"), Some(&4));
        // at least one die, even for a level 1 character
        assert!(regain_half(&[die("wizard", 6, 1, 1)]).is_empty());
        // a level 3 character regains one of three
        assert_eq!(regain_half(&[die("rogue", 8, 3, 3)]).get("rogue"), Some(&2));
        // multiclass: five d10 and five d8 spent, five come back, the d10s first
        let back = regain_half(&[die("cleric", 8, 5, 5), die("paladin", 10, 5, 5)]);
        assert_eq!(back.get("paladin"), None);
        assert_eq!(back.get("cleric"), Some(&5));
        // fewer spent than may come back: all of them return
        assert!(regain_half(&[die("cleric", 8, 5, 1), die("paladin", 10, 5, 1)]).is_empty());
    }
}
