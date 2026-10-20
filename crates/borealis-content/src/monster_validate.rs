//! Consistency checks for [`MonsterData`](crate::monster::MonsterData).

use crate::monster::{MonsterData, MonsterIssue};
use crate::monster_types::{Ability, Attack, Feature, Skill, Usage};

/// Parse `NdS`, `NdS+M` or `NdS-M` into `(n, s, m)`.
#[must_use]
pub fn parse_dice(text: &str) -> Option<(u32, u32, i32)> {
    let (count, rest) = text.split_once('d')?;
    let n: u32 = count.parse().ok()?;
    let (sides, modifier) = match rest.find(['+', '-']) {
        Some(i) => (&rest[..i], rest[i..].parse::<i32>().ok()?),
        None => (rest, 0),
    };
    let s: u32 = sides.parse().ok()?;
    (n >= 1 && s >= 2).then_some((n, s, modifier))
}

/// Average of a dice expression, rounded down (as printed in stat blocks).
#[must_use]
pub fn dice_average(text: &str) -> Option<i32> {
    let (n, s, m) = parse_dice(text)?;
    let doubled = i32::try_from(n * (s + 1)).ok()?;
    Some(doubled / 2 + m)
}

const MAX_SCORE: u8 = 30;

pub(crate) fn validate(m: &MonsterData) -> Vec<MonsterIssue> {
    let mut out = Vec::new();
    let mut bad = |field: &str, msg: String| out.push(MonsterIssue::new(field, msg));
    if m.size.is_empty() {
        bad("size", "at least one size is required".into());
    }
    if m.alignment.trim().is_empty() {
        bad("alignment", "alignment is empty".into());
    }
    defenses(m, &mut bad);
    for ability in Ability::ALL {
        let score = m.abilities.score(*ability);
        if score == 0 || score > MAX_SCORE {
            bad(
                &format!("abilities.{ability}"),
                format!("score {score} outside 1..={MAX_SCORE}"),
            );
        }
    }
    if m.speed.walk.is_none()
        && m.speed.fly.is_none()
        && m.speed.swim.is_none()
        && m.speed.climb.is_none()
        && m.speed.burrow.is_none()
    {
        bad("speed", "no speed given".into());
    }
    for clause in m
        .damage_vulnerabilities
        .iter()
        .chain(&m.damage_resistances)
        .chain(&m.damage_immunities)
    {
        if clause.types.is_empty() && clause.text.is_none() {
            bad("damage", "empty damage clause".into());
        }
    }
    challenge(m, &mut bad);
    if m.gear.iter().any(|g| g.trim().is_empty()) {
        bad("gear", "empty gear entry".into());
    }
    for (section, list) in [
        ("traits", &m.traits),
        ("actions", &m.actions),
        ("bonus_actions", &m.bonus_actions),
        ("reactions", &m.reactions),
    ] {
        check_features(section, list, &mut out);
    }
    if let Some(l) = &m.legendary {
        check_features("legendary.actions", &l.actions, &mut out);
        if !l.actions.is_empty() && l.per_round.unwrap_or(0) == 0 {
            out.push(MonsterIssue::new(
                "legendary.per_round",
                "legendary actions need a per-round count",
            ));
        }
        if l.lair_uses
            .is_some_and(|lair| lair < l.per_round.unwrap_or(0))
        {
            out.push(MonsterIssue::new(
                "legendary.lair_uses",
                "lair uses are below the normal uses",
            ));
        }
        if l.actions.iter().any(|a| a.cost == Some(0)) {
            out.push(MonsterIssue::new("legendary.actions", "cost 0"));
        }
    }
    out
}

fn check_features(section: &str, list: &[Feature], out: &mut Vec<MonsterIssue>) {
    for (i, f) in list.iter().enumerate() {
        let at = format!("{section}[{i}]");
        if f.name.trim().is_empty() || f.text.trim().is_empty() {
            out.push(MonsterIssue::new(&at, "name and text must not be empty"));
        }
        if let Some(Usage::Recharge { min, max }) = f.usage {
            if !(2..=6).contains(&min) || max < min || max > 6 {
                out.push(MonsterIssue::new(
                    &at,
                    format!("recharge {min}-{max} is not a d6 range"),
                ));
            }
        }
        if let Some(Usage::PerDayInLair { normal, lair }) = f.usage {
            if normal == 0 || lair < normal {
                out.push(MonsterIssue::new(
                    &at,
                    format!("per-day uses {normal} / {lair} in lair are inconsistent"),
                ));
            }
        }
        if let Some(sp) = &f.spellcasting {
            if sp.lists.iter().any(|l| l.spells.is_empty()) {
                out.push(MonsterIssue::new(&at, "empty spell list"));
            }
        }
        let attack_damage = f.attack.iter().flat_map(|a| a.damage.iter());
        let save_damage = f.save.iter().flat_map(|s| s.damage.iter());
        for (j, d) in attack_damage.chain(save_damage).enumerate() {
            if let Some(dice) = d.dice.as_deref().filter(|x| parse_dice(x).is_none()) {
                out.push(MonsterIssue::new(
                    format!("{at}.damage[{j}]"),
                    format!("bad dice {dice:?}"),
                ));
            }
        }
    }
}

/// Numbers a printed stat block may set apart from the standard tables, kept as given and
/// reported (decision HB4): an XP other than the standard XP of the challenge rating.
pub(crate) fn warnings(m: &MonsterData) -> Vec<MonsterIssue> {
    let (Some(cr), Some(xp)) = (m.cr, m.xp) else {
        return Vec::new();
    };
    if cr.accepts_xp(xp) {
        return Vec::new();
    }
    vec![MonsterIssue::new(
        "xp",
        format!(
            "printed XP {xp} differs from the standard {} XP for CR {cr}; the printed value is kept",
            cr.standard_xp(),
        ),
    )]
}

/// Armor class and hit points (decision IM12a): each is a number or, when printed as a formula,
/// a non-empty note standing in for it, never both.
fn defenses(m: &MonsterData, bad: &mut impl FnMut(&str, String)) {
    let has_note = |note: &Option<String>| note.as_deref().is_some_and(|n| !n.trim().is_empty());
    if m.ac.is_empty() {
        if !has_note(&m.ac_note) {
            bad("ac", "needs an armor class or a note for it".into());
        }
    } else {
        if m.ac.iter().any(|a| a.value == 0) {
            bad("ac", "every armor class must be greater than zero".into());
        }
        if m.ac_note.is_some() {
            bad(
                "ac_note",
                "an armor class note next to an armor class".into(),
            );
        }
    }
    let Some(hp) = &m.hp else {
        if !has_note(&m.hp_note) {
            bad("hp", "needs hit points or a note for them".into());
        }
        return;
    };
    if m.hp_note.is_some() {
        bad("hp_note", "a hit point note next to hit points".into());
    }
    if hp.average == 0 {
        bad(
            "hp.average",
            "hit point average must be greater than zero".into(),
        );
    }
    if let Some(f) = &hp.formula {
        if parse_dice(f).is_none() {
            bad("hp.formula", format!("{f:?} is not NdS[+-M]"));
        }
    }
}

/// Challenge rating, XP and proficiency bonus agree (decision IM12): with a CR, the XP and the
/// CR's proficiency bonus are present and no note stands in for it; without one there is no XP.
fn challenge(m: &MonsterData, bad: &mut impl FnMut(&str, String)) {
    let Some(cr) = m.cr else {
        if m.xp.is_some() || m.xp_lair.is_some() {
            bad("xp", "XP without a challenge rating".into());
        }
        return;
    };
    if m.cr_note.is_some() {
        bad(
            "cr_note",
            "a challenge rating note next to a challenge rating".into(),
        );
    }
    match m.proficiency_bonus {
        Some(pb) if pb == cr.proficiency_bonus() => {}
        Some(pb) => bad(
            "proficiency_bonus",
            format!(
                "{pb} does not match CR {cr} (expected {})",
                cr.proficiency_bonus()
            ),
        ),
        None => bad(
            "proficiency_bonus",
            format!("missing (CR {cr} gives {})", cr.proficiency_bonus()),
        ),
    }
    match (m.xp, m.xp_lair) {
        (None, _) => bad("xp", format!("missing for CR {cr}")),
        (Some(xp), Some(lair)) if lair < xp => {
            bad("xp_lair", "lair XP is below the XP outside the lair".into());
        }
        _ => {}
    }
}

pub(crate) fn soft_checks(m: &MonsterData) -> Vec<MonsterIssue> {
    let mut out = Vec::new();
    if let Some((average, f)) =
        m.hp.as_ref()
            .and_then(|hp| hp.formula.as_ref().map(|f| (hp.average, f)))
    {
        if let Some(avg) = dice_average(f) {
            if i64::from(avg) != i64::from(average) {
                out.push(MonsterIssue::new(
                    "hp",
                    format!("average {average} but {f} averages {avg}"),
                ));
            }
        }
    }
    let perception = m
        .skills
        .get(&Skill::Perception)
        .copied()
        .unwrap_or_else(|| m.modifier(Ability::Wis));
    if i64::from(m.senses.passive_perception) != 10 + i64::from(perception) {
        out.push(MonsterIssue::new(
            "senses.passive_perception",
            format!(
                "{} but Perception bonus {perception}",
                m.senses.passive_perception
            ),
        ));
    }
    if let Some(init) = &m.initiative {
        if i32::from(init.score) != 10 + init.modifier {
            out.push(MonsterIssue::new(
                "initiative",
                format!("score {} but modifier {:+}", init.score, init.modifier),
            ));
        }
    }
    for (i, a) in m.actions.iter().enumerate() {
        let save_damage = a.save.iter().flat_map(|s| s.damage.iter());
        for d in save_damage {
            if let Some(avg) = d.dice.as_deref().and_then(dice_average) {
                if i64::from(avg) != i64::from(d.average) {
                    out.push(MonsterIssue::new(
                        format!("actions[{i}]"),
                        format!("save damage {} but {:?} averages {avg}", d.average, d.dice),
                    ));
                }
            }
        }
        if let Some(Attack { damage, .. }) = &a.attack {
            for d in damage {
                if let Some(avg) = d.dice.as_deref().and_then(dice_average) {
                    if i64::from(avg) != i64::from(d.average) {
                        out.push(MonsterIssue::new(
                            format!("actions[{i}]"),
                            format!("damage {} but {:?} averages {avg}", d.average, d.dice),
                        ));
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dice_parsing_and_average() {
        assert_eq!(parse_dice("19d12+133"), Some((19, 12, 133)));
        assert_eq!(parse_dice("2d6-2"), Some((2, 6, -2)));
        assert_eq!(parse_dice("1d8"), Some((1, 8, 0)));
        assert_eq!(parse_dice("d8"), None);
        assert_eq!(parse_dice("2d6+"), None);
        assert_eq!(dice_average("19d12+133"), Some(256));
        assert_eq!(dice_average("2d6-2"), Some(5));
        assert_eq!(dice_average("1d8"), Some(4));
    }
}
