//! Attack rows for damaging cantrips (Fire Bolt, Eldritch Blast, Sacred Flame, ...).
//!
//! Aurora has the row type (AttackSectionItem.cs:159-172, 365-376: spell name, range and
//! "+N ABI vs AC" from the casting source's spell attack statistic plus `spellcasting:attack`)
//! but never creates one, so the rows follow the SRD: every known or prepared cantrip whose text
//! deals dice damage gets one row per casting source that has it (first source wins). The
//! roll is the source's spell attack ("melee/ranged spell attack") or its save DC ("<Ability>
//! saving throw"); the damage scales by character level (`cantrips.rs`; golden ruling A2) and
//! adds the `<spell name>:damage` statistic (Aurora's per-item convention), Agonizing Blast's
//! `agonizing blast:damage` on Eldritch Blast, and 2024 `cantrip_damage_bonus` rules of the
//! source's class (a `choose_cantrip` bonus only when the class has exactly one damaging
//! cantrip, as there is no stored choice).

use crate::graph::Graph;
use crate::snapshot::{AttackSave, AttackSnapshot, SpellcastingSnapshot};
use crate::stats::StatSet;
use crate::universe::Universe;

const AGONIZING_STAT: &str = "agonizing blast:damage";
const ELDRITCH_BLAST: &str = "eldritch blast";

/// How a cantrip hits.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Roll {
    Attack,
    Save(String),
}

const SAVE_ABILITIES: [&str; 6] = [
    "strength",
    "dexterity",
    "constitution",
    "intelligence",
    "wisdom",
    "charisma",
];

fn roll_of(text: &str) -> Option<Roll> {
    let lower = text.to_lowercase();
    if lower.contains("spell attack") {
        return Some(Roll::Attack);
    }
    let at = lower.find("saving throw")?;
    let before = lower[..at].trim_end();
    SAVE_ABILITIES
        .iter()
        .find(|a| before.ends_with(*a))
        .map(|a| Roll::Save((*a).to_string()))
}

/// `1d10 Fire` -> (`1d10`, `fire`).
fn split_damage(d: &str) -> (String, String) {
    let mut it = d.splitn(2, ' ');
    let dice = it.next().unwrap_or("").to_string();
    (dice, it.next().unwrap_or("").to_lowercase())
}

struct Bonus<'a> {
    graph: &'a Graph<'a>,
    stats: &'a StatSet,
    mods: &'a [i32; 6],
}

impl Bonus<'_> {
    /// `cantrip_damage_bonus` rules of `class` (`only` = this is the class's only damaging
    /// cantrip, so a `choose_cantrip` bonus goes to it).
    fn class_rules(&self, class: &str, only: bool) -> i32 {
        crate::rule_view::live_rules(self.graph, &["cantrip_damage_bonus"])
            .iter()
            .filter(|l| {
                l.str("spell_class")
                    .is_some_and(|c| c.eq_ignore_ascii_case(class))
            })
            .filter(|l| only || !l.c.fields.contains_key("choose_cantrip"))
            .filter_map(|l| {
                let v = l.str("value")?;
                crate::play_rules::eval_formula(self.graph, self.stats, self.mods, v)
            })
            .sum()
    }

    fn of(&self, spell: &str, class: &str, only: bool) -> i32 {
        let lower = spell.to_lowercase();
        let mut n = self.stats.sum(&format!("{lower}:damage"));
        if lower == ELDRITCH_BLAST {
            n += self.stats.sum(AGONIZING_STAT);
        }
        n + self.class_rules(class, only)
    }
}

/// Attack rows of the damaging cantrips of every casting source.
#[must_use]
pub fn cantrip_attacks(
    graph: &Graph<'_>,
    uni: &Universe,
    stats: &StatSet,
    mods: &[i32; 6],
    casting: &[SpellcastingSnapshot],
) -> Vec<AttackSnapshot> {
    let level = graph.total_level();
    let bonus = Bonus { graph, stats, mods };
    let mut out: Vec<AttackSnapshot> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for sc in casting {
        let rows: Vec<_> = sc
            .spells
            .iter()
            .filter(|s| s.level == 0 && (s.known || s.prepared))
            .filter_map(|s| {
                let e = uni.get(&s.id)?;
                let text = crate::play_rules::plain_text(&e.el.description);
                let roll = roll_of(&text)?;
                let damage = crate::cantrips::attack_damage(&e.el.description, level)?;
                Some((s, e, roll, damage))
            })
            .collect();
        let only = rows.len() == 1;
        for (s, e, roll, damage) in &rows {
            if seen.contains(&s.id.as_str()) {
                continue;
            }
            seen.push(&s.id);
            let (dice, ty) = split_damage(damage);
            let extra = bonus.of(&e.el.name, &sc.name, only);
            let (attack, save) = match roll {
                Roll::Attack => (sc.attack, None),
                Roll::Save(a) => (
                    0,
                    Some(AttackSave {
                        dc: sc.dc,
                        ability: a.clone(),
                    }),
                ),
            };
            out.push(AttackSnapshot {
                uid: format!("spell:{}:{}", sc.name, s.id),
                name: e.el.name.clone(),
                range: e.el.setter_value("range").unwrap_or("").to_string(),
                attack,
                damage: crate::gear::dice_with_bonus(&dice, extra, &ty),
                ability: sc.ability.to_lowercase(),
                proficient: true,
                mastery: None,
                crit_range: 20,
                die_floor: None,
                offhand_damage: None,
                ammunition: None,
                save,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roll_kinds() {
        assert_eq!(
            roll_of("Make a ranged spell attack against the target."),
            Some(Roll::Attack)
        );
        assert_eq!(
            roll_of("it must succeed on a Dexterity saving throw or take 1d8 radiant damage"),
            Some(Roll::Save("dexterity".into()))
        );
        assert_eq!(roll_of("You create a harmless sensory effect."), None);
    }

    #[test]
    fn damage_split() {
        assert_eq!(
            split_damage("2d10 Fire"),
            ("2d10".to_string(), "fire".to_string())
        );
    }
}
