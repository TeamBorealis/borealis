//! Attack-line modifiers and skill bonuses of the 2024 rule kinds: `attack_bonus`,
//! `critical_range`, `damage_die_floor`, `extra_attack_damage_modifier`, `skill_bonus`,
//! `skill_bonus_unproficient`.
use crate::graph::Graph;
use crate::rule_view::{eval_value, live_rules};
use serde_json::Value;

/// The default critical hit threshold.
pub const DEFAULT_CRIT: u8 = 20;

/// What the attack rules say about one weapon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackFacts {
    /// Melee (otherwise ranged).
    pub melee: bool,
    /// Unarmed Strike.
    pub unarmed: bool,
    /// Light property.
    pub light: bool,
    /// Held in two hands (Two-Handed, or Versatile used two-handed).
    pub two_handed: bool,
}

/// Modifiers for one attack line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackMods {
    /// Flat attack roll bonus.
    pub attack: i32,
    /// Critical hit threshold.
    pub crit: u8,
    /// Damage die floor.
    pub die_floor: Option<u8>,
    /// The off-hand attack adds the ability modifier.
    pub offhand_modifier: bool,
}

fn targets(applies: &[String], f: &AttackFacts) -> bool {
    let all = applies.is_empty();
    let hit = |key: &str| applies.iter().any(|a| a.contains(key));
    all || (f.unarmed && hit("unarmed")) || (!f.unarmed && hit("weapon"))
}

fn weapon_filter(filter: &str, f: &AttackFacts) -> bool {
    let want = filter.to_lowercase();
    if want.contains("two-handed") || want.contains("versatile") {
        return f.melee && f.two_handed;
    }
    match want.as_str() {
        "ranged" => !f.melee,
        "melee" => f.melee,
        _ => true,
    }
}

/// Collect the modifiers that apply to a weapon.
#[must_use]
pub fn attack_mods(graph: &Graph<'_>, f: &AttackFacts) -> AttackMods {
    let mut m = AttackMods {
        attack: 0,
        crit: DEFAULT_CRIT,
        die_floor: None,
        offhand_modifier: false,
    };
    let kinds = [
        "attack_bonus",
        "critical_range",
        "damage_die_floor",
        "extra_attack_damage_modifier",
    ];
    for l in live_rules(graph, &kinds) {
        match l.c.kind.as_str() {
            "attack_bonus" if weapon_filter(l.str("weapon").unwrap_or(""), f) => {
                let v = l.c.fields.get("value").and_then(Value::as_i64).unwrap_or(0);
                m.attack += i32::try_from(v).unwrap_or(0);
            }
            "critical_range" if targets(&l.list("applies_to"), f) => {
                let min = l.c.fields.get("min").and_then(Value::as_u64).unwrap_or(20);
                m.crit = m.crit.min(u8::try_from(min).unwrap_or(DEFAULT_CRIT));
            }
            "damage_die_floor" if weapon_filter(l.str("weapon").unwrap_or(""), f) => {
                let v = l.c.fields.get("floor").and_then(Value::as_u64).unwrap_or(0);
                m.die_floor = u8::try_from(v).ok().filter(|v| *v > 0);
            }
            "extra_attack_damage_modifier" if f.light && !f.unarmed => m.offhand_modifier = true,
            _ => {}
        }
    }
    m
}

/// Computed bonus to a skill's checks: `skill_bonus` (listed skills, with a minimum) and
/// `skill_bonus_unproficient` (Jack of All Trades) for skills without proficiency.
#[must_use]
pub fn skill_bonus(
    graph: &Graph<'_>,
    skill: &str,
    proficiency_part: i32,
    mods: &[i32; 6],
    prof: i32,
) -> i32 {
    let mut total = 0;
    for l in live_rules(graph, &["skill_bonus", "skill_bonus_unproficient"]) {
        let value = l
            .str("value")
            .and_then(|v| eval_value(v, mods, prof))
            .unwrap_or(0);
        if l.c.kind == "skill_bonus" {
            if l.list("skills")
                .iter()
                .any(|s| s.eq_ignore_ascii_case(skill))
            {
                let min = l.c.fields.get("minimum").and_then(Value::as_i64);
                total += min.map_or(value, |m| value.max(i32::try_from(m).unwrap_or(0)));
            }
        } else if proficiency_part == 0 {
            total += value;
        }
    }
    total
}
