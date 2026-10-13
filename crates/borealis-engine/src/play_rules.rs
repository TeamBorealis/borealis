//! 2024 play-time rule kinds read from live elements: `resource` (+ `resource_cost`,
//! `resource_regain`), `toggle`, `martial_arts`, `heroic_inspiration`, the Weapon Mastery slots
//! and the linear exhaustion numbers.

use crate::graph::{Graph, NodeId};
use crate::profile::ExhaustionModel;
use crate::snapshot::{
    ExhaustionSnapshot, InspirationSource, MartialArtsSnapshot, MasteryPick, MasteryRef,
    ResourceCostSnapshot, ResourceSnapshot, ToggleSnapshot,
};
use crate::spell_rules::progression;
use crate::stats::StatSet;
use crate::universe::{Cond, UElem, UStat, Universe, Val};
use borealis_content::rules::{CustomRule, StatRule};
use borealis_content::{Element, Rule};
use serde_json::Value;

/// Stat group that carries the exhaustion D20 penalty.
pub const D20_GROUP: &str = "d20 tests";
const DEFAULT_D20_PER_LEVEL: i32 = 2;
const DEFAULT_SPEED_PER_LEVEL: i32 = 5;
const DEFAULT_MAX_LEVELS: u32 = 6;
const EXHAUSTION_ID: &str = "ID_2024_CONDITION_EXHAUSTION";

// ---------------------------------------------------------------- exhaustion

/// Exhaustion numbers and the prebuilt stat rules of each level (two per level: speed, D20).
#[derive(Debug, Clone)]
pub struct ExhaustionRules {
    /// D20 Test penalty per level.
    pub d20_per_level: i32,
    /// Speed penalty in feet per level.
    pub speed_per_level: i32,
    /// Highest level, which is also the level of death.
    pub max_levels: u32,
    rules: Vec<UStat>,
}

impl Default for ExhaustionRules {
    fn default() -> Self {
        Self::with(
            DEFAULT_D20_PER_LEVEL,
            DEFAULT_SPEED_PER_LEVEL,
            DEFAULT_MAX_LEVELS,
        )
    }
}

fn synthetic(name: &str, value: i32, label: &str) -> UStat {
    UStat {
        name: name.to_string(),
        value: Val::Lit(value),
        maximum: None,
        bonus: None,
        level: 1,
        label: label.to_string(),
        inline: false,
        req: Cond::None,
        equipped: Cond::None,
        owner_id: EXHAUSTION_ID.to_string(),
        owner_name: "Exhaustion".to_string(),
        value_text: value.to_string(),
        req_text: None,
        equipped_text: None,
    }
}

impl ExhaustionRules {
    fn with(d20: i32, speed: i32, max: u32) -> Self {
        let mut rules = Vec::new();
        for lvl in 1..=i32::try_from(max).unwrap_or(6) {
            rules.push(synthetic("speed", -speed * lvl, "Exhaustion"));
            rules.push(synthetic(D20_GROUP, -d20 * lvl, "Exhaustion"));
        }
        Self {
            d20_per_level: d20,
            speed_per_level: speed,
            max_levels: max,
            rules,
        }
    }

    /// Read the numbers from the `exhaustion` rule of the content (defaults otherwise).
    #[must_use]
    pub fn load(uni: &Universe) -> Self {
        for e in uni.iter() {
            for r in &e.el.rules {
                let Rule::Custom(c) = r else { continue };
                if c.kind != "exhaustion" {
                    continue;
                }
                let n = |k: &str, d: i64| c.fields.get(k).and_then(Value::as_i64).unwrap_or(d);
                return Self::with(
                    i32::try_from(n("d20_penalty_per_level", 2)).unwrap_or(2),
                    i32::try_from(n("speed_penalty_ft_per_level", 5)).unwrap_or(5),
                    u32::try_from(n("max_levels", 6)).unwrap_or(6),
                );
            }
        }
        Self::default()
    }
}

/// The stat rules of the current exhaustion level (linear model only).
pub(crate) fn exhaustion_stats<'u>(graph: &Graph<'u>) -> Vec<&'u UStat> {
    if graph.ruleset_hooks.exhaustion() != ExhaustionModel::Linear || graph.runtime.exhaustion == 0
    {
        return Vec::new();
    }
    let ex = graph.uni.exhaustion();
    let lvl = graph.runtime.exhaustion.min(ex.max_levels) as usize;
    ex.rules.iter().skip((lvl - 1) * 2).take(2).collect()
}

/// The exhaustion block of the snapshot.
#[must_use]
pub fn exhaustion_snapshot(graph: &Graph<'_>, stats: &StatSet) -> ExhaustionSnapshot {
    let ex = graph.uni.exhaustion();
    let linear = graph.ruleset_hooks.exhaustion() == ExhaustionModel::Linear;
    let level = graph.runtime.exhaustion.min(ex.max_levels);
    ExhaustionSnapshot {
        linear,
        level,
        max_levels: ex.max_levels,
        d20_penalty: stats.sum(D20_GROUP),
        speed_penalty: if linear {
            -ex.speed_per_level * i32::try_from(level).unwrap_or(0)
        } else {
            0
        },
        dead: linear && level >= ex.max_levels,
    }
}

// ------------------------------------------------------------------- toggles

/// Stat effects of one `toggle` rule.
#[derive(Debug, Clone)]
pub struct ToggleEffects {
    /// Toggle id.
    pub id: String,
    /// `requires_not_wearing` text.
    pub requires_not_wearing: Option<String>,
    /// Prebuilt stat rules.
    pub stats: Vec<UStat>,
}

fn field_str<'a>(c: &'a CustomRule, k: &str) -> Option<&'a str> {
    c.fields.get(k).and_then(Value::as_str)
}

/// Prebuild the stat rules inside the `effects` of the `toggle` rules of an element.
#[must_use]
pub fn toggle_stat_effects(el: &Element) -> Vec<ToggleEffects> {
    let mut out = Vec::new();
    for r in &el.rules {
        let Rule::Custom(c) = r else { continue };
        if c.kind != "toggle" {
            continue;
        }
        let Some(id) = field_str(c, "id") else {
            continue;
        };
        let mut stats = Vec::new();
        for eff in c
            .fields
            .get("effects")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if eff.get("kind").and_then(Value::as_str) != Some("stat") {
                continue;
            }
            let mut obj = eff.clone();
            if let Some(m) = obj.as_object_mut() {
                m.remove("kind");
            }
            let Ok(sr) = serde_json::from_value::<StatRule>(obj) else {
                continue;
            };
            stats.push(UStat {
                name: sr.name.trim().to_lowercase(),
                value: Val::parse(&sr.value),
                maximum: None,
                bonus: sr.bonus.clone().filter(|b| !b.is_empty()),
                level: sr.level.unwrap_or(1),
                label: el.name.clone(),
                inline: false,
                req: Cond::None,
                equipped: Cond::None,
                owner_id: el.id.clone(),
                owner_name: el.name.clone(),
                value_text: sr.value.trim().to_string(),
                req_text: None,
                equipped_text: None,
            });
        }
        out.push(ToggleEffects {
            id: id.to_string(),
            requires_not_wearing: field_str(c, "requires_not_wearing").map(str::to_string),
            stats,
        });
    }
    out
}

fn worn_blocker(graph: &Graph<'_>, requires_not_wearing: Option<&str>) -> Option<String> {
    let want = requires_not_wearing?.to_lowercase();
    let armor = graph.equip.armor.as_ref()?;
    let kind = armor.armor_kind.as_str();
    let hit = if want.contains("heavy") {
        kind == "heavy"
    } else if want.contains("armor") {
        !kind.is_empty()
    } else {
        false
    };
    hit.then(|| format!("wearing {}", want.trim_start_matches("wearing ")))
}

/// Stat rules of the switched-on toggles declared by a node.
pub(crate) fn active_toggle_stats<'u>(graph: &Graph<'u>, id: NodeId) -> Vec<&'u UStat> {
    let e: &'u UElem = graph.nodes[id].e;
    let mut out = Vec::new();
    for t in &e.toggle_effects {
        if graph.runtime.toggles.contains(&t.id)
            && worn_blocker(graph, t.requires_not_wearing.as_deref()).is_none()
        {
            out.extend(t.stats.iter());
        }
    }
    out
}

fn title_case(s: &str) -> String {
    s.split(['_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| {
                    f.to_uppercase()
                        .chain(c.flat_map(char::to_lowercase))
                        .collect()
                })
                .unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

fn describe_effect(eff: &Value, stats: &StatSet) -> String {
    let s = |k: &str| eff.get(k).and_then(Value::as_str).unwrap_or("");
    match s("kind") {
        "grant" => title_case(s("id").trim_start_matches("ID_INTERNAL_CONDITION_")),
        "roll_advantage" => format!(
            "Advantage on {} {}s",
            title_case(s("ability")),
            s("roll").replace('_', " ")
        ),
        "stat" => {
            let v = s("value");
            let n = v
                .parse::<i32>()
                .unwrap_or_else(|_| stats.sum(&v.to_lowercase()));
            format!("{} {n:+}", s("name"))
        }
        other => title_case(other),
    }
}

/// Toggles of the live elements.
#[must_use]
pub fn toggle_snapshots(graph: &Graph<'_>, stats: &StatSet) -> Vec<ToggleSnapshot> {
    let mut out: Vec<ToggleSnapshot> = Vec::new();
    for id in graph.preorder() {
        let el = &graph.nodes[id].e.el;
        for r in &el.rules {
            let Rule::Custom(c) = r else { continue };
            let Some(tid) = (c.kind == "toggle").then(|| field_str(c, "id")).flatten() else {
                continue;
            };
            if out.iter().any(|t| t.id == tid) {
                continue;
            }
            let on = graph.runtime.toggles.contains(tid);
            let blocked_by = worn_blocker(graph, field_str(c, "requires_not_wearing"));
            let list = |k: &str| -> Vec<Value> {
                c.fields
                    .get(k)
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
            };
            out.push(ToggleSnapshot {
                id: tid.to_string(),
                name: field_str(c, "name").unwrap_or(tid).to_string(),
                on,
                active: on && blocked_by.is_none(),
                blocked_by: blocked_by.filter(|_| on),
                activation: field_str(c, "activation").unwrap_or("").to_string(),
                duration: field_str(c, "duration").unwrap_or("").to_string(),
                blocks: list("blocks")
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect(),
                effects: list("effects")
                    .iter()
                    .map(|e| describe_effect(e, stats))
                    .collect(),
                source: el.name.clone(),
            });
        }
    }
    out
}

// ----------------------------------------------------------------- resources

struct ExprCtx<'a> {
    graph: &'a Graph<'a>,
    stats: &'a StatSet,
    mods: &'a [i32; 6],
}

fn split_top(expr: &str, ops: &[char]) -> Option<(String, char, String)> {
    let mut depth = 0i32;
    let chars: Vec<(usize, char)> = expr.char_indices().collect();
    for (i, ch) in chars.iter().rev() {
        match ch {
            ')' => depth += 1,
            '(' => depth -= 1,
            c if depth == 0 && ops.contains(c) && *i > 0 => {
                return Some((
                    expr[..*i].trim().to_string(),
                    *c,
                    expr[*i + c.len_utf8()..].trim().to_string(),
                ));
            }
            _ => {}
        }
    }
    None
}

fn split_commas(inner: &str) -> Vec<&str> {
    let mut depth = 0i32;
    let mut start = 0;
    let mut out = Vec::new();
    for (i, ch) in inner.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&inner[start..]);
    out
}

/// Evaluate a content formula (`5*cleric level`, `max(1, charisma:modifier)`, `ceil(druid level / 2)`).
pub(crate) fn eval_formula(
    graph: &Graph<'_>,
    stats: &StatSet,
    mods: &[i32; 6],
    expr: &str,
) -> Option<i32> {
    eval_expr(expr, &ExprCtx { graph, stats, mods })
}

fn eval_expr(expr: &str, cx: &ExprCtx<'_>) -> Option<i32> {
    let e = expr.trim();
    if let Some((l, '/', r)) = e
        .strip_prefix("ceil(")
        .and_then(|x| x.strip_suffix(')'))
        .and_then(|x| split_top(x, &['/']))
    {
        let (a, b) = (eval_expr(&l, cx)?, eval_expr(&r, cx)?);
        return (b > 0).then(|| (a + b - 1) / b);
    }
    if let Some((l, op, r)) = split_top(e, &['+', '-']) {
        let (a, b) = (eval_expr(&l, cx)?, eval_expr(&r, cx)?);
        return Some(if op == '+' { a + b } else { a - b });
    }
    if let Some((l, op, r)) = split_top(e, &['*', '/']) {
        let (a, b) = (eval_expr(&l, cx)?, eval_expr(&r, cx)?);
        return if op == '*' {
            Some(a * b)
        } else {
            a.checked_div(b)
        };
    }
    for f in ["max", "min", "floor"] {
        if let Some(inner) = e
            .strip_prefix(f)
            .and_then(|r| r.trim_start().strip_prefix('('))
            .and_then(|r| r.strip_suffix(')'))
        {
            let vals: Option<Vec<i32>> = split_commas(inner)
                .into_iter()
                .map(|p| eval_expr(p, cx))
                .collect();
            let vals = vals?;
            return match f {
                "max" => vals.into_iter().max(),
                "min" => vals.into_iter().min(),
                _ => vals.first().copied(),
            };
        }
    }
    if let Some(inner) = e.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
        return eval_expr(inner, cx);
    }
    if let Ok(n) = e.parse::<i32>() {
        return Some(n);
    }
    let lower = e.to_lowercase();
    if lower == "proficiency" || lower == "proficiency bonus" {
        return Some(cx.stats.sum("proficiency"));
    }
    if let Some(a) = lower.strip_suffix(":modifier") {
        let i = crate::ids::ability_index(a)?;
        return Some(cx.mods[i]);
    }
    if let Some(class) = lower.strip_suffix(" level") {
        return Some(
            cx.graph
                .class_levels
                .get(class.trim())
                .copied()
                .unwrap_or(0),
        );
    }
    None
}

fn class_name(el: &Element) -> Option<&str> {
    el.data.get("class").and_then(Value::as_str)
}

/// Evaluate a `max` descriptor.
fn resource_max(c: &CustomRule, el: &Element, cx: &ExprCtx<'_>) -> i32 {
    let Some(m) = c.fields.get("max") else {
        return 0;
    };
    let s = |k: &str| m.get(k).and_then(Value::as_str).unwrap_or("");
    match s("type") {
        "fixed" => m
            .get("value")
            .and_then(Value::as_i64)
            .and_then(|n| i32::try_from(n).ok())
            .unwrap_or(0),
        "stat" => {
            let name = s("stat").to_lowercase();
            cx.stats.sum(&name)
        }
        "table" => {
            let class = class_name(el).unwrap_or("");
            let level = cx
                .graph
                .class_levels
                .get(&class.to_lowercase())
                .copied()
                .unwrap_or(0);
            progression(
                cx.graph.uni,
                class,
                s("key"),
                u32::try_from(level).unwrap_or(0),
            )
            .unwrap_or(0)
        }
        "formula" => eval_expr(s("expr"), cx).unwrap_or(0).max(0),
        _ => 0,
    }
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// A `resource_regain` rule of a live element.
struct RegainRule {
    resource: String,
    on: Vec<String>,
    by: Option<String>,
    up_to: Option<i64>,
    amount: Option<Value>,
}

fn regain_rules(graph: &Graph<'_>) -> Vec<RegainRule> {
    let mut out = Vec::new();
    for id in graph.preorder() {
        for r in &graph.nodes[id].e.el.rules {
            let Rule::Custom(c) = r else { continue };
            if c.kind != "resource_regain" {
                continue;
            }
            out.push(RegainRule {
                resource: field_str(c, "resource").unwrap_or("").to_string(),
                on: strings(c.fields.get("on")),
                by: field_str(c, "by").map(str::to_string),
                up_to: c.fields.get("up_to").and_then(Value::as_i64),
                amount: c.fields.get("amount").cloned(),
            });
        }
    }
    out
}

fn describe_regain(r: &RegainRule) -> String {
    let amount = match &r.amount {
        Some(Value::Number(n)) => format!("{n}"),
        Some(Value::String(s)) => s.clone(),
        _ => "all".to_string(),
    };
    let trigger = if let Some(by) = &r.by {
        by.clone()
    } else {
        r.on.iter()
            .map(|o| o.replace('_', " "))
            .collect::<Vec<_>>()
            .join(" or ")
    };
    match r.up_to {
        Some(n) => format!("Regain uses on {trigger} (up to {n})"),
        None => format!("Regain {amount} on {trigger}"),
    }
}

/// Limited-use pools of the live elements.
#[must_use]
pub fn resource_snapshots(
    graph: &Graph<'_>,
    stats: &StatSet,
    mods: &[i32; 6],
) -> Vec<ResourceSnapshot> {
    let cx = ExprCtx { graph, stats, mods };
    let regains = regain_rules(graph);
    let mut out: Vec<ResourceSnapshot> = Vec::new();
    for id in graph.preorder() {
        let el = &graph.nodes[id].e.el;
        for r in &el.rules {
            let Rule::Custom(c) = r else { continue };
            if c.kind != "resource" {
                continue;
            }
            let Some(rid) = field_str(c, "id") else {
                continue;
            };
            let max = resource_max(c, el, &cx).max(0);
            let recharge = strings(c.fields.get("recharge"));
            let reg = c.fields.get("regain").filter(|v| !v.is_null());
            let short_listed = recharge.iter().any(|x| x == "short_rest");
            let mut short = if short_listed {
                let n = reg
                    .and_then(|g| g.get("short_rest"))
                    .and_then(Value::as_i64)
                    .and_then(|n| i32::try_from(n).ok());
                Some(n.map_or(max, |n| n.min(max)))
            } else {
                None
            };
            let mut other: Vec<String> = field_str(c, "alt_regain")
                .map(|t| vec![t.to_string()])
                .into_iter()
                .flatten()
                .collect();
            for g in regains.iter().filter(|g| g.resource == rid) {
                if g.on.iter().any(|o| o == "short_rest") {
                    short = Some(max);
                }
                other.push(describe_regain(g));
            }
            let snap = ResourceSnapshot {
                id: rid.to_string(),
                name: field_str(c, "name").unwrap_or(rid).to_string(),
                max,
                current: (max - i32::try_from(graph.runtime.used_of(rid)).unwrap_or(0)).max(0),
                source: el.name.clone(),
                long_rest_regain_all: recharge.iter().any(|x| x == "long_rest")
                    || recharge.is_empty(),
                recharge,
                short_rest_regain: short,
                other_regain: other,
            };
            if c.fields
                .get("replaces")
                .is_some_and(|v| !v.is_null() && v != &Value::Bool(false))
            {
                out.retain(|x| x.id != rid);
            } else if out.iter().any(|x| x.id == rid) {
                continue;
            }
            out.push(snap);
        }
    }
    out
}

/// Costs of options in resource points (Metamagic).
#[must_use]
pub fn resource_costs(graph: &Graph<'_>) -> Vec<ResourceCostSnapshot> {
    let mut out = Vec::new();
    for id in graph.preorder() {
        let el = &graph.nodes[id].e.el;
        for r in &el.rules {
            let Rule::Custom(c) = r else { continue };
            if c.kind != "resource_cost" {
                continue;
            }
            out.push(ResourceCostSnapshot {
                option: el.name.clone(),
                resource: field_str(c, "resource").unwrap_or("").to_string(),
                cost: c
                    .fields
                    .get("cost")
                    .and_then(Value::as_i64)
                    .and_then(|n| i32::try_from(n).ok())
                    .unwrap_or(0),
            });
        }
    }
    out
}

/// Features that grant Heroic Inspiration.
#[must_use]
pub fn inspiration_sources(graph: &Graph<'_>) -> Vec<InspirationSource> {
    let mut out = Vec::new();
    for id in graph.preorder() {
        let el = &graph.nodes[id].e.el;
        for r in &el.rules {
            let Rule::Custom(c) = r else { continue };
            if c.kind == "heroic_inspiration" {
                out.push(InspirationSource {
                    source: el.name.clone(),
                    trigger: field_str(c, "trigger").unwrap_or("").to_string(),
                });
            }
        }
    }
    out
}

// ------------------------------------------------------- martial arts / mastery

fn is_weapon_kind_prop(c: &Value, key: &str) -> bool {
    c.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// Monk Martial Arts of the character, if any.
#[must_use]
pub fn martial_arts(graph: &Graph<'_>) -> Option<MartialArtsSnapshot> {
    for id in graph.preorder() {
        let el = &graph.nodes[id].e.el;
        for r in &el.rules {
            let Rule::Custom(c) = r else { continue };
            if c.kind != "martial_arts" {
                continue;
            }
            let class = class_name(el).unwrap_or("Monk");
            let level = graph
                .class_levels
                .get(&class.to_lowercase())
                .copied()
                .unwrap_or(1);
            let key = field_str(c, "die_table").unwrap_or("martial_arts");
            let die = graph
                .uni
                .class_named(class)
                .and_then(|k| {
                    k.el.data
                        .get("progression")?
                        .get(key)?
                        .as_array()?
                        .get(usize::try_from(level - 1).ok()?)?
                        .as_str()
                        .map(str::to_string)
                })
                .unwrap_or_else(|| "1d6".to_string());
            let cond = c.fields.get("conditions").cloned().unwrap_or(Value::Null);
            let armor = graph
                .equip
                .armor
                .as_ref()
                .is_some_and(|a| !a.armor_kind.is_empty());
            let shield = graph.equip.secondary.as_ref().is_some_and(|s| s.shield)
                || graph.equip.primary.as_ref().is_some_and(|s| s.shield);
            let active = !(is_weapon_kind_prop(&cond, "no_armor") && armor)
                && !(is_weapon_kind_prop(&cond, "no_shield") && shield);
            return Some(MartialArtsSnapshot {
                die,
                active,
                bonus_unarmed_strike: is_weapon_kind_prop(
                    &Value::Object(c.fields.clone()),
                    "bonus_unarmed_strike",
                ),
            });
        }
    }
    None
}

pub(crate) fn plain_text(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Mastery property name and text of a weapon element.
#[must_use]
pub fn mastery_of(uni: &Universe, weapon: &Element) -> Option<(String, String)> {
    let mid = weapon.data.get("mastery_id")?.as_str()?;
    let m = uni.get(mid)?;
    Some((m.el.name.clone(), plain_text(&m.el.description)))
}

/// The weapons chosen for Weapon Mastery slots.
#[must_use]
pub fn mastery_picks(graph: &Graph<'_>) -> Vec<MasteryPick> {
    let mut out = Vec::new();
    for id in graph.preorder() {
        let n = &graph.nodes[id];
        // `Origin::Selected` carries the parent's slot index, not its rule index.
        let parent_is_mastery = n.parent.is_some_and(|p| {
            let parent = &graph.nodes[p];
            matches!(n.origin, crate::graph::Origin::Selected(si)
                if parent.slots.get(si).is_some_and(|slot|
                    matches!(parent.e.el.rules.get(slot.rule), Some(Rule::Select(s))
                        if crate::custom_rules::kind_of(s) == Some("mastery"))))
        });
        if !parent_is_mastery || out.iter().any(|p: &MasteryPick| p.weapon == n.e.el.id) {
            continue;
        }
        if let Some((property, text)) = mastery_of(graph.uni, &n.e.el) {
            out.push(MasteryPick {
                weapon: n.e.el.id.clone(),
                name: n.e.el.name.clone(),
                property,
                text,
            });
        }
    }
    out
}

/// The attack line mastery of a weapon (informational unless chosen).
#[must_use]
pub fn attack_mastery(
    uni: &Universe,
    weapon: &Element,
    picks: &[MasteryPick],
) -> Option<MasteryRef> {
    let (name, text) = mastery_of(uni, weapon)?;
    Some(MasteryRef {
        name,
        text,
        mastered: picks.iter().any(|p| p.weapon == weapon.id),
    })
}

/// Number of Weapon Mastery slots a rewritten select offers at a class level.
#[must_use]
pub fn mastery_slot_count(
    graph: &Graph<'_>,
    owner: NodeId,
    rule: &borealis_content::rules::SelectRule,
) -> u32 {
    let n = &graph.nodes[owner];
    let spec: Value = rule
        .extra
        .attributes
        .get(crate::custom_rules::X_COUNT)
        .and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or(Value::Null);
    match spec.get("type").and_then(Value::as_str) {
        Some("fixed") => spec
            .get("value")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(1),
        Some("table") => {
            let class = class_name(&n.e.el).unwrap_or("");
            let level = graph.mgr_level(n.mgr);
            progression(
                graph.uni,
                class,
                spec.get("key").and_then(Value::as_str).unwrap_or(""),
                level,
            )
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(1)
        }
        _ => 1,
    }
}

/// May `cand` fill a Weapon Mastery slot (`weapon_mastery.filter`)?
#[must_use]
pub fn mastery_allows(
    graph: &Graph<'_>,
    rule: &borealis_content::rules::SelectRule,
    cand: &UElem,
) -> bool {
    let filter: Value = rule
        .extra
        .attributes
        .get(crate::custom_rules::X_FILTER)
        .and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or(Value::Null);
    if !cand.el.data.contains_key("mastery_id") {
        return false;
    }
    let category = cand
        .el
        .data
        .get("weapon_category")
        .and_then(Value::as_str)
        .unwrap_or("");
    let melee = cand
        .el
        .data
        .get("melee")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if filter.get("weapon_kind").and_then(Value::as_str) == Some("melee") && !melee {
        return false;
    }
    let cats = strings(filter.get("categories"));
    if !cats.is_empty() && !cats.iter().any(|c| category.starts_with(c.as_str())) {
        return false;
    }
    let needs_prof = filter
        .get("proficient")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if needs_prof {
        if let Some(p) = cand.el.setter_value("proficiency") {
            return graph.has(p);
        }
    }
    true
}
