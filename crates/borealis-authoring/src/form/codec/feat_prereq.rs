//! A feat's prerequisite (spec 4.2): one condition builder bound to `requirements`, the 2024 structured
//! `data.prerequisite` (`level_min`, `abilities_any`, `requires_feature`, read by `borealis_engine::custom_rules::Prereq`)
//! and the display sentence `prerequisite`, which is written from the condition until the author rewords it.
//!
//! Reading joins the two engine locations: the terms of `data.prerequisite` that `requirements` does not say already
//! are added to the condition (all of them hold for the engine, so the builder shows all of them). Writing puts the
//! whole condition in `requirements` and, when the element had the structured mirror, the part of it the mirror can
//! hold (the terms every character must meet: character level, one "any of these abilities", one feature).

use crate::expr_build::condition::{Condition, ConditionContext, ConditionLanguage, ConditionRow};
use crate::expr_build::{Ability, Built, ExprLanguage, ExprValue, GroupMode, Node};
use crate::form::Lookup;
use serde_json::{json, Map, Value};

pub(super) const LEVEL_MIN: &str = "level_min";
pub(super) const ABILITIES_ANY: &str = "abilities_any";
pub(super) const REQUIRES_FEATURE: &str = "requires_feature";
const KNOWN: [&str; 3] = [LEVEL_MIN, ABILITIES_ANY, REQUIRES_FEATURE];
/// The structured prerequisite "Spellcasting" is the strict class-spell-slot rule, the condition `[spellcasting:1]`.
const SPELLCASTING: &str = "Spellcasting";

/// The rows a structured prerequisite means, in the engine's order (level, abilities, feature).
fn rows_of(p: &Map<String, Value>) -> Vec<(&'static str, Condition)> {
    let mut out = Vec::new();
    if let Some(n) = p.get(LEVEL_MIN).and_then(Value::as_i64) {
        out.push((
            LEVEL_MIN,
            Node::row(ConditionRow::CharacterLevel { min: n }),
        ));
    }
    let abilities: Vec<Condition> = p
        .get(ABILITIES_ANY)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .flat_map(|o| o.iter())
        .filter_map(|(k, v)| {
            Some(Node::row(ConditionRow::Ability {
                ability: Ability::from_key(k)?,
                min: v.as_i64()?,
            }))
        })
        .collect();
    match abilities.len() {
        0 => {}
        1 => out.push((ABILITIES_ANY, abilities.into_iter().next().expect("one"))),
        _ => out.push((ABILITIES_ANY, Node::group(GroupMode::Any, abilities))),
    }
    if let Some(f) = p.get(REQUIRES_FEATURE).and_then(Value::as_str) {
        let f = f.trim();
        let row = if f.eq_ignore_ascii_case(SPELLCASTING) {
            ConditionRow::Spellcasting { level: 1 }
        } else {
            ConditionRow::Feature {
                name: f.to_string(),
            }
        };
        out.push((REQUIRES_FEATURE, Node::row(row)));
    }
    out
}

/// The top-level terms of a condition: the items of an All of root, else the root itself.
fn top_terms(model: &Condition) -> Vec<&Condition> {
    match model {
        Node::Group {
            mode: GroupMode::All,
            items,
        } => items.iter().collect(),
        other => vec![other],
    }
}

fn ability_term(node: &Condition) -> Option<Vec<(Ability, i64)>> {
    let one = |n: &Condition| match n {
        Node::Row {
            row: ConditionRow::Ability { ability, min },
        } => Some((*ability, *min)),
        _ => None,
    };
    match node {
        Node::Group {
            mode: GroupMode::Any | GroupMode::All,
            items,
        } if items.len() == 1 => ability_term(&items[0]),
        Node::Group {
            mode: GroupMode::Any,
            items,
        } if !items.is_empty() => items.iter().map(one).collect(),
        row => one(row).map(|a| vec![a]),
    }
}

/// The structured prerequisite a condition holds: only terms every character must meet, so it never asks more than
/// the condition does.
pub(super) fn structured(model: &Condition) -> Map<String, Value> {
    let mut level = None;
    let mut abilities = None;
    let mut feature = None;
    for t in top_terms(model) {
        match t {
            Node::Row {
                row: ConditionRow::CharacterLevel { min },
            } => {
                level.get_or_insert(*min);
            }
            Node::Row {
                row: ConditionRow::Spellcasting { level: l },
            } if *l <= 1 => {
                feature.get_or_insert_with(|| SPELLCASTING.to_string());
            }
            Node::Row {
                row: ConditionRow::Feature { name },
            } if !name.trim().is_empty() => {
                feature.get_or_insert_with(|| name.trim().to_string());
            }
            other => {
                if abilities.is_none() {
                    abilities = ability_term(other);
                }
            }
        }
    }
    let mut out = Map::new();
    if let Some(n) = level {
        out.insert(LEVEL_MIN.into(), json!(n));
    }
    if let Some(list) = abilities {
        let list: Vec<Value> = list
            .into_iter()
            .map(|(a, min)| json!({ a.full(): min }))
            .collect();
        out.insert(ABILITIES_ANY.into(), Value::Array(list));
    }
    if let Some(f) = feature {
        out.insert(REQUIRES_FEATURE.into(), json!(f));
    }
    out
}

/// The same structured prerequisite, compared by what it means (ability lists in any order, any case).
fn same_term(key: &str, a: Option<&Value>, b: Option<&Value>) -> bool {
    let norm = |v: Option<&Value>| -> Option<Value> {
        let v = v?;
        Some(match key {
            ABILITIES_ANY => {
                let mut pairs: Vec<(Ability, i64)> = v
                    .as_array()?
                    .iter()
                    .filter_map(Value::as_object)
                    .flat_map(|o| o.iter())
                    .filter_map(|(k, n)| Some((Ability::from_key(k)?, n.as_i64()?)))
                    .collect();
                pairs.sort();
                json!(pairs
                    .iter()
                    .map(|(a, n)| format!("{}:{n}", a.short()))
                    .collect::<Vec<_>>())
            }
            REQUIRES_FEATURE => json!(v.as_str()?.trim().to_lowercase()),
            _ => v.clone(),
        })
    };
    norm(a) == norm(b)
}

/// The condition the feat's two engine locations mean together, as the builder shows it.
#[must_use]
pub(super) fn condition(
    requirements: &str,
    data: Option<&Value>,
    cx: &ConditionContext,
) -> ExprValue<Condition> {
    let read = ExprValue::read::<ConditionLanguage>(requirements, cx);
    let (Built::Model { model }, Some(p)) = (&read.value, data.and_then(Value::as_object)) else {
        return read;
    };
    let said = structured(model);
    let extra: Vec<Condition> = rows_of(p)
        .into_iter()
        .filter(|(key, _)| !same_term(key, p.get(*key), said.get(*key)))
        .map(|(_, row)| row)
        .collect();
    if extra.is_empty() {
        return read;
    }
    let mut items: Vec<Condition> = match model {
        Node::Group {
            mode: GroupMode::All,
            items,
        } => items.clone(),
        other if other.is_empty() => Vec::new(),
        other => vec![other.clone()],
    };
    items.extend(extra);
    // the model the joined text reads as, so the canonical text reads back the same (spec 18.2)
    let text = ConditionLanguage::render(&Node::group(GroupMode::All, items), cx);
    ExprValue {
        value: ConditionLanguage::parse(&text, cx),
        original: read.original,
    }
}

/// `data.prerequisite` for an edited condition: the stored object's other keys, then what the condition holds.
pub(super) fn mirror(stored: &Value, model: &Condition) -> Value {
    let mut out: Map<String, Value> = stored
        .as_object()
        .map(|o| {
            o.iter()
                .filter(|(k, _)| !KNOWN.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        })
        .unwrap_or_default();
    out.extend(structured(model));
    Value::Object(out)
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

fn ability_name(a: Ability) -> String {
    capital(a.full())
}

/// The words of a speed statistic's movement mode (`innate speed:fly` -> "flying"), as the SRD names a speed.
fn speed_mode(stat: &str) -> Option<&'static str> {
    let stat = stat.trim().to_lowercase();
    match stat.strip_prefix("innate ").unwrap_or(&stat) {
        "speed" => Some("walking"),
        "speed:fly" => Some("flying"),
        "speed:swim" => Some("swimming"),
        "speed:climb" => Some("climbing"),
        "speed:burrow" => Some("burrowing"),
        _ => None,
    }
}

/// A statistic row in words (IqzaQ): a speed as authors and the SRD write it ("A flying speed", "Climbing speed 30+
/// ft."), any other statistic by its label in the statistic picker ("Darkvision range 60+").
fn statistic_phrase(stat: &str, min: i64) -> String {
    match speed_mode(stat) {
        Some(mode) if min <= 1 => format!("A {mode} speed"),
        Some(mode) => format!("{} speed {min}+ ft.", capital(mode)),
        None => format!("{} {min}+", crate::stats::label(stat)),
    }
}

fn row_phrase(row: &ConditionRow, lookup: &dyn Lookup) -> Option<String> {
    Some(match row {
        ConditionRow::CharacterLevel { min } | ConditionRow::Level { min } => {
            format!("Level {min}+")
        }
        ConditionRow::Ability { ability, min } => format!("{} {min}+", ability_name(*ability)),
        ConditionRow::ClassLevel { class, min } => format!("{} level {min}+", capital(class)),
        ConditionRow::Feature { name } => format!("{} Feature", name.trim()),
        ConditionRow::Spellcasting { level } if *level <= 1 => "Spellcasting Feature".into(),
        ConditionRow::Spellcasting { level } => format!("Spell slots of level {level}"),
        ConditionRow::Statistic { stat, min } => statistic_phrase(stat, *min),
        ConditionRow::Has { id } => lookup.find(id)?.name,
        ConditionRow::Lacks { id } => format!("No {}", lookup.find(id)?.name),
        ConditionRow::Type { .. } | ConditionRow::Kept { .. } => return None,
    })
}

fn phrase(node: &Condition, lookup: &dyn Lookup) -> Option<String> {
    match node {
        Node::Row { row } => row_phrase(row, lookup),
        Node::Group { items, .. } if items.is_empty() => None,
        Node::Group { items, .. }
            if items.len() == 1
                && !matches!(
                    node,
                    Node::Group {
                        mode: GroupMode::None,
                        ..
                    }
                ) =>
        {
            phrase(&items[0], lookup)
        }
        Node::Group { mode, items } => {
            if *mode == GroupMode::Any {
                if let Some(list) = ability_term(node) {
                    let min = list[0].1;
                    if list.iter().all(|(_, m)| *m == min) {
                        let names: Vec<String> =
                            list.iter().map(|(a, _)| ability_name(*a)).collect();
                        return Some(format!("{} {min}+", names.join(" or ")));
                    }
                }
            }
            let parts: Vec<String> = items
                .iter()
                .map(|i| phrase(i, lookup))
                .collect::<Option<_>>()?;
            Some(match mode {
                GroupMode::All => parts.join(", "),
                GroupMode::Any => parts.join(" or "),
                GroupMode::None => format!("Not {}", parts.join(" or ")),
            })
        }
    }
}

/// The display sentence of a condition, as the 2024 SRD writes it ("Level 4+, Strength or Dexterity 13+",
/// "Fighting Style Feature"); `None` for no condition or one with a part words can't say (a test kept as is).
#[must_use]
pub(super) fn sentence(value: &ExprValue<Condition>, lookup: &dyn Lookup) -> Option<String> {
    match &value.value {
        Built::Model { model } => phrase(model, lookup),
        Built::Kept { .. } => None,
    }
}
