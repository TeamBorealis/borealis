//! Generic machinery for the custom rule kinds of the 2024-era content (`ENGINE-NEEDS.md` 1):
//! `select_feat`, `grant_feat`, `select_param`, `select_size`, `ability_distribution` and
//! `ability_increase`.
//!
//! The kinds are rewritten in place into the core `select` / `grant` rules when an element enters
//! the universe, so rule indices (and with them slot keys) stay stable and the whole select
//! pipeline (slots, options, replay, snapshots) is reused. What the core rules cannot say travels
//! as `x-*` attributes in the rule's `extra`. Ability bonuses of a chosen list entry are
//! prebuilt stat rules ([`Effects`]).

use crate::ids::{ability_index, ABILITIES};
use crate::universe::{Cond, UStat, Val};
use borealis_content::rules::{CustomRule, GrantRule, ListItem, SelectRule};
use borealis_content::{Element, Rule};
use serde_json::Value;
use std::collections::BTreeMap;

/// Marker attribute: what a rewritten rule was (`feat`, `param`, `ability`).
pub const X_KIND: &str = "x-kind";
/// Select attribute: comma separated non-feat element ids that may fill a feat slot.
pub const X_ALTERNATIVES: &str = "x-alternatives";
/// Select attribute: parameters may not repeat across instances of the same feat.
pub const X_DISTINCT: &str = "x-distinct";
/// Grant attribute: JSON object of preset feat parameters.
pub const X_PARAMS: &str = "x-params";
/// Select attribute: normalised parameter key.
pub const X_KEY: &str = "x-key";
/// Grant attribute (`grant_spell`): JSON of the `free_cast` object.
pub const X_FREE_CAST: &str = "x-free-cast";
/// Grant attribute (`grant_spell`): the `mode` string.
pub const X_SPELL_MODE: &str = "x-spell-mode";
/// Grant attribute (`grant_spell`): the spell can also be cast with slots.
pub const X_ALSO_WITH_SLOTS: &str = "x-also-with-slots";
/// Grant attribute (`grant_spell`): the spellcasting ability (id or JSON object).
pub const X_SPELL_ABILITY: &str = "x-spell-ability";
/// Grant attribute (`grant_spell`): JSON object `{param name: value}`; the grant is live only
/// while the owning element's `select_param` values match (Circle of the Land).
pub const X_REQ_PARAM: &str = "x-requires-param";

/// Select attribute (`weapon_mastery`): JSON of the `count` descriptor.
pub const X_COUNT: &str = "x-count";
/// Select attribute (`weapon_mastery`): JSON of the `filter` object.
pub const X_FILTER: &str = "x-filter";

/// The default ability maximum.
const DEFAULT_ABILITY_CAP: i32 = 20;

/// Ability bonus rules of every list entry, per rule index.
pub type Effects = BTreeMap<usize, Vec<(i64, Vec<UStat>)>>;

/// Normalised parameter key: `Spell List` and `spell_list` are the same parameter.
#[must_use]
pub fn param_key(name: &str) -> String {
    name.trim().to_lowercase().replace([' ', '-'], "_")
}

/// Structured feat prerequisites (`Feat.data.prerequisite`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prereq {
    /// Minimum character level.
    pub level_min: Option<u32>,
    /// At least one of these (ability index, minimum score) must hold.
    pub abilities_any: Vec<(usize, i32)>,
    /// A live element with this name must exist (`Fighting Style`, `Spellcasting`).
    pub requires_feature: Option<String>,
}

impl Prereq {
    /// Read `data.prerequisite`.
    #[must_use]
    pub fn from_element(el: &Element) -> Self {
        let Some(p) = el.data.get("prerequisite").and_then(Value::as_object) else {
            return Self::default();
        };
        let abilities_any = p
            .get("abilities_any")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_object)
                    .flat_map(|o| o.iter())
                    .filter_map(|(k, v)| {
                        Some((ability_index(k)?, i32::try_from(v.as_i64()?).ok()?))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            level_min: p
                .get("level_min")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok()),
            abilities_any,
            requires_feature: p
                .get("requires_feature")
                .and_then(Value::as_str)
                .map(str::to_string),
        }
    }
}

/// `data.repeatable.allowed`.
#[must_use]
pub fn is_repeatable(el: &Element) -> bool {
    el.data
        .get("repeatable")
        .and_then(|r| r.get("allowed"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn str_field(c: &CustomRule, key: &str) -> Option<String> {
    c.fields
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn level_field(c: &CustomRule) -> Option<u32> {
    c.fields
        .get("level")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
}

fn marked(kind: &str) -> borealis_content::extra::Extra {
    let mut e = borealis_content::extra::Extra::default();
    e.attributes.insert(X_KIND.into(), kind.into());
    e
}

fn feat_slot(c: &CustomRule) -> SelectRule {
    let category = str_field(c, "category").unwrap_or_else(|| "General".into());
    let mut extra = marked("feat");
    if let Some(alt) = c.fields.get("alternatives").and_then(Value::as_array) {
        let ids: Vec<&str> = alt.iter().filter_map(Value::as_str).collect();
        extra
            .attributes
            .insert(X_ALTERNATIVES.into(), ids.join(","));
    }
    if c.fields.get("distinct").and_then(Value::as_bool) == Some(true) {
        extra.attributes.insert(X_DISTINCT.into(), "1".into());
    }
    SelectRule {
        ty: "Feat".into(),
        name: str_field(c, "name").unwrap_or_else(|| format!("{category} Feat")),
        level: level_field(c),
        requirements: str_field(c, "requirements"),
        supports: Some(format!("{category} Feat")),
        default_id: str_field(c, "default"),
        extra,
        ..SelectRule::default()
    }
}

fn grant_feat(c: &CustomRule) -> Option<GrantRule> {
    let mut extra = borealis_content::extra::Extra::default();
    if let Some(p) = c.fields.get("params").filter(|p| !is_empty_object(p)) {
        extra.attributes.insert(X_PARAMS.into(), p.to_string());
    }
    Some(GrantRule {
        id: str_field(c, "feat")?,
        ty: Some("Feat".into()),
        level: level_field(c),
        requirements: str_field(c, "requirements"),
        extra,
        ..GrantRule::default()
    })
}

/// `grant_spell` with a fixed spell becomes a core Spell grant (always-prepared when the mode says
/// so). Rules gated by `requirements_param` or naming no spell stay custom (deferred).
fn grant_spell(c: &CustomRule) -> Option<GrantRule> {
    let mode = str_field(c, "mode").unwrap_or_default();
    let mut extra = borealis_content::extra::Extra::default();
    extra.attributes.insert(X_SPELL_MODE.into(), mode.clone());
    for (key, attr) in [
        ("free_cast", X_FREE_CAST),
        ("ability", X_SPELL_ABILITY),
        ("also_with_slots", X_ALSO_WITH_SLOTS),
    ] {
        if let Some(v) = c.fields.get(key) {
            let text = v.as_str().map_or_else(|| v.to_string(), str::to_string);
            extra.attributes.insert(attr.into(), text);
        }
    }
    if let Some(v) = c.fields.get("requirements_param") {
        extra.attributes.insert(X_REQ_PARAM.into(), v.to_string());
    }
    Some(GrantRule {
        id: str_field(c, "spell")?,
        ty: Some("Spell".into()),
        level: level_field(c),
        requirements: str_field(c, "requirements"),
        spellcasting: str_field(c, "spell_list"),
        prepared: (mode == "always_prepared").then_some(true),
        extra,
        ..GrantRule::default()
    })
}

fn is_empty_object(v: &Value) -> bool {
    v.as_object().is_some_and(serde_json::Map::is_empty)
}

fn text_items(options: &[String]) -> Vec<ListItem> {
    options
        .iter()
        .zip(1i64..)
        .map(|(text, id)| ListItem {
            id,
            text: text.clone(),
            ..ListItem::default()
        })
        .collect()
}

fn string_list(c: &CustomRule, key: &str) -> Vec<String> {
    c.fields
        .get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn param_select(c: &CustomRule) -> Option<SelectRule> {
    let name = str_field(c, "name")?;
    let mut extra = marked("param");
    extra.attributes.insert(X_KEY.into(), param_key(&name));
    if c.fields
        .get("distinct_per_instance")
        .and_then(Value::as_bool)
        == Some(true)
    {
        extra.attributes.insert(X_DISTINCT.into(), "1".into());
    }
    Some(SelectRule {
        ty: "List".into(),
        items: text_items(&string_list(c, "options")),
        name,
        level: level_field(c),
        requirements: str_field(c, "requirements"),
        extra,
        ..SelectRule::default()
    })
}

fn size_select(c: &CustomRule) -> SelectRule {
    let ids: Vec<String> = string_list(c, "options")
        .iter()
        .map(|o| format!("ID_SIZE_{}", o.trim().to_uppercase()))
        .collect();
    SelectRule {
        ty: "Size".into(),
        name: str_field(c, "name").unwrap_or_else(|| "Size".into()),
        level: level_field(c),
        supports: Some(ids.join("|")),
        extra: marked("size"),
        ..SelectRule::default()
    }
}

/// A Weapon Mastery slot group: a `Weapon` select whose slot count follows the class table.
fn mastery_select(c: &CustomRule) -> SelectRule {
    let mut extra = marked("mastery");
    if let Some(v) = c.fields.get("count") {
        extra.attributes.insert(X_COUNT.into(), v.to_string());
    }
    if let Some(v) = c.fields.get("filter") {
        extra.attributes.insert(X_FILTER.into(), v.to_string());
    }
    SelectRule {
        ty: "Weapon".into(),
        name: str_field(c, "name").unwrap_or_else(|| "Weapon Mastery".into()),
        level: level_field(c),
        optional: Some(true),
        extra,
        ..SelectRule::default()
    }
}

/// `expertise`: pick `count` skills (or tools) the character is proficient in.
fn expertise_select(c: &CustomRule) -> SelectRule {
    SelectRule {
        ty: "Class Feature".into(),
        name: "Expertise".into(),
        number: c
            .fields
            .get("count")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok()),
        level: level_field(c),
        supports: Some("ID_INTERNAL_SUPPORT_EXPERTISE".into()),
        extra: marked("expertise"),
        ..SelectRule::default()
    }
}

fn title(ability: &str) -> String {
    let mut chars = ability.chars();
    chars
        .next()
        .map(|f| f.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// One ability choice: (ability, amount) pairs.
type Allocation = Vec<(String, i32)>;

fn all_abilities() -> Vec<String> {
    ABILITIES.iter().map(|a| (*a).to_string()).collect()
}

/// Ordered selections of `k` distinct items.
fn arrangements(pool: &[String], k: usize) -> Vec<Vec<String>> {
    if k == 0 {
        return vec![Vec::new()];
    }
    let mut out = Vec::new();
    for (i, first) in pool.iter().enumerate() {
        let rest: Vec<String> = pool
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, p)| p.clone())
            .collect();
        for mut tail in arrangements(&rest, k - 1) {
            tail.insert(0, first.clone());
            out.push(tail);
        }
    }
    out
}

/// Unordered selections of `k` distinct items, in pool order.
fn combinations(pool: &[String], k: usize) -> Vec<Vec<String>> {
    if k == 0 {
        return vec![Vec::new()];
    }
    let mut out = Vec::new();
    for i in 0..pool.len() {
        for mut tail in combinations(&pool[i + 1..], k - 1) {
            tail.insert(0, pool[i].clone());
            out.push(tail);
        }
    }
    out
}

fn distribution_choices(c: &CustomRule) -> Vec<Allocation> {
    let pool = string_list(c, "pool");
    let patterns: Vec<Vec<i32>> = c
        .fields
        .get("options")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_array)
                .map(|p| {
                    p.iter()
                        .filter_map(Value::as_i64)
                        .filter_map(|n| i32::try_from(n).ok())
                        .collect()
                })
                .collect()
        })
        .unwrap_or_default();
    let mut out: Vec<Allocation> = Vec::new();
    for pattern in patterns {
        for picked in arrangements(&pool, pattern.len()) {
            let mut alloc: Allocation = picked.into_iter().zip(pattern.iter().copied()).collect();
            alloc.sort_by_key(|(a, _)| ability_index(a));
            if !out.contains(&alloc) {
                out.push(alloc);
            }
        }
    }
    out
}

fn increase_choices(c: &CustomRule) -> Vec<Allocation> {
    let mut out: Vec<Allocation> = Vec::new();
    let options = c.fields.get("options").and_then(Value::as_array);
    for opt in options.into_iter().flatten().filter_map(Value::as_object) {
        let amount = opt
            .get("amount")
            .and_then(Value::as_i64)
            .and_then(|n| i32::try_from(n).ok())
            .unwrap_or(1);
        let (pool, count) = match opt.get("abilities") {
            Some(Value::Array(list)) => {
                let pool: Vec<String> = list
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect();
                let pick = opt.get("pick").and_then(Value::as_u64).unwrap_or(1);
                (pool, usize::try_from(pick).unwrap_or(1))
            }
            Some(n) => (
                all_abilities(),
                usize::try_from(n.as_u64().unwrap_or(1)).unwrap_or(1),
            ),
            None => (all_abilities(), 1),
        };
        for combo in combinations(&pool, count) {
            let alloc: Allocation = combo.into_iter().map(|a| (a, amount)).collect();
            if !out.contains(&alloc) {
                out.push(alloc);
            }
        }
    }
    out
}

fn describe(alloc: &Allocation) -> String {
    alloc
        .iter()
        .map(|(a, n)| format!("{} {n:+}", title(a)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn lit_stat(name: String, amount: i32, bonus: Option<&str>, owner: (&str, &str)) -> UStat {
    UStat {
        name,
        value: Val::Lit(amount),
        maximum: None,
        bonus: bonus.map(str::to_string),
        level: 1,
        label: owner.1.to_string(),
        inline: false,
        req: Cond::None,
        equipped: Cond::None,
        owner_id: owner.0.to_string(),
        owner_name: owner.1.to_string(),
        value_text: amount.to_string(),
        req_text: None,
        equipped_text: None,
    }
}

/// Stat rules of one allocation. Increases capped at the default maximum add to the ability
/// itself; a higher cap (Epic Boons) goes to `<ability>:boon` with its cap in
/// `<ability>:boon:cap`, so it is applied after the ordinary maximum.
fn effect_stats(alloc: &Allocation, cap: i32, owner: (&str, &str)) -> Vec<UStat> {
    let mut out = Vec::new();
    for (ability, amount) in alloc {
        let a = ability.to_lowercase();
        if cap > DEFAULT_ABILITY_CAP {
            out.push(lit_stat(format!("{a}:boon"), *amount, None, owner));
            out.push(lit_stat(format!("{a}:boon:cap"), cap, Some("base"), owner));
        } else {
            out.push(lit_stat(a, *amount, None, owner));
        }
    }
    out
}

fn ability_select(
    c: &CustomRule,
    owner: (&str, &str),
    distribution: bool,
) -> (SelectRule, Vec<(i64, Vec<UStat>)>) {
    let choices = if distribution {
        distribution_choices(c)
    } else {
        increase_choices(c)
    };
    let cap = c
        .fields
        .get("cap")
        .and_then(Value::as_i64)
        .and_then(|n| i32::try_from(n).ok())
        .unwrap_or(20);
    let mut items = Vec::new();
    let mut effects = Vec::new();
    for (alloc, id) in choices.iter().zip(1i64..) {
        items.push(ListItem {
            id,
            text: describe(alloc),
            ..ListItem::default()
        });
        effects.push((id, effect_stats(alloc, cap, owner)));
    }
    let default_name = if distribution {
        "Ability Scores"
    } else {
        "Ability Score Increase"
    };
    let rule = SelectRule {
        ty: "List".into(),
        name: str_field(c, "name").unwrap_or_else(|| default_name.into()),
        level: level_field(c),
        requirements: str_field(c, "requirements"),
        items,
        extra: marked("ability"),
        ..SelectRule::default()
    };
    (rule, effects)
}

/// Rewrite the custom kinds of `el` into core rules; returns the ability effects per rule index.
pub fn normalize(el: &mut Element) -> Effects {
    let mut effects = Effects::new();
    let (id, name) = (el.id.clone(), el.name.clone());
    for (ri, rule) in el.rules.iter_mut().enumerate() {
        let Rule::Custom(c) = rule else { continue };
        let replaced = match c.kind.as_str() {
            "select_feat" => Some(Rule::Select(feat_slot(c))),
            "grant_feat" => grant_feat(c).map(Rule::Grant),
            "grant_spell" => grant_spell(c).map(Rule::Grant),
            "select_param" => param_select(c).map(Rule::Select),
            "select_size" => Some(Rule::Select(size_select(c))),
            "weapon_mastery" => Some(Rule::Select(mastery_select(c))),
            "expertise" => Some(Rule::Select(expertise_select(c))),
            "ability_distribution" | "ability_increase" => {
                let (sel, items) = ability_select(
                    c,
                    (id.as_str(), name.as_str()),
                    c.kind == "ability_distribution",
                );
                effects.insert(ri, items);
                Some(Rule::Select(sel))
            }
            _ => None,
        };
        if let Some(r) = replaced {
            *rule = r;
        }
    }
    effects
}

/// The `x-kind` marker of a select rule.
#[must_use]
pub fn kind_of(rule: &SelectRule) -> Option<&str> {
    rule.extra.attributes.get(X_KIND).map(String::as_str)
}

/// Ids of the non-feat alternatives of a feat slot.
#[must_use]
pub fn alternatives_of(rule: &SelectRule) -> Vec<&str> {
    rule.extra
        .attributes
        .get(X_ALTERNATIVES)
        .map(|s| s.split(',').filter(|x| !x.is_empty()).collect())
        .unwrap_or_default()
}

/// Preset parameters of a granted feat, keyed by [`param_key`].
#[must_use]
pub fn preset_params(rule: &GrantRule) -> BTreeMap<String, String> {
    let Some(Value::Object(map)) = rule
        .extra
        .attributes
        .get(X_PARAMS)
        .and_then(|t| serde_json::from_str::<Value>(t).ok())
    else {
        return BTreeMap::new();
    };
    map.iter()
        .filter_map(|(k, v)| Some((param_key(k), v.as_str()?.to_string())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn custom(v: Value) -> CustomRule {
        let Value::Object(mut m) = v else {
            panic!("object")
        };
        let kind = m.remove("kind").and_then(|k| k.as_str().map(String::from));
        CustomRule {
            kind: kind.unwrap_or_default(),
            fields: m,
        }
    }

    #[test]
    fn distribution_enumerates_both_patterns() {
        let c = custom(json!({
            "kind": "ability_distribution",
            "pool": ["strength", "dexterity", "constitution"],
            "options": [[2, 1], [1, 1, 1]],
            "cap": 20
        }));
        let choices = distribution_choices(&c);
        assert_eq!(
            choices.len(),
            7,
            "six ordered +2/+1 pairs and the +1 triple"
        );
        assert!(choices.contains(&vec![("strength".into(), 2), ("constitution".into(), 1)]));
    }

    #[test]
    fn increase_enumerates_asi_and_restricted_pools() {
        let asi = custom(json!({
            "kind": "ability_increase",
            "options": [{"abilities": 1, "amount": 2}, {"abilities": 2, "amount": 1}],
            "cap": 20
        }));
        assert_eq!(increase_choices(&asi).len(), 6 + 15);
        let grappler = custom(json!({
            "kind": "ability_increase",
            "options": [{"abilities": ["strength", "dexterity"], "pick": 1, "amount": 1}]
        }));
        assert_eq!(increase_choices(&grappler).len(), 2);
    }

    #[test]
    fn param_keys_ignore_case_and_spaces() {
        assert_eq!(param_key("Spell List"), "spell_list");
        assert_eq!(param_key("spell_list"), "spell_list");
    }

    #[test]
    fn boon_increases_use_their_own_group_and_cap() {
        let alloc: Allocation = vec![("strength".into(), 1)];
        let stats = effect_stats(&alloc, 30, ("ID_X", "Boon"));
        assert_eq!(stats.len(), 2);
        assert_eq!(stats[0].name, "strength:boon");
        assert_eq!(stats[1].name, "strength:boon:cap");
        assert_eq!(effect_stats(&alloc, 20, ("ID_X", "Boon")).len(), 1);
    }
}
