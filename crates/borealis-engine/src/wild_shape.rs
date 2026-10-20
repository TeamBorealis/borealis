//! Wild Shape (`beast_forms`, ENGINE-NEEDS): the Beast stat blocks of the installed monster pack
//! that fit the Druid's current limits (maximum CR, flying) and the forms the character knows.

use crate::graph::Graph;
use crate::play_rules::eval_formula;
use crate::rule_view::live_rules;
use crate::stats::StatSet;
use crate::universe::Universe;
use borealis_content::monster_types::Cr;
use borealis_content::MonsterData;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

/// Stat summary of one Beast.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormSummary {
    /// Monster element id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Challenge rating text (`1/4`).
    pub cr: String,
    /// Size.
    pub size: String,
    /// Armor Class (first entry).
    pub ac: u32,
    /// Average hit points.
    pub hp: u32,
    /// Speeds, e.g. `40 ft., Fly 60 ft.`.
    pub speed: String,
    /// Has a Fly speed.
    pub flying: bool,
    /// Ability scores in the order Str, Dex, Con, Int, Wis, Cha.
    pub abilities: [u8; 6],
    /// Senses, e.g. `Darkvision 60 ft.`.
    pub senses: String,
}

/// The Wild Shape block of the snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WildShapeSnapshot {
    /// Feature that grants it.
    pub source: String,
    /// Druid level used for the limits.
    pub druid_level: u32,
    /// Known forms allowed by the class table.
    pub max_forms: u32,
    /// Highest challenge rating (`1/2`).
    pub max_cr: String,
    /// Flying forms are allowed.
    pub fly_allowed: bool,
    /// Duration in hours.
    pub duration_hours: i32,
    /// Temporary hit points gained on transforming.
    pub temp_hp: i32,
    /// When known forms may be changed (`long_rest`).
    pub change_on: String,
    /// Learned forms that still fit the limits.
    pub known: Vec<FormSummary>,
    /// Learned forms that no longer fit (the level or limits changed); not usable.
    pub invalid: Vec<String>,
    /// Every Beast that fits the limits.
    pub options: Vec<FormSummary>,
}

struct Limits {
    forms: u32,
    cr: f64,
    cr_text: String,
    fly: bool,
}

fn limits_at(rows: &[Value], level: u32) -> Option<Limits> {
    let row = rows
        .iter()
        .filter(|r| r.get("level").and_then(Value::as_u64).unwrap_or(0) <= u64::from(level))
        .max_by_key(|r| r.get("level").and_then(Value::as_u64).unwrap_or(0))?;
    let cr_text = row.get("max_cr").and_then(Value::as_str)?.to_string();
    Some(Limits {
        forms: u32::try_from(row.get("forms").and_then(Value::as_u64).unwrap_or(0)).unwrap_or(0),
        cr: Cr::parse(&cr_text)?.as_f64(),
        cr_text,
        fly: row.get("fly").and_then(Value::as_bool).unwrap_or(false),
    })
}

fn speed_text(m: &MonsterData) -> String {
    let s = &m.speed;
    let mut parts = Vec::new();
    if let Some(w) = s.walk {
        parts.push(format!("{w} ft."));
    }
    for (label, v) in [
        ("Burrow", s.burrow),
        ("Climb", s.climb),
        ("Fly", s.fly),
        ("Swim", s.swim),
    ] {
        if let Some(v) = v {
            parts.push(format!("{label} {v} ft."));
        }
    }
    parts.join(", ")
}

fn senses_text(m: &MonsterData) -> String {
    let s = &m.senses;
    [
        ("Blindsight", s.blindsight),
        ("Darkvision", s.darkvision),
        ("Tremorsense", s.tremorsense),
        ("Truesight", s.truesight),
    ]
    .iter()
    .filter_map(|(n, v)| v.map(|v| format!("{n} {v} ft.")))
    .collect::<Vec<_>>()
    .join(", ")
}

/// Every Beast stat block of the universe with a challenge rating, summarised, with its CR. A
/// beast without one (decision IM12) has no number to compare with the form limit: never a form.
fn beasts(uni: &Universe) -> Vec<(FormSummary, f64)> {
    let mut out = Vec::new();
    for u in uni.by_type("Monster") {
        let Ok(m) = MonsterData::from_element(&u.el) else {
            continue;
        };
        let Some(cr) = m.cr else {
            continue;
        };
        if m.creature_type.as_str() != "beast" {
            continue;
        }
        out.push((
            FormSummary {
                id: u.el.id.clone(),
                name: u.el.name.clone(),
                cr: cr.as_text(),
                size: m
                    .size
                    .first()
                    .map_or(String::new(), |s| s.as_str().to_string()),
                ac: m.ac.first().map_or(0, |a| a.value),
                hp: m.hp.average,
                speed: speed_text(&m),
                flying: m.speed.fly.is_some(),
                abilities: [
                    m.abilities.str,
                    m.abilities.dex,
                    m.abilities.con,
                    m.abilities.int,
                    m.abilities.wis,
                    m.abilities.cha,
                ],
                senses: senses_text(&m),
            },
            cr.as_f64(),
        ));
    }
    out
}

/// The Wild Shape block, when a live `beast_forms` rule exists.
#[must_use]
pub fn wild_shape(
    graph: &Graph<'_>,
    uni: &Universe,
    stats: &StatSet,
    mods: &[i32; 6],
    known: &BTreeSet<String>,
) -> Option<WildShapeSnapshot> {
    let live = live_rules(graph, &["beast_forms"]);
    let l = live.first()?;
    let level = graph.mgr_level(graph.nodes[l.node].mgr);
    let rows = l.c.fields.get("known").and_then(Value::as_array)?;
    let lim = limits_at(rows, level)?;
    let eval = |k: &str| {
        l.str(k)
            .and_then(|e| eval_formula(graph, stats, mods, e))
            .unwrap_or(0)
    };
    let mut options: Vec<FormSummary> = beasts(uni)
        .into_iter()
        .filter(|(f, cr)| *cr <= lim.cr && (lim.fly || !f.flying))
        .map(|(f, _)| f)
        .collect();
    options.sort_by(|a, b| a.name.cmp(&b.name));
    let (known_ok, invalid): (Vec<&String>, Vec<&String>) = known
        .iter()
        .partition(|id| options.iter().any(|o| &o.id == *id));
    Some(WildShapeSnapshot {
        source: l.el.name.clone(),
        druid_level: level,
        max_forms: lim.forms,
        max_cr: lim.cr_text.clone(),
        fly_allowed: lim.fly,
        duration_hours: eval("duration_hours"),
        temp_hp: eval("temp_hp"),
        change_on: l.str("change_on").unwrap_or("").to_string(),
        known: known_ok
            .iter()
            .filter_map(|id| options.iter().find(|o| &o.id == *id).cloned())
            .collect(),
        invalid: invalid.into_iter().cloned().collect(),
        options,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::{ContentFile, ContentStoreBuilder, Element, PackOrigin, Ruleset};
    use serde_json::json;

    /// A synthetic beast stat block, with a CR or (decision IM12) without one.
    fn beast(id: &str, cr: Option<&str>) -> Element {
        let mut data = json!({"ruleset": "dnd2014", "size": ["small"], "creature_type": "beast",
            "alignment": "unaligned", "ac": [{"value": 12}], "hp": {"average": 9},
            "speed": {"walk": 30},
            "abilities": {"str": 10, "dex": 12, "con": 10, "int": 3, "wis": 10, "cha": 5},
            "senses": {"passive_perception": 10}});
        if let Some(cr) = cr {
            data["cr"] = json!(cr);
            data["xp"] = json!(25);
            data["proficiency_bonus"] = json!(2);
        } else {
            data["cr_note"] = json!("Scales with spell level");
        }
        let m: MonsterData = serde_json::from_value(data).expect("stat block");
        m.to_element(id, id, "Test")
    }

    #[test]
    fn a_beast_without_a_challenge_rating_is_never_a_form() {
        let file = ContentFile {
            elements: vec![
                beast("ID_TEST_RATED", Some("1/8")),
                beast("ID_TEST_UNRATED", None),
            ],
            ..ContentFile::default()
        };
        let store = ContentStoreBuilder::new()
            .add_file("beasts.json", PackOrigin::Custom, file)
            .build();
        let uni = Universe::new(&store, Ruleset::Dnd2014);
        let ids: Vec<String> = beasts(&uni).into_iter().map(|(f, _)| f.id).collect();
        assert_eq!(ids, ["ID_TEST_RATED"]);
    }
}
