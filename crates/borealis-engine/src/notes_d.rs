//! Surfaced rules of engine slice D (auras, Cunning Strike, Channel Divinity options, cantrip
//! riders, free casts, slot recovery, ...). Each live custom rule becomes one [`RuleNote`] whose
//! text carries the numbers the engine can compute (level scaling, ability modifiers).
//!
//! The notes are the Play tab's Situational list, so every note is a plain sentence (GitHub #70).
//! Rules that are bookkeeping rather than something to remember at the table give no note: a
//! re-pick on a level or a rest ([`BOOKKEEPING`]: the picks stay editable in the builder), a weapon
//! proficiency filter (`item_prof` applies it to the weapons) and a pact weapon (`pact_weapon`
//! bonds it on the Gear tab). Their feature's own text still tells the player. A free cast of a
//! chosen spell (`spell_free_cast`) is a tracked use (`spell_rules`), not a note.

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
    "spell_cast_alt_cost",
    "slot_recovery",
    "at_will_spells",
    "spell_replace_on_level",
];

/// Kinds of [`KINDS`] that give no note: when a pick may be changed, which weapons count as
/// proficient, and the pact weapon bond are bookkeeping, not Situational text.
pub const BOOKKEEPING: &[&str] = &[
    "replace_on_level",
    "spell_replace_on_level",
    "cantrip_swap",
    "proficiency_filter",
    "pact_weapon",
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
        eval_note_dc(formula, self.mods, self.prof)
    }
}

/// Value of a save DC formula shown in a note (`cunning_strike.save_dc`, `8+dexterity+proficiency`):
/// `+`-separated numbers, `proficiency` and bare ability names. The homebrew creator writes these
/// formulas and checks its text against this reader.
#[must_use]
pub fn eval_note_dc(formula: &str, mods: &[i32; 6], prof: i32) -> Option<i32> {
    let mut total = 0;
    for part in formula.split('+') {
        let p = part.trim().to_lowercase();
        total += if let Ok(n) = p.parse::<i32>() {
            n
        } else if p == "proficiency" {
            prof
        } else {
            mods[crate::ids::ability_index(&p)?]
        };
    }
    Some(total)
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
    let dice = crate::breath::scaled_dice(l, cx.level)?;
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

/// `"a"` or `"an"` before `word`.
fn article(word: &str) -> &'static str {
    if word.starts_with(['a', 'e', 'i', 'o', 'u', 'A', 'E', 'I', 'O', 'U']) {
        "an"
    } else {
        "a"
    }
}

/// `one`, `two` or `n` joined as `a, b or c`.
fn or_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
    }
}

/// How often: `once`, `twice`, `3 times`.
fn times(n: u64) -> String {
    match n {
        1 => "once".to_string(),
        2 => "twice".to_string(),
        n => format!("{n} times"),
    }
}

/// A rest token as words: `short_rest` -> `short rest`.
fn words(token: &str) -> String {
    token.replace('_', " ")
}

/// Spell Mastery: chosen spells of some levels cast at will.
fn at_will_spells(l: &Live<'_>) -> String {
    let levels: Vec<String> = match l.c.fields.get("levels") {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_u64).collect(),
        Some(v) => v.as_u64().into_iter().collect(),
        None => Vec::new(),
    }
    .iter()
    .map(|lv| format!("level {lv}"))
    .collect();
    let levels = match levels.as_slice() {
        [] => String::new(),
        [one] => format!("{one} "),
        [rest @ .., last] => format!("{} and {last} ", rest.join(", ")),
    };
    let time = l.str("casting_time").map_or(String::new(), |t| {
        let t = words(t);
        format!(" (casting time: {} {t})", article(&t))
    });
    format!("Cast your chosen {levels}spells{time} at their lowest level without a spell slot")
}

/// Signature Spells, Natural Recovery: chosen spells cast without a slot a number of times.
fn free_cast_choice(l: &Live<'_>) -> String {
    let count = l.c.fields.get("count").and_then(Value::as_u64).unwrap_or(1);
    let level =
        l.c.fields
            .get("spell_level")
            .and_then(Value::as_u64)
            .map_or(String::new(), |n| format!("level {n} "));
    let from = l
        .str("from_feature")
        .map_or(String::new(), |f| format!(" from {f}"));
    let uses =
        l.c.fields
            .get("uses_each")
            .or_else(|| l.c.fields.get("uses"))
            .and_then(Value::as_u64)
            .unwrap_or(1);
    let what = if count == 1 {
        format!("one chosen {level}spell{from}")
    } else {
        format!("each of {count} chosen {level}spells{from}")
    };
    let rests: Vec<String> = l.list("recharge").iter().map(|r| words(r)).collect();
    let regain = if rests.is_empty() {
        String::new()
    } else {
        format!("; regained on {} {}", article(&rests[0]), or_list(&rests))
    };
    format!("Cast {what} {} without a spell slot{regain}", times(uses))
}

/// Wild Companion: a spell cast with another cost.
fn spell_cast_alt_cost(l: &Live<'_>, cx: &Cx<'_, '_>) -> String {
    // an id not loaded reads as its words ("ID_X_SPELL_FIND_FAMILIAR" -> "find familiar")
    let spell = crate::rule_terms::term(cx.graph.uni, s(l, "spell"), "SPELL_");
    let costs: Vec<String> = l
        .list("cost")
        .iter()
        .map(|c| format!("{} {c}", article(c)))
        .collect();
    let by = if costs.is_empty() {
        String::new()
    } else {
        format!(" by expending {}", or_list(&costs))
    };
    let material = if l.c.fields.get("no_material").and_then(Value::as_bool) == Some(true) {
        ", without Material components"
    } else {
        ""
    };
    format!("Cast {spell}{by}{material}")
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
            match s(l, "cantrip_filter") {
                "" => s(l, "spell_class").to_string(),
                f => format!("{} ({f})", s(l, "spell_class")),
            }
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
            "Chosen cantrip ({}, {} list): {}",
            s(l, "cantrip_filter"),
            s(l, "spell_class"),
            s(l, "effect")
        ),
        "slot_recovery" => slot_recovery(l, cx),
        "at_will_spells" => at_will_spells(l),
        "free_cast_choice" => free_cast_choice(l),
        "spell_cast_alt_cost" => spell_cast_alt_cost(l, cx),
        _ => return None,
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
        if BOOKKEEPING.contains(&l.c.kind.as_str()) {
            continue;
        }
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
