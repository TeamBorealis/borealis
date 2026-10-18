//! Attack-line modifiers and skill bonuses of the 2024 rule kinds: `attack_bonus`,
//! `critical_range`, `damage_die_floor`, `extra_attack_damage_modifier`, `skill_bonus`,
//! `skill_bonus_unproficient`.
use crate::graph::Graph;
use crate::rule_view::{eval_value, live_rules, Live};
use serde_json::Value;

/// The default critical hit threshold.
pub const DEFAULT_CRIT: u8 = 20;

/// What the attack rules say about one weapon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackFacts<'a> {
    /// Inventory uid and element id of the weapon (`None` for the Unarmed Strike).
    pub item: Option<(&'a str, &'a str)>,
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

fn targets(applies: &[String], f: &AttackFacts<'_>) -> bool {
    let all = applies.is_empty();
    let hit = |key: &str| applies.iter().any(|a| a.contains(key));
    all || (f.unarmed && hit("unarmed")) || (!f.unarmed && hit("weapon"))
}

fn weapon_filter(filter: &str, f: &AttackFacts<'_>) -> bool {
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

/// A rule with an `item` field applies only to that weapon: `self` = the item whose element
/// carries the rule (while attuned, when it requires attunement), else a weapon element id.
fn item_bound_ok(graph: &Graph<'_>, l: &Live<'_>, f: &AttackFacts<'_>) -> bool {
    let Some(want) = l.str("item").map(str::trim).filter(|w| !w.is_empty()) else {
        return true;
    };
    let Some((uid, id)) = f.item else {
        return false;
    };
    if !want.eq_ignore_ascii_case("self") {
        return want.eq_ignore_ascii_case(id);
    }
    let node = &graph.nodes[l.node];
    node.item_uid.as_deref() == Some(uid)
        && (node.attuned || !crate::inventory::item_facts(node.e).requires_attunement)
}

/// Collect the modifiers that apply to a weapon.
#[must_use]
pub fn attack_mods(graph: &Graph<'_>, f: &AttackFacts<'_>) -> AttackMods {
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
            "critical_range"
                if targets(&l.list("applies_to"), f) && item_bound_ok(graph, &l, f) =>
            {
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

/// `extra_damage` triggers that add their dice to every hit of a melee weapon attack line
/// (`unarmed` = the trigger also covers an Unarmed Strike).
const PER_HIT_TRIGGERS: &[(&str, bool)] = &[
    ("melee weapon hit", false),
    ("melee weapon or unarmed strike hit", true),
];

/// Fixed extra damage on every hit of a melee attack line (`+ 1d8 radiant`: Improved Divine
/// Smite, Radiant Strikes). Rules limited per turn, with a cost, a requirement, a choice of
/// effects, or replaced by another live rule stay notes only. A thrown melee weapon keeps it
/// (golden ruling A1: the line is the melee weapon's).
#[must_use]
pub fn per_hit_extra_damage(graph: &Graph<'_>, unarmed: bool) -> Vec<String> {
    let live = live_rules(graph, &["extra_damage"]);
    let replaced: Vec<&str> = live.iter().filter_map(|l| l.str("replaces")).collect();
    live.iter()
        .filter(|l| {
            l.str("trigger").is_some_and(|t| {
                PER_HIT_TRIGGERS
                    .iter()
                    .any(|(name, any)| t.eq_ignore_ascii_case(name) && (*any || !unarmed))
            })
        })
        .filter(|l| {
            [
                "once_per",
                "cost",
                "requires",
                "effects_choose",
                "per_slot_level",
            ]
            .iter()
            .all(|k| !l.c.fields.contains_key(*k))
        })
        .filter(|l| !l.str("name").is_some_and(|n| replaced.contains(&n)))
        .filter_map(|l| {
            let dice = l.str("dice")?.trim();
            let ty = l.str("damage_type").unwrap_or("").trim();
            Some(format!("{dice} {ty}").trim().to_string())
        })
        .collect()
}
