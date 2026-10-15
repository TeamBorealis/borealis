//! Spellcasting progression and class tables.

use super::build::{grant, stat_at};
use super::spell_choices::{spell_choices, ChoiceInput, SchoolRule};
use crate::core_ids;
use crate::text::strip_tags;
use borealis_content::{Rule, Spellcasting, SpellcastingList};
use serde_json::{json, Value};

/// Inputs describing the spellcasting of a class or subclass.
#[derive(Debug, Default)]
pub struct CasterInput<'a> {
    /// Class or subclass name (the spellcasting name and stat prefix).
    pub owner: &'a str,
    /// Class name used for `level:<class>` references.
    pub class_name: &'a str,
    /// Tag of the spell list (class name or subclass name).
    pub list_tag: &'a str,
    /// Ability abbreviation.
    pub ability: Option<&'a str>,
    /// `full`, `1/2`, `1/3`, `pact`, `artificer`.
    pub progression: Option<&'a str>,
    /// Table groups of the class or subclass.
    pub groups: &'a [Value],
    /// Cantrips known per level.
    pub cantrips: &'a [i64],
    /// Spells known per level (cumulative).
    pub known: &'a [i64],
    /// Spells gained per level (spellbook).
    pub known_fixed: &'a [i64],
    /// Prepared spell formula.
    pub prepared_formula: Option<&'a str>,
    /// Prepared spells per level (cumulative).
    pub prepared_progression: &'a [i64],
    /// Spellbook spells may be of any level with slots.
    pub book_allow_lower: bool,
    /// Spells of a fixed level learned at class levels (Mystic Arcanum).
    pub by_level: Option<&'a Value>,
    /// When prepared spells change (`level`, `restLong`).
    pub prepared_change: Option<&'a str>,
    /// School restriction of learned spells (2014 Eldritch Knight).
    pub schools: Option<&'a SchoolRule>,
    /// 2024 rules.
    pub is_2024: bool,
}

/// Result of [`caster`].
#[derive(Debug, Default)]
pub struct CasterOut {
    /// Stat rules and the multiclass slot grant.
    pub rules: Vec<Rule>,
    /// The `spellcasting` node.
    pub spellcasting: Option<Spellcasting>,
    /// Losses.
    pub partial: Vec<String>,
}

/// Numeric value of a table cell.
#[must_use]
pub fn cell_number(cell: &Value) -> Option<i64> {
    match cell {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => strip_tags(s).trim().trim_start_matches('+').parse().ok(),
        Value::Object(_) => cell.get("value").and_then(Value::as_i64),
        _ => None,
    }
}

/// Plain text of a table cell.
#[must_use]
pub fn cell_plain(cell: &Value) -> String {
    match cell {
        Value::String(s) => strip_tags(s),
        Value::Number(n) => n.to_string(),
        Value::Object(o) => match o.get("type").and_then(Value::as_str) {
            Some("bonus") => format!(
                "+{}",
                cell.get("value")
                    .map(ToString::to_string)
                    .unwrap_or_default()
            ),
            Some("dice") => cell
                .get("toRoll")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|r| {
                            format!(
                                "{}d{}",
                                r.get("number").unwrap_or(&json!(1)),
                                r.get("faces").unwrap_or(&json!(0))
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" + ")
                })
                .unwrap_or_default(),
            _ => cell.get("entry").map(cell_plain).unwrap_or_default(),
        },
        _ => String::new(),
    }
}

fn plain_label(label: &Value) -> String {
    strip_tags(label.as_str().unwrap_or("")).trim().to_string()
}

fn column_by_label(groups: &[Value], needle: &str) -> Option<Vec<i64>> {
    for g in groups {
        let labels = g.get("colLabels").and_then(Value::as_array)?;
        let rows = g.get("rows").and_then(Value::as_array)?;
        for (ci, l) in labels.iter().enumerate() {
            if plain_label(l).to_lowercase().contains(needle) {
                let vals: Vec<i64> = rows
                    .iter()
                    .filter_map(|r| r.get(ci).and_then(cell_number))
                    .collect();
                if vals.len() == rows.len() {
                    return Some(vals);
                }
            }
        }
    }
    None
}

/// Per-level increments of a cumulative series as stat rules.
fn delta_rules(name: &str, cumulative: &[i64], out: &mut Vec<Rule>) {
    let mut prev = 0i64;
    for (i, v) in cumulative.iter().enumerate() {
        let delta = v - prev;
        if delta != 0 {
            if let Ok(level) = u32::try_from(i + 1) {
                out.push(stat_at(name, delta, level));
            }
        }
        prev = *v;
    }
}

fn slot_matrix(groups: &[Value], progression: Option<&str>) -> Option<Vec<[i64; 9]>> {
    for g in groups {
        if let Some(rows) = g.get("rowsSpellProgression").and_then(Value::as_array) {
            let mut out = Vec::new();
            for r in rows {
                let cells = r.as_array()?;
                let mut row = [0i64; 9];
                for (j, c) in cells.iter().take(9).enumerate() {
                    row[j] = c.as_i64().unwrap_or(0);
                }
                out.push(row);
            }
            return Some(out);
        }
    }
    if progression == Some("pact") {
        for g in groups {
            let labels: Vec<String> = g
                .get("colLabels")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(|l| plain_label(l).to_lowercase()).collect())
                .unwrap_or_default();
            let slots = labels.iter().position(|l| l.contains("spell slots"));
            let level = labels.iter().position(|l| l.contains("slot level"));
            let (Some(si), Some(li)) = (slots, level) else {
                continue;
            };
            let rows = g.get("rows").and_then(Value::as_array)?;
            let mut out = Vec::new();
            for r in rows {
                let count = r.get(si).and_then(cell_number)?;
                let lvl = r.get(li).and_then(|c| match c {
                    Value::String(s) => strip_tags(s)
                        .chars()
                        .take_while(char::is_ascii_digit)
                        .collect::<String>()
                        .parse::<i64>()
                        .ok(),
                    other => cell_number(other),
                })?;
                let mut row = [0i64; 9];
                if (1..=9).contains(&lvl) {
                    row[usize::try_from(lvl - 1).ok()?] = count;
                }
                out.push(row);
            }
            return Some(out);
        }
    }
    None
}

fn prepared_formula_rules(
    owner_lower: &str,
    class_lower: &str,
    formula: &str,
    out: &mut Vec<Rule>,
) -> bool {
    let group = format!("{owner_lower}:spellcasting:prepare");
    let mut rules = Vec::new();
    for raw in formula.split('+') {
        let token = raw.trim();
        if token.is_empty() {
            return false;
        }
        let compact: String = token.chars().filter(|c| !c.is_whitespace()).collect();
        if let Ok(n) = compact.parse::<i64>() {
            rules.push(stat_at(&group, n, 1));
            continue;
        }
        let value = match compact.as_str() {
            "<$level$>" => format!("level:{class_lower}"),
            "<$level$>/2" | "floor(<$level$>/2)" => format!("level:{class_lower}:half"),
            "<$level$>/3" => format!("level:{class_lower}:third"),
            other => {
                let Some(inner) = other
                    .strip_prefix("<$")
                    .and_then(|r| r.strip_suffix("_mod$>"))
                else {
                    return false;
                };
                let Some(stat_name) = core_ids::ability_stat(inner) else {
                    return false;
                };
                format!("{stat_name}:modifier")
            }
        };
        rules.push(Rule::Stat(super::build::stat(&group, value)));
    }
    out.extend(rules);
    true
}

/// Pact Magic as the bundled packs write it: the slot count (`slots:count`) and the slot level
/// (`slot`) per class level, so the engine keeps the slots as their own pool.
fn pact_rules(owner: &str, matrix: &[[i64; 9]], out: &mut Vec<Rule>) {
    let count: Vec<i64> = matrix
        .iter()
        .map(|row| row.iter().copied().max().unwrap_or(0))
        .collect();
    let level: Vec<i64> = matrix
        .iter()
        .map(|row| {
            row.iter()
                .rposition(|n| *n > 0)
                .and_then(|i| i64::try_from(i + 1).ok())
                .unwrap_or(0)
        })
        .collect();
    delta_rules(&format!("{owner}:spellcasting:slots:count"), &count, out);
    delta_rules(&format!("{owner}:spellcasting:slot"), &level, out);
}

/// Build the spellcasting stat rules and `spellcasting` node.
#[must_use]
pub fn caster(input: &CasterInput<'_>) -> CasterOut {
    let mut out = CasterOut::default();
    let Some(progression) = input.progression else {
        return out;
    };
    let owner = input.owner.to_lowercase();
    let class_lower = input.class_name.to_lowercase();

    let mut max_slot_level = 0u32;
    match slot_matrix(input.groups, Some(progression)) {
        Some(matrix) => {
            for (j, level) in (1..=9u32).enumerate() {
                if matrix.iter().any(|row| row[j] > 0) {
                    max_slot_level = level;
                }
            }
            for j in 0..9usize {
                let name = format!("{owner}:spellcasting:slots:{}", j + 1);
                let series: Vec<i64> = matrix.iter().map(|row| row[j]).collect();
                delta_rules(&name, &series, &mut out.rules);
            }
            if progression == "pact" {
                pact_rules(&owner, &matrix, &mut out.rules);
            }
        }
        None => out
            .partial
            .push(format!("no spell slot table found for {}", input.owner)),
    }
    let cantrips: Vec<i64> = if input.cantrips.is_empty() {
        column_by_label(input.groups, "cantrip").unwrap_or_default()
    } else {
        input.cantrips.to_vec()
    };
    delta_rules(
        &format!("{owner}:spellcasting:cantrips known"),
        &cantrips,
        &mut out.rules,
    );
    let known: Vec<i64> = if input.known.is_empty() && input.known_fixed.is_empty() {
        column_by_label(input.groups, "spells known").unwrap_or_default()
    } else {
        input.known.to_vec()
    };
    delta_rules(
        &format!("{owner}:spellcasting:spells known"),
        &known,
        &mut out.rules,
    );
    for (i, n) in input.known_fixed.iter().enumerate() {
        if *n != 0 {
            if let Ok(level) = u32::try_from(i + 1) {
                out.rules.push(stat_at(
                    &format!("{owner}:spellcasting:spells known"),
                    *n,
                    level,
                ));
            }
        }
    }
    let mut prepared_series: Vec<i64> = input.prepared_progression.to_vec();
    if prepared_series.is_empty() && input.prepared_formula.is_none() {
        prepared_series = column_by_label(input.groups, "prepared spells").unwrap_or_default();
    }
    delta_rules(
        &format!("{owner}:spellcasting:prepare"),
        &prepared_series,
        &mut out.rules,
    );
    if let Some(formula) = input.prepared_formula {
        if !prepared_formula_rules(&owner, &class_lower, formula, &mut out.rules) {
            out.partial.push(format!(
                "prepared spell formula {formula:?} was not converted"
            ));
        }
    }

    // Multiclass spell slot pool.
    if let Some(id) = core_ids::multiclass_slot_grant(progression, input.is_2024) {
        let mut g = grant(id, "Grants");
        if let Err(e) = super::build::gate(&mut g, "ID_INTERNAL_GRANT_MULTICLASS") {
            out.partial.push(e);
        }
        out.rules.push(g);
    } else {
        out.partial.push(format!(
            "caster progression {progression:?} has no multiclass slot grant"
        ));
    }

    let prepares = input.prepared_formula.is_some() || !prepared_series.is_empty();
    let spellbook = !input.known_fixed.is_empty();
    let choices = spell_choices(&ChoiceInput {
        owner: input.owner,
        cantrips: &cantrips,
        known: &known,
        book: input.known_fixed,
        book_allow_lower: input.book_allow_lower,
        by_level: input.by_level,
        max_slot_level,
        prepares,
        progression,
        prepared_change: input.prepared_change,
        schools: input.schools,
        is_2024: input.is_2024,
    });
    out.rules.extend(choices.rules);
    out.partial.extend(choices.partial);
    // learned spells may be swapped on level up: 2014 known casters, every 2024 caster (as the
    // bundled classes)
    let allow_replace = input.is_2024 || !known.is_empty();
    out.spellcasting = Some(Spellcasting {
        name: Some(input.owner.to_string()),
        ability: input
            .ability
            .and_then(core_ids::ability_name)
            .map(str::to_string),
        prepare: Some(prepares),
        allow_replace: allow_replace.then_some(true),
        list: Some(SpellcastingList {
            supports: input.list_tag.to_string(),
            known: (prepares && !spellbook && known.is_empty()).then_some(true),
            ..SpellcastingList::default()
        }),
        ..Spellcasting::default()
    });
    out
}

/// Result of [`class_table`].
#[derive(Debug, Default)]
pub struct TableOut {
    /// Per-level increments as stat rules.
    pub rules: Vec<Rule>,
    /// The whole table as `{columns: [{label, values}]}` for display.
    pub data: Option<Value>,
}

const SPELL_COLUMN_WORDS: &[&str] = &[
    "cantrip",
    "spells known",
    "prepared spells",
    "spell slots",
    "slot level",
];

/// Stat rules for the numeric columns of a class table plus the table as data.
#[must_use]
pub fn class_table(owner: &str, groups: &[Value]) -> TableOut {
    let owner_lower = owner.to_lowercase();
    let mut out = TableOut::default();
    let mut columns = Vec::new();
    for g in groups {
        let Some(labels) = g.get("colLabels").and_then(Value::as_array) else {
            continue;
        };
        if let Some(rows) = g.get("rows").and_then(Value::as_array) {
            for (ci, l) in labels.iter().enumerate() {
                let label = plain_label(l);
                let values: Vec<String> = rows
                    .iter()
                    .map(|r| r.get(ci).map(cell_plain).unwrap_or_default())
                    .collect();
                columns.push(json!({"label": label, "values": values}));
                let lower = label.to_lowercase();
                if SPELL_COLUMN_WORDS.iter().any(|w| lower.contains(w)) {
                    continue;
                }
                let numbers: Vec<Option<i64>> = rows
                    .iter()
                    .map(|r| r.get(ci).and_then(cell_number))
                    .collect();
                if numbers.iter().all(Option::is_some) && !numbers.is_empty() {
                    let series: Vec<i64> = numbers.into_iter().flatten().collect();
                    let name = format!("{owner_lower}:{}", lower.replace(':', " "));
                    delta_rules(&name, &series, &mut out.rules);
                }
            }
        } else if g.get("rowsSpellProgression").is_some() {
            let slots: Vec<Vec<i64>> = g["rowsSpellProgression"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|r| {
                            r.as_array()
                                .map(|c| c.iter().map(|x| x.as_i64().unwrap_or(0)).collect())
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .unwrap_or_default();
            columns.push(json!({"label": "Spell Slots per Spell Level", "values": slots}));
        }
    }
    if !columns.is_empty() {
        out.data = Some(json!({"columns": columns}));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(rules: &[Rule]) -> Vec<(String, String, u32)> {
        rules
            .iter()
            .filter_map(|r| match r {
                Rule::Stat(s) => Some((s.name.clone(), s.value.clone(), s.level.unwrap_or(1))),
                _ => None,
            })
            .collect()
    }

    fn wizard_groups() -> Vec<Value> {
        vec![
            json!({"colLabels": ["{@filter Cantrips Known|spells|level=0}"], "rows": [[3],[3],[3],[4],[4]]}),
            json!({"title": "Spell Slots per Spell Level", "colLabels": ["1st","2nd","3rd"],
                   "rowsSpellProgression": [[2,0,0,0,0,0,0,0,0],[3,0,0,0,0,0,0,0,0],[4,2,0,0,0,0,0,0,0],[4,3,0,0,0,0,0,0,0],[4,3,2,0,0,0,0,0,0]]}),
        ]
    }

    #[test]
    fn wizard_like_progression() {
        let groups = wizard_groups();
        let out = caster(&CasterInput {
            owner: "Wizard",
            class_name: "Wizard",
            list_tag: "Wizard",
            ability: Some("int"),
            progression: Some("full"),
            groups: &groups,
            cantrips: &[3, 3, 3, 4, 4],
            known_fixed: &[6, 2, 2, 2, 2],
            book_allow_lower: true,
            prepared_formula: Some("<$level$> + <$int_mod$>"),
            ..CasterInput::default()
        });
        let picks: Vec<(String, u32)> = out
            .rules
            .iter()
            .filter_map(|r| match r {
                Rule::Select(s) if s.ty == "Spell" => Some((s.name.clone(), s.number.unwrap_or(1))),
                _ => None,
            })
            .collect();
        assert_eq!(picks[0], ("Cantrip (Wizard 1)".into(), 3));
        assert_eq!(picks[2], ("Spellbook (Wizard 1)".into(), 6));
        assert_eq!(picks.len(), 2 + 5, "{picks:?}");
        let s = stats(&out.rules);
        assert!(s.contains(&("wizard:spellcasting:slots:1".into(), "2".into(), 1)));
        assert!(s.contains(&("wizard:spellcasting:slots:1".into(), "1".into(), 2)));
        assert!(s.contains(&("wizard:spellcasting:slots:1".into(), "1".into(), 3)));
        assert!(s.contains(&("wizard:spellcasting:slots:2".into(), "2".into(), 3)));
        assert!(s.contains(&("wizard:spellcasting:slots:2".into(), "1".into(), 4)));
        assert!(s.contains(&("wizard:spellcasting:slots:3".into(), "2".into(), 5)));
        assert!(s.contains(&("wizard:spellcasting:cantrips known".into(), "3".into(), 1)));
        assert!(s.contains(&("wizard:spellcasting:cantrips known".into(), "1".into(), 4)));
        assert!(s.contains(&("wizard:spellcasting:spells known".into(), "6".into(), 1)));
        assert!(s.contains(&("wizard:spellcasting:spells known".into(), "2".into(), 2)));
        assert!(s.contains(&(
            "wizard:spellcasting:prepare".into(),
            "level:wizard".into(),
            1
        )));
        assert!(s.contains(&(
            "wizard:spellcasting:prepare".into(),
            "intelligence:modifier".into(),
            1
        )));
        let g = out
            .rules
            .iter()
            .find_map(|r| match r {
                Rule::Grant(g) => Some(g),
                _ => None,
            })
            .unwrap();
        assert_eq!(g.id, "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_FULL");
        assert_eq!(
            g.requirements.as_deref(),
            Some("ID_INTERNAL_GRANT_MULTICLASS")
        );
        let sc = out.spellcasting.unwrap();
        assert_eq!(sc.name.as_deref(), Some("Wizard"));
        assert_eq!(sc.ability.as_deref(), Some("Intelligence"));
        assert_eq!(sc.prepare, Some(true));
        assert_eq!(
            sc.list.unwrap().known,
            None,
            "spellbook casters prepare from known spells"
        );
        assert!(out.partial.is_empty());
    }

    #[test]
    fn list_prepared_pact_and_half_casters() {
        let cleric = caster(&CasterInput {
            owner: "Cleric",
            class_name: "Cleric",
            list_tag: "Cleric",
            ability: Some("wis"),
            progression: Some("full"),
            groups: &wizard_groups(),
            prepared_formula: Some("<$level$> + <$wis_mod$>"),
            ..CasterInput::default()
        });
        assert_eq!(cleric.spellcasting.unwrap().list.unwrap().known, Some(true));
        let groups = vec![
            json!({"colLabels": ["Spell Slots", "Slot Level"], "rows": [["1", "1st"], ["2", "1st"], ["2", {"type": "cell", "entry": "2nd"}], [2, "2nd"]]}),
        ];
        let _ = &groups;
        let groups = vec![
            json!({"colLabels": ["Cantrips Known", "Spells Known", "Spell Slots", "Slot Level"],
            "rows": [[2, 2, 1, "1st"], [2, 3, 2, "1st"], [2, 4, 2, "2nd"]]}),
        ];
        let warlock = caster(&CasterInput {
            owner: "Warlock",
            class_name: "Warlock",
            list_tag: "Warlock",
            ability: Some("cha"),
            progression: Some("pact"),
            groups: &groups,
            ..CasterInput::default()
        });
        let s = stats(&warlock.rules);
        assert!(s.contains(&("warlock:spellcasting:slots:1".into(), "1".into(), 1)));
        assert!(s.contains(&("warlock:spellcasting:slots:1".into(), "1".into(), 2)));
        assert!(s.contains(&("warlock:spellcasting:slots:1".into(), "-2".into(), 3)));
        assert!(s.contains(&("warlock:spellcasting:slots:2".into(), "2".into(), 3)));
        // a separate pool: slot count and slot level per class level
        assert!(s.contains(&("warlock:spellcasting:slots:count".into(), "1".into(), 1)));
        assert!(s.contains(&("warlock:spellcasting:slots:count".into(), "1".into(), 2)));
        assert!(s.contains(&("warlock:spellcasting:slot".into(), "1".into(), 1)));
        assert!(s.contains(&("warlock:spellcasting:slot".into(), "1".into(), 3)));
        assert!(!s.iter().any(|x| x.0 == "warlock:spellcasting:slot" && x.2 == 2));
        assert!(s.contains(&("warlock:spellcasting:cantrips known".into(), "2".into(), 1)));
        assert!(s.contains(&("warlock:spellcasting:spells known".into(), "1".into(), 2)));
        assert!(warlock
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Grant(g) if g.id.ends_with("SLOTS_SOLO"))));
        let sc = warlock.spellcasting.as_ref().unwrap();
        assert_eq!(sc.prepare, Some(false));
        assert_eq!(
            sc.allow_replace,
            Some(true),
            "known casters swap on level up"
        );
        let names: Vec<&str> = warlock
            .rules
            .iter()
            .filter_map(|r| match r {
                Rule::Select(s) => Some(s.name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            names,
            [
                "Cantrip (Warlock 1)",
                "Spell (Warlock 1)",
                "Spell (Warlock 2)",
                "Spell (Warlock 3)"
            ]
        );

        let half2014 = caster(&CasterInput {
            owner: "Paladin",
            class_name: "Paladin",
            list_tag: "Paladin",
            progression: Some("1/2"),
            groups: &wizard_groups(),
            ..CasterInput::default()
        });
        assert!(half2014
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Grant(g) if g.id.ends_with("SLOTS_HALF"))));
        let half2024 = caster(&CasterInput {
            owner: "Paladin",
            class_name: "Paladin",
            list_tag: "Paladin",
            progression: Some("1/2"),
            is_2024: true,
            groups: &wizard_groups(),
            ..CasterInput::default()
        });
        assert!(half2024
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Grant(g) if g.id.ends_with("SLOTS_HALF_UP"))));
    }

    #[test]
    fn prepared_progression_formulas_and_failures() {
        let out = caster(&CasterInput {
            owner: "Druid",
            class_name: "Druid",
            list_tag: "Druid",
            ability: Some("wis"),
            progression: Some("full"),
            groups: &wizard_groups(),
            prepared_progression: &[4, 5, 6, 7, 9],
            is_2024: true,
            ..CasterInput::default()
        });
        let s = stats(&out.rules);
        assert!(s.contains(&("druid:spellcasting:prepare".into(), "4".into(), 1)));
        assert!(s.contains(&("druid:spellcasting:prepare".into(), "2".into(), 5)));
        let bad = caster(&CasterInput {
            owner: "X",
            class_name: "X",
            list_tag: "X",
            progression: Some("full"),
            groups: &[],
            prepared_formula: Some("<$weird$> + 2"),
            ..CasterInput::default()
        });
        assert_eq!(bad.partial.len(), 2, "{:?}", bad.partial);
        let none = caster(&CasterInput::default());
        assert!(none.spellcasting.is_none() && none.rules.is_empty());
        let half = caster(&CasterInput {
            owner: "P",
            class_name: "P",
            list_tag: "P",
            progression: Some("full"),
            groups: &wizard_groups(),
            prepared_formula: Some("floor(<$level$> / 2) + <$cha_mod$> + 1"),
            ..CasterInput::default()
        });
        let s = stats(&half.rules);
        assert!(s.contains(&("p:spellcasting:prepare".into(), "level:p:half".into(), 1)));
        assert!(s.contains(&("p:spellcasting:prepare".into(), "1".into(), 1)));
        let odd = caster(&CasterInput {
            owner: "P",
            class_name: "P",
            list_tag: "P",
            progression: Some("odd"),
            groups: &wizard_groups(),
            ..CasterInput::default()
        });
        assert!(odd
            .partial
            .iter()
            .any(|p| p.contains("multiclass slot grant")));
    }

    #[test]
    fn class_tables() {
        let groups = vec![
            json!({"colLabels": ["Rages", "Rage Damage", "Sneak Attack", "Notes"], "rows": [
                ["2", {"type": "bonus", "value": 2}, {"type": "dice", "toRoll": [{"number": 1, "faces": 6}]}, "x"],
                ["2", {"type": "bonus", "value": 2}, {"type": "dice", "toRoll": [{"number": 1, "faces": 6}]}, "y"],
                ["3", {"type": "bonus", "value": 3}, {"type": "dice", "toRoll": [{"number": 2, "faces": 6}]}, "z"],
            ]}),
            json!({"colLabels": ["Cantrips Known"], "rows": [[2],[2],[3]]}),
        ];
        let t = class_table("Barbarian", &groups);
        let s = stats(&t.rules);
        assert_eq!(
            s,
            vec![
                ("barbarian:rages".to_string(), "2".to_string(), 1),
                ("barbarian:rages".to_string(), "1".to_string(), 3),
                ("barbarian:rage damage".to_string(), "2".to_string(), 1),
                ("barbarian:rage damage".to_string(), "1".to_string(), 3),
            ]
        );
        let data = t.data.unwrap();
        assert_eq!(data["columns"][2]["values"], json!(["1d6", "1d6", "2d6"]));
        assert_eq!(data["columns"][1]["values"][0], "+2");
        assert!(class_table("X", &[]).data.is_none());
        assert_eq!(cell_number(&json!("+3")), Some(3));
        assert_eq!(cell_number(&json!("Unlimited")), None);
    }
}
