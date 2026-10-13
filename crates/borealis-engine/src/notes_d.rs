//! Surfaced rules of engine slice D (auras, Cunning Strike, Channel Divinity options, cantrip
//! riders, free casts, slot recovery, ...). Each live custom rule becomes one [`RuleNote`] whose
//! text carries the numbers the engine can compute (level scaling, ability modifiers).

use crate::combat::RuleNote;
use crate::graph::Graph;
use crate::play_rules::eval_formula;
use crate::rule_view::{cap, live_rules, Live};
use crate::stats::StatSet;
use serde_json::Value;

/// Rule kinds handled here.
pub const KINDS: &[&str] = &[
    "aura",
    "aura_immunity",
    "convert",
    "cunning_strike",
    "cunning_effect",
    "channel_divinity_option",
    "pact_weapon",
    "replace_on_level",
    "proficiency_filter",
    "regeneration",
    "healing",
    "damage_scaling",
    "cantrip_damage_bonus",
    "cantrip_range_bonus",
    "cantrip_rider",
    "cantrip_swap",
    "free_cast_choice",
    "spell_free_cast",
    "spell_cast_alt_cost",
    "slot_recovery",
    "at_will_spells",
    "spell_replace_on_level",
];

struct Cx<'a, 'u> {
    graph: &'a Graph<'u>,
    stats: &'a StatSet,
    mods: &'a [i32; 6],
    prof: i32,
    level: u32,
}

impl Cx<'_, '_> {
    fn eval(&self, expr: &str) -> Option<i32> {
        eval_formula(self.graph, self.stats, self.mods, expr)
    }

    /// `expr` with its value when computable: `5*cleric level (25)`.
    fn show(&self, expr: &str) -> String {
        match self.eval(expr) {
            Some(n) if expr.trim().parse::<i32>().is_err() => format!("{expr} ({n})"),
            _ => expr.to_string(),
        }
    }

    /// Value of a save DC formula such as `8+dexterity+proficiency`.
    fn dc(&self, formula: &str) -> Option<i32> {
        let mut total = 0;
        for part in formula.split('+') {
            let p = part.trim().to_lowercase();
            total += if let Ok(n) = p.parse::<i32>() {
                n
            } else if p == "proficiency" {
                self.prof
            } else {
                self.mods[crate::ids::ability_index(&p)?]
            };
        }
        Some(total)
    }
}

fn s<'a>(l: &'a Live<'_>, k: &str) -> &'a str {
    l.str(k).unwrap_or("")
}

fn aura(l: &Live<'_>, cx: &Cx<'_, '_>) -> String {
    let base =
        l.c.fields
            .get("radius_feet")
            .and_then(Value::as_u64)
            .unwrap_or(0);
    let by_level =
        l.c.fields
            .get("radius_by_level")
            .and_then(Value::as_object)
            .and_then(|m| {
                m.iter()
                    .filter_map(|(k, v)| Some((k.parse::<u32>().ok()?, v.as_u64()?)))
                    .filter(|(lv, _)| *lv <= cx.graph.mgr_level(cx.graph.nodes[l.node].mgr))
                    .max_by_key(|(lv, _)| *lv)
                    .map(|(_, r)| r)
            })
            .unwrap_or(base);
    let bonus =
        l.c.fields
            .get("bonus_saving_throws")
            .and_then(|b| b.get("formula"))
            .and_then(Value::as_str)
            .and_then(|f| cx.eval(f))
            .map_or(String::new(), |n| {
                format!("; you and allies in it add +{n} to saving throws")
            });
    format!("{}: {by_level}-foot emanation{bonus}", cap(s(l, "id")))
}

fn convert(l: &Live<'_>, cx: &Cx<'_, '_>) -> String {
    let (from, to) = (s(l, "from"), s(l, "to"));
    let Some(table) = l.c.fields.get("table").and_then(Value::as_array) else {
        return format!("Convert {from} into {to} ({})", s(l, "amount"));
    };
    let rows: Vec<String> = table
        .iter()
        .filter(|r| r.get("min_level").and_then(Value::as_u64).unwrap_or(0) <= u64::from(cx.level))
        .map(|r| {
            format!(
                "level {} slot = {} points",
                r.get("slot").and_then(Value::as_u64).unwrap_or(0),
                r.get("cost").and_then(Value::as_u64).unwrap_or(0)
            )
        })
        .collect();
    format!(
        "Convert {from} into {to} ({}): {}",
        s(l, "action").replace('_', " "),
        rows.join(", ")
    )
}

fn cunning_strike(l: &Live<'_>, cx: &Cx<'_, '_>) -> String {
    let max =
        l.c.fields
            .get("max_effects")
            .and_then(Value::as_u64)
            .unwrap_or(1);
    let dc = l
        .str("save_dc")
        .and_then(|f| cx.dc(f))
        .map_or(String::new(), |n| format!("; save DC {n}"));
    format!("Cunning Strike: up to {max} effect(s) per Sneak Attack{dc}")
}

fn cunning_effect(l: &Live<'_>) -> String {
    let dice =
        l.c.fields
            .get("cost_dice")
            .and_then(Value::as_u64)
            .unwrap_or(0);
    let save = l
        .str("save")
        .map_or(String::new(), |a| format!("; {} save", cap(a)));
    format!(
        "{}: costs {dice} Sneak Attack di{}{save}",
        l.el.name,
        if dice == 1 { "e" } else { "ce" }
    )
}

fn healing(l: &Live<'_>, cx: &Cx<'_, '_>) -> String {
    let plus = cx
        .eval(s(l, "plus"))
        .map_or_else(|| s(l, "plus").to_string(), |n| n.to_string());
    format!(
        "{}: {} + {plus} hit points ({})",
        l.str("name").unwrap_or(&l.el.name),
        s(l, "dice"),
        s(l, "action").replace('_', " ")
    )
}

fn damage_scaling(l: &Live<'_>, cx: &Cx<'_, '_>) -> Option<String> {
    let levels = l.c.fields.get("levels")?.as_object()?;
    let dice = crate::cantrips::dice_for_level(levels, cx.level)?;
    Some(format!(
        "{}: {dice} {}",
        l.str("name").unwrap_or(&l.el.name),
        s(l, "type_from")
    ))
}

fn slot_recovery(l: &Live<'_>, cx: &Cx<'_, '_>) -> String {
    if l.c.fields.contains_key("pact_slots") {
        return format!(
            "Recover {} of your Pact Magic slots ({}, once per long rest)",
            s(l, "amount"),
            s(l, "cast_time")
        );
    }
    let combined = l
        .str("max_combined_level")
        .map_or(String::new(), |e| cx.show(e));
    format!(
        "Recover expended spell slots on a {}: combined level up to {combined}, none above level {}; {} use per {}",
        s(l, "on").replace('_', " "),
        l.c.fields.get("max_slot_level").and_then(Value::as_u64).unwrap_or(0),
        l.c.fields.get("uses").and_then(Value::as_u64).unwrap_or(1),
        s(l, "recharge").replace('_', " ")
    )
}

fn generic(l: &Live<'_>) -> String {
    let rest: Vec<String> =
        l.c.fields
            .iter()
            .filter(|(k, _)| !matches!(k.as_str(), "level" | "requirements"))
            .map(|(k, v)| {
                let t = v.as_str().map_or_else(|| v.to_string(), str::to_string);
                format!("{}: {t}", k.replace('_', " "))
            })
            .collect();
    format!("{}: {}", l.el.name, rest.join("; "))
}

fn text(l: &Live<'_>, cx: &Cx<'_, '_>) -> Option<String> {
    Some(match l.c.kind.as_str() {
        "aura" => aura(l, cx),
        "aura_immunity" => format!(
            "Immune to {} while in your {}",
            l.list("conditions").join(", "),
            cap(s(l, "aura"))
        ),
        "convert" => convert(l, cx),
        "cunning_strike" => cunning_strike(l, cx),
        "cunning_effect" => cunning_effect(l),
        "channel_divinity_option" => {
            let pool = l.str("pool_hit_points").map_or(String::new(), |e| {
                format!(
                    "; heal up to {} hit points, each creature at most {}",
                    cx.show(e),
                    s(l, "max_fraction")
                )
            });
            let bonus = l.str("attack_bonus").map_or(String::new(), |e| {
                format!("; +{} to attack rolls for {}", cx.show(e), s(l, "duration"))
            });
            format!(
                "{} (Channel Divinity){pool}{bonus}",
                l.str("name").unwrap_or(&l.el.name)
            )
        }
        "regeneration" => format!(
            "Regain {} hit points at the {} while {}",
            cx.show(s(l, "amount")),
            s(l, "timing").replace('_', " "),
            s(l, "condition")
        ),
        "healing" => healing(l, cx),
        "damage_scaling" => damage_scaling(l, cx)?,
        "cantrip_damage_bonus" => format!(
            "Add {} to the damage of {}cantrips of the {} list",
            cx.show(s(l, "value")),
            if l.c.fields.contains_key("choose_cantrip") {
                "one chosen damage cantrip: "
            } else {
                "your "
            },
            s(l, "spell_class")
        ),
        "cantrip_range_bonus" => {
            let per =
                l.c.fields
                    .get("feet_per_warlock_level")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
            let lv = i64::from(cx.graph.mgr_level(cx.graph.nodes[l.node].mgr));
            format!(
                "Chosen cantrip ({}): range +{} feet ({per} per {} level)",
                s(l, "cantrip_filter"),
                per * lv,
                s(l, "spell_class")
            )
        }
        "cantrip_rider" => format!(
            "Chosen cantrip ({}): {}",
            s(l, "cantrip_filter"),
            s(l, "effect")
        ),
        "slot_recovery" => slot_recovery(l, cx),
        _ => generic(l),
    })
}

/// One note per live slice D rule (duplicates from copied multiclass rules are dropped).
#[must_use]
pub fn notes(graph: &Graph<'_>, stats: &StatSet, mods: &[i32; 6]) -> Vec<RuleNote> {
    let cx = Cx {
        graph,
        stats,
        mods,
        prof: stats.sum("proficiency"),
        level: graph.total_level(),
    };
    let mut out: Vec<RuleNote> = Vec::new();
    for l in live_rules(graph, KINDS) {
        let Some(text) = text(&l, &cx) else { continue };
        let note = RuleNote {
            kind: l.c.kind.clone(),
            text,
            source: l.el.name.clone(),
        };
        if !out.contains(&note) {
            out.push(note);
        }
    }
    out
}
