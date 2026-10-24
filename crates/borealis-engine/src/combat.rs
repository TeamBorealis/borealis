//! Defences, senses, advantages and rule notes (2024). Computed where the number is
//! unconditional (resistances, condition immunities, senses, carry capacity); surfaced as
//! structured [`AdvantageEntry`] / [`RuleNote`] lines where the rule needs a table decision.
use crate::graph::Graph;
use crate::rule_view::{cap, live_rules, Live};
use crate::snapshot::ToggleSnapshot;
use crate::stats::StatSet;
use borealis_content::Rule;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One resistance, immunity, vulnerability or condition immunity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefenseEntry {
    /// Damage type or condition, lower case.
    pub name: String,
    /// Source feature name.
    pub source: String,
    /// In effect now (false while its toggle is off).
    pub active: bool,
    /// The toggle that gates it, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub while_toggle: Option<String>,
}

/// Damage and condition defences.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Defenses {
    /// Damage resistances.
    pub resistances: Vec<DefenseEntry>,
    /// Damage immunities.
    pub immunities: Vec<DefenseEntry>,
    /// Damage vulnerabilities.
    pub vulnerabilities: Vec<DefenseEntry>,
    /// Condition immunities.
    pub condition_immunities: Vec<DefenseEntry>,
}

/// A sense with its range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SenseSnapshot {
    /// Lower-case sense name (`darkvision`).
    pub name: String,
    /// Range in feet.
    pub range: i32,
    /// Source feature.
    pub source: String,
    /// Extra text.
    pub note: String,
}

/// Advantage on a class of roll, shown on the sheet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvantageEntry {
    /// `initiative`, `ability_check`, `saving_throw`, `spell_attack` or `death_save`.
    pub roll: String,
    /// Display text.
    pub text: String,
    /// Source feature.
    pub source: String,
    /// In effect now.
    pub active: bool,
    /// The toggle that gates it, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub while_toggle: Option<String>,
}

/// A surfaced rule (extra damage, Evasion, Reliable Talent, flight, ...).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleNote {
    /// The rule kind (`extra_damage`, `evasion`, ...).
    pub kind: String,
    /// Display text.
    pub text: String,
    /// Source feature.
    pub source: String,
}

/// Carrying capacity with size steps (2024 table).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CarrySnapshot {
    /// Size of the creature.
    pub size: String,
    /// Size steps granted (Powerful Build: 1).
    pub steps: u32,
    /// Size used for the table.
    pub effective_size: String,
    /// Carry capacity in pounds.
    pub carry: f64,
    /// Drag, lift and push limit in pounds.
    pub drag_lift_push: f64,
}

const SIZES: [&str; 5] = ["Tiny", "Medium", "Large", "Huge", "Gargantuan"];
const SIZE_MULT: [f64; 5] = [7.5, 15.0, 30.0, 60.0, 120.0];
const DAMAGE_PREFIXES: [(&str, &str); 3] = [
    ("ID_INTERNAL_CONDITION_DAMAGE_RESISTANCE_", "r"),
    ("ID_INTERNAL_CONDITION_DAMAGE_IMMUNITY_", "i"),
    ("ID_INTERNAL_CONDITION_DAMAGE_VULNERABILITY_", "v"),
];

fn push_unique(list: &mut Vec<DefenseEntry>, e: DefenseEntry) {
    if !list.contains(&e) {
        list.push(e);
    }
}

fn damage_grant(id: &str, source: &str, active: bool, toggle: Option<&str>, d: &mut Defenses) {
    for (prefix, kind) in DAMAGE_PREFIXES {
        let Some(rest) = id.strip_prefix(prefix) else {
            continue;
        };
        let e = DefenseEntry {
            name: rest.to_lowercase(),
            source: source.to_string(),
            active,
            while_toggle: toggle.map(str::to_string),
        };
        let list = match kind {
            "r" => &mut d.resistances,
            "i" => &mut d.immunities,
            _ => &mut d.vulnerabilities,
        };
        push_unique(list, e);
    }
}

fn toggle_active(toggles: &[ToggleSnapshot], id: &str) -> bool {
    toggles.iter().any(|t| t.id == id && t.active)
}

/// Resistances, immunities, vulnerabilities and condition immunities of the live elements.
#[must_use]
pub fn defenses(graph: &Graph<'_>, toggles: &[ToggleSnapshot]) -> Defenses {
    let mut d = Defenses::default();
    for id in graph.preorder() {
        let el = &graph.nodes[id].e.el;
        for r in &el.rules {
            let Rule::Grant(g) = r else { continue };
            if crate::rule_view::level_met(graph, id, g.level.map(u64::from)) {
                damage_grant(&g.id, &el.name, true, None, &mut d);
            }
        }
    }
    for l in live_rules(graph, &["toggle"]) {
        let Some(tid) = l.str("id") else { continue };
        let on = toggle_active(toggles, tid);
        for eff in
            l.c.fields
                .get("effects")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        {
            if eff.get("kind").and_then(Value::as_str) == Some("grant") {
                if let Some(g) = eff.get("id").and_then(Value::as_str) {
                    damage_grant(g, &l.el.name, on, Some(tid), &mut d);
                }
            }
        }
    }
    for l in live_rules(graph, &["condition_immunity", "condition_immunity_while"]) {
        let toggle = l.str("toggle").map(str::to_string);
        let active = toggle.as_deref().is_none_or(|t| toggle_active(toggles, t));
        for cond in l.list("conditions") {
            push_unique(
                &mut d.condition_immunities,
                DefenseEntry {
                    name: crate::rule_terms::condition(graph.uni, &cond),
                    source: l.el.name.clone(),
                    active,
                    while_toggle: toggle.clone(),
                },
            );
        }
    }
    d
}

/// Senses from `Vision` elements (range from the `<sense>:range` statistic) and `sense` rules.
#[must_use]
pub fn senses(graph: &Graph<'_>, stats: &StatSet) -> Vec<SenseSnapshot> {
    let mut out: Vec<SenseSnapshot> = Vec::new();
    for (id, n) in graph.nodes_of_type("Vision") {
        let name = n.e.el.name.to_lowercase();
        let range = stats.sum(&format!("{name}:range"));
        if range <= 0 || out.iter().any(|s| s.name == name) {
            continue;
        }
        let source = graph.nodes[id]
            .parent
            .map_or_else(|| n.e.el.name.clone(), |p| graph.nodes[p].e.el.name.clone());
        out.push(SenseSnapshot {
            name,
            range,
            source,
            note: String::new(),
        });
    }
    for l in live_rules(graph, &["sense"]) {
        let name = l.str("sense").unwrap_or("").replace('_', " ");
        let range =
            l.c.fields
                .get("range")
                .and_then(Value::as_i64)
                .and_then(|n| i32::try_from(n).ok())
                .unwrap_or(0);
        out.push(SenseSnapshot {
            name,
            range,
            source: l.el.name.clone(),
            note: l.str("note").unwrap_or("").to_string(),
        });
    }
    out
}

/// Total `carry_capacity_size_bonus` steps.
#[must_use]
pub fn carry_steps(graph: &Graph<'_>) -> u32 {
    live_rules(graph, &["carry_capacity_size_bonus"])
        .iter()
        .filter_map(|l| l.c.fields.get("steps").and_then(Value::as_u64))
        .map(|n| u32::try_from(n).unwrap_or(0))
        .sum()
}

fn size_tier(size_id: Option<&str>) -> usize {
    match size_id {
        Some("ID_SIZE_TINY") => 0,
        Some("ID_SIZE_LARGE") => 2,
        Some("ID_SIZE_HUGE") => 3,
        Some("ID_SIZE_GARGANTUAN" | "ID_SIZE_COLOSSAL") => 4,
        _ => 1,
    }
}

/// The 2024 carrying capacity table with size steps; `None` without a step bonus.
#[must_use]
pub fn carry(graph: &Graph<'_>, strength: i32) -> Option<CarrySnapshot> {
    let steps = carry_steps(graph);
    if steps == 0 {
        return None;
    }
    let size = graph
        .nodes_of_type("Size")
        .next()
        .map(|(_, n)| n.e.el.id.as_str());
    let base = size_tier(size);
    let eff = (base + steps as usize).min(SIZES.len() - 1);
    let carry = f64::from(strength) * SIZE_MULT[eff];
    Some(CarrySnapshot {
        size: SIZES[base].to_string(),
        steps,
        effective_size: SIZES[eff].to_string(),
        carry,
        drag_lift_push: carry * 2.0,
    })
}

fn roll_text(l: &Live<'_>, uni: &crate::universe::Universe) -> (String, String) {
    let roll = crate::rule_terms::roll_kind(l.str("roll").unwrap_or("ability_check"));
    let ab = l
        .str("ability")
        .map(|a| cap(&crate::rule_terms::ability(a)));
    let skill = l.str("skill").map(|s| crate::rule_terms::skill(uni, s));
    let text = match (roll.as_str(), ab, skill.as_deref(), l.str("spell_class")) {
        ("initiative", ..) => "Advantage on initiative rolls".to_string(),
        ("spell_attack", _, _, class) => format!(
            "Advantage on spell attack rolls{}",
            class.map_or(String::new(), |c| format!(" ({c} spells)"))
        ),
        ("saving_throw", Some(a), ..) => format!("Advantage on {a} saving throws"),
        ("ability_check", _, Some(s), _) => format!("Advantage on {} checks", cap(s)),
        ("ability_check", Some(a), ..) => format!("Advantage on {a} checks"),
        (r, ..) => format!("Advantage on {} rolls", r.replace('_', " ")),
    };
    (roll, text)
}

fn save_text(l: &Live<'_>) -> String {
    let mut t = if let Some(cond) = l.str("against_condition") {
        let mut s = format!("Advantage on saving throws against being {}", cap(cond));
        if l.str("scope") == Some("avoid_or_end") {
            s.push_str(" (to avoid or end the condition)");
        }
        s
    } else if !l.list("against_ability").is_empty() {
        let abs: Vec<String> = l.list("against_ability").iter().map(|a| cap(a)).collect();
        format!("Advantage on {} saving throws", abs.join(", "))
    } else if let Some(a) = l.str("ability") {
        let mut s = format!("Advantage on {} saving throws", cap(a));
        if let Some(ctx) = l.str("context") {
            s.push_str(&format!(" to {ctx}"));
        }
        s
    } else {
        "Advantage on saving throws".to_string()
    };
    if let Some(u) = l.str("unless_condition") {
        t.push_str(&format!(" unless {}", cap(u)));
    }
    t
}

fn adv(roll: &str, text: String, l: &Live<'_>, toggle: Option<&str>, on: bool) -> AdvantageEntry {
    AdvantageEntry {
        roll: roll.to_string(),
        text,
        source: l.el.name.clone(),
        active: on,
        while_toggle: toggle.map(str::to_string),
    }
}

/// Structured advantages: `roll_advantage`, `save_advantage`, `save_advantage_check`,
/// `death_save_rules`, and `roll_advantage` effects inside toggles (Rage).
#[must_use]
pub fn advantages(graph: &Graph<'_>, toggles: &[ToggleSnapshot]) -> Vec<AdvantageEntry> {
    let mut out = Vec::new();
    for l in live_rules(
        graph,
        &[
            "roll_advantage",
            "save_advantage",
            "save_advantage_check",
            "death_save_rules",
        ],
    ) {
        match l.c.kind.as_str() {
            "roll_advantage" => {
                let (roll, text) = roll_text(&l, graph.uni);
                out.push(adv(&roll, text, &l, None, true));
            }
            "save_advantage" => out.push(adv("saving_throw", save_text(&l), &l, None, true)),
            "save_advantage_check" => {
                let ctx = l.str("context").unwrap_or("");
                let t = format!("Advantage on ability checks to {ctx}");
                out.push(adv("ability_check", t, &l, None, true));
            }
            _ if l.c.fields.get("advantage").and_then(Value::as_bool) == Some(true) => {
                out.push(adv(
                    "death_save",
                    "Advantage on Death Saving Throws".to_string(),
                    &l,
                    None,
                    true,
                ));
            }
            _ => {}
        }
    }
    for l in live_rules(graph, &["toggle"]) {
        let Some(tid) = l.str("id") else { continue };
        let on = toggle_active(toggles, tid);
        for eff in
            l.c.fields
                .get("effects")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        {
            if eff.get("kind").and_then(Value::as_str) != Some("roll_advantage") {
                continue;
            }
            let Some(m) = eff.as_object() else { continue };
            let fake = borealis_content::rules::CustomRule {
                kind: "roll_advantage".into(),
                fields: m.clone(),
            };
            let inner = Live {
                node: l.node,
                el: l.el,
                c: &fake,
            };
            let (roll, text) = roll_text(&inner, graph.uni);
            out.push(adv(&roll, text, &l, Some(tid), on));
        }
    }
    out
}

fn dice_text(l: &Live<'_>, graph: &Graph<'_>, stats: &StatSet) -> String {
    if let Some(d) = l.str("dice") {
        return d.to_string();
    }
    if let Some(t) = l.str("dice_table") {
        let class = l.el.data.get("class").and_then(Value::as_str).unwrap_or("");
        let level = graph
            .class_levels
            .get(&class.to_lowercase())
            .copied()
            .unwrap_or(0);
        let cell = graph.uni.class_named(class).and_then(|c| {
            c.el.data
                .get("progression")?
                .get(t)?
                .as_array()?
                .get(usize::try_from(level - 1).ok()?)
                .cloned()
        });
        if let Some(s) = cell.and_then(|v| v.as_str().map(str::to_string)) {
            return s;
        }
    }
    let die = l.str("die").unwrap_or("");
    match l.c.fields.get("dice_count") {
        Some(Value::Number(n)) => format!("{n}{die}"),
        Some(v) => {
            let stat = v.get("stat").and_then(Value::as_str).unwrap_or("");
            format!("{}{die}", stats.sum(stat))
        }
        None => String::new(),
    }
}

fn extra_damage_text(l: &Live<'_>, graph: &Graph<'_>, stats: &StatSet) -> String {
    let mut t = format!(
        "{}: {}",
        l.str("name").unwrap_or(&l.el.name),
        dice_text(l, graph, stats)
    );
    if let Some(ty) = l.str("damage_type") {
        t.push_str(&format!(" {ty} damage"));
    }
    if let Some(o) = l.str("once_per") {
        t.push_str(&format!(", once per {o}"));
    }
    for k in ["trigger", "requires", "cost"] {
        if let Some(v) = l.str(k) {
            t.push_str(&format!("; {k} {v}"));
        }
    }
    t
}

fn deflect_text(l: &Live<'_>, graph: &Graph<'_>, mods: &[i32; 6]) -> String {
    let mut flat = 0;
    for p in l.list("plus") {
        flat += crate::rule_view::eval_value(&p, mods, 0).unwrap_or_else(|| {
            let class = p.trim_end_matches(" level").to_lowercase();
            graph.class_levels.get(&class).copied().unwrap_or(0)
        });
    }
    let all = !live_rules(graph, &["damage_reduction_all_types"]).is_empty();
    let types = if all {
        "any damage type".to_string()
    } else {
        l.list("damage_types").join(", ")
    };
    format!(
        "Reaction: reduce damage by {}+{flat} ({types})",
        l.str("dice").unwrap_or("")
    )
}

/// Surfaced rules as display notes.
#[must_use]
pub fn notes(
    graph: &Graph<'_>,
    stats: &StatSet,
    mods: &[i32; 6],
    scores: &[i32; 6],
) -> Vec<RuleNote> {
    let kinds = [
        "extra_damage",
        "damage_bonus_spell",
        "damage_reduction",
        "check_floor",
        "evasion",
        "initiative_swap",
        "movement_mode",
        "death_save_rules",
    ];
    let live = live_rules(graph, &kinds);
    let replaced: Vec<&str> = live
        .iter()
        .filter(|l| l.c.kind == "extra_damage")
        .filter_map(|l| l.str("replaces"))
        .collect();
    let mut out = Vec::new();
    for l in &live {
        let text = match l.c.kind.as_str() {
            "extra_damage" if l.str("name").is_some_and(|n| replaced.contains(&n)) => continue,
            "extra_damage" => extra_damage_text(l, graph, stats),
            "damage_bonus_spell" => format!(
                "Add {} to one damage roll of a spell of your chosen type, once per {}",
                l.str("value").unwrap_or("a modifier"),
                l.str("once_per").unwrap_or("spell")
            ),
            "damage_reduction" => deflect_text(l, graph, mods),
            "check_floor" => floor_text(l, scores),
            "evasion" => "Evasion: successful Dexterity saves deal no damage, failures half (not while Incapacitated)".to_string(),
            "initiative_swap" => "Can swap Initiative with a willing ally".to_string(),
            "movement_mode" => format!(
                "{} speed {} for {} ({})",
                cap(l.str("mode").unwrap_or("")),
                l.c.fields.get("speed").map_or(String::new(), |v| v.to_string().trim_matches('"').to_string()),
                l.str("duration").unwrap_or(""),
                l.str("action").unwrap_or("").replace('_', " ")
            ),
            "death_save_rules" => match l.c.fields.get("natural_20_threshold").and_then(Value::as_i64) {
                Some(n) => format!("A natural {n}-20 on a Death Saving Throw counts as a 20"),
                None => continue,
            },
            _ => continue,
        };
        out.push(RuleNote {
            kind: l.c.kind.clone(),
            text,
            source: l.el.name.clone(),
        });
    }
    out
}

fn floor_text(l: &Live<'_>, scores: &[i32; 6]) -> String {
    let on = l.list("applies_to").join(", ");
    if let Some(f) = l.c.fields.get("d20_floor").and_then(Value::as_i64) {
        return format!("A d20 roll of {} or lower counts as {f} on {on}", f - 1);
    }
    let ab = l.str("floor").unwrap_or("");
    let score = crate::ids::ability_index(ab).map_or(0, |i| scores[i]);
    format!(
        "{on}: the total can't be lower than your {} score ({score})",
        cap(ab)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carry_table_has_five_tiers() {
        assert!((SIZE_MULT[2] - 30.0).abs() < f64::EPSILON);
        assert_eq!(size_tier(Some("ID_SIZE_MEDIUM")), 1);
    }
}
