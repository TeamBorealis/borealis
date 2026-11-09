//! The saving throws, senses, languages and damage and condition lines of a companion creature
//! (GitHub #40), read from a bundled `Monster` stat block or from an Aurora `Companion` element.
//! Aurora's companion page prints the six saving throws, then (after the skills) the damage and
//! condition lines, `Senses` when the creature has some, and `Languages`.

use crate::companion_block::modifier;
use crate::content_dto::{CompanionFeature, CompanionSkill};
use borealis_content::monster_types::{Ability, DamageClause};
use borealis_content::{Element, MonsterData, Rule};

/// Ability name, abbreviation, stat block key and the Aurora statistic and setter name, STR to CHA.
const ABILITIES: [(&str, &str, Ability, &str); 6] = [
    ("Strength", "STR", Ability::Str, "strength"),
    ("Dexterity", "DEX", Ability::Dex, "dexterity"),
    ("Constitution", "CON", Ability::Con, "constitution"),
    ("Intelligence", "INT", Ability::Int, "intelligence"),
    ("Wisdom", "WIS", Ability::Wis, "wisdom"),
    ("Charisma", "CHA", Ability::Cha, "charisma"),
];
/// Aurora's Languages text when a `Companion` element has none.
const NO_LANGUAGES: &str = "\u{2014}";
/// The statistic a `Companion` element's proficiency rules point at (Aurora seeds it with the
/// creature's proficiency bonus).
const COMPANION_PROFICIENCY: &str = "companion:proficiency";
const DEFAULT_SCORE: i32 = 10;

/// Saves, senses, languages and the damage and condition lines of one creature.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatLines {
    /// Saving throw bonuses the stat block lists (the proficient saves), STR to CHA; the
    /// others are the ability modifier.
    pub save_bonuses: [Option<i32>; 6],
    /// Special senses (`Darkvision 60 ft.`), empty when there are none.
    pub senses: String,
    /// Languages as printed.
    pub languages: String,
    /// `Damage Resistances`: `Poison`, in Aurora's order.
    pub defenses: Vec<CompanionFeature>,
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().collect::<String>() + chars.as_str()
    })
}

fn clauses(list: &[DamageClause]) -> String {
    list.iter()
        .map(|c| match c.text.as_deref().map(str::trim) {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => c
                .types
                .iter()
                .map(|t| capitalize(t.as_str()))
                .collect::<Vec<_>>()
                .join(", "),
        })
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
}

fn defense_lines(pairs: &[(&str, String)]) -> Vec<CompanionFeature> {
    pairs
        .iter()
        .filter(|(_, text)| !text.trim().is_empty())
        .map(|(name, text)| CompanionFeature {
            name: (*name).to_string(),
            text: text.trim().to_string(),
        })
        .collect()
}

/// `Blindsight 10 ft. (blind beyond this radius), Darkvision 60 ft.` from a stat block.
fn monster_senses(m: &MonsterData) -> String {
    let s = &m.senses;
    let mut parts = Vec::new();
    if let Some(range) = s.blindsight {
        let mut text = format!("Blindsight {range} ft.");
        if let Some(deaf) = s.deafened_range {
            text.push_str(&format!(" or {deaf} ft. while deafened"));
        }
        if s.blind_beyond {
            text.push_str(" (blind beyond this radius)");
        }
        parts.push(text);
    }
    for (name, range) in [
        ("Darkvision", s.darkvision),
        ("Tremorsense", s.tremorsense),
        ("Truesight", s.truesight),
    ] {
        if let Some(range) = range {
            parts.push(format!("{name} {range} ft."));
        }
    }
    for note in s.notes.iter().map(|n| n.trim()).filter(|n| !n.is_empty()) {
        // `darkvision: rat form only` belongs to that sense: `Darkvision 60 ft. (rat form only)`.
        let attached = note.split_once(':').and_then(|(sense, rest)| {
            let part = parts.iter_mut().find(|p| {
                p.split(' ')
                    .next()
                    .is_some_and(|n| n.eq_ignore_ascii_case(sense.trim()))
            })?;
            part.push_str(&format!(" ({})", rest.trim()));
            Some(())
        });
        if attached.is_none() {
            parts.push(note.to_string());
        }
    }
    parts.join(", ")
}

fn monster_languages(m: &MonsterData) -> String {
    let mut parts: Vec<String> = m.languages.iter().map(|l| l.trim().to_string()).collect();
    if let Some(range) = m.telepathy {
        parts.push(format!("telepathy {range} ft."));
    }
    if parts.is_empty() {
        NO_LANGUAGES.to_string()
    } else {
        parts.join(", ")
    }
}

/// The lines of a bundled stat block.
#[must_use]
pub fn monster_lines(m: &MonsterData) -> StatLines {
    StatLines {
        save_bonuses: ABILITIES.map(|(_, _, a, _)| m.saves.get(&a).copied()),
        senses: monster_senses(m),
        languages: monster_languages(m),
        defenses: defense_lines(&[
            ("Damage Vulnerabilities", clauses(&m.damage_vulnerabilities)),
            ("Damage Resistances", clauses(&m.damage_resistances)),
            ("Damage Immunities", clauses(&m.damage_immunities)),
            (
                "Condition Immunities",
                m.condition_immunities
                    .iter()
                    .map(|c| capitalize(c.trim()))
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        ]),
    }
}

/// Index (STR..CHA) of `Dex`, `dexterity` or `DEX`.
fn ability_index(name: &str) -> Option<usize> {
    let key = name.trim().to_lowercase();
    let key = key.get(..3)?;
    ABILITIES
        .iter()
        .position(|(_, abbr, _, _)| abbr.eq_ignore_ascii_case(key))
}

/// `Dex +4, Con +3` to the bonuses it lists.
fn printed_saves(text: &str) -> [Option<i32>; 6] {
    let mut out = [None; 6];
    for part in text.split(',') {
        let Some((name, bonus)) = part.trim().rsplit_once(' ') else {
            continue;
        };
        let (Some(i), Ok(bonus)) = (
            ability_index(name),
            bonus.trim().trim_start_matches('+').parse::<i32>(),
        ) else {
            continue;
        };
        out[i] = Some(bonus);
    }
    out
}

/// The sum of the element's `stat` rules named `name`; `None` when it has none. A rule whose
/// value is the companion's proficiency statistic adds `proficiency`; other non-numbers add 0.
fn rule_sum(el: &Element, name: &str, proficiency: i32) -> Option<i32> {
    let values: Vec<i32> = el
        .rules
        .iter()
        .filter_map(|r| match r {
            Rule::Stat(s) if s.name.eq_ignore_ascii_case(name) => Some(s.value.trim()),
            _ => None,
        })
        .map(|v| {
            v.parse::<i32>().unwrap_or_else(|_| {
                if v.eq_ignore_ascii_case(COMPANION_PROFICIENCY) {
                    proficiency
                } else {
                    0
                }
            })
        })
        .collect();
    (!values.is_empty()).then(|| values.iter().sum())
}

/// The saving throw bonuses of an Aurora `Companion` element: the `saves` setter as printed,
/// else its `companion:<ability>:save:proficiency` (and `:misc`) statistics, which Aurora's
/// companion page adds to the ability modifier.
fn element_saves(el: &Element, proficiency: i32) -> [Option<i32>; 6] {
    let printed = printed_saves(el.setter_value("saves").unwrap_or_default());
    let mut out = [None; 6];
    for (i, (_, _, _, stat)) in ABILITIES.iter().enumerate() {
        out[i] = printed[i].or_else(|| {
            let prof = rule_sum(
                el,
                &format!("companion:{stat}:save:proficiency"),
                proficiency,
            )?;
            let misc = rule_sum(el, &format!("companion:{stat}:save:misc"), proficiency);
            let score = el
                .setter_value(stat)
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(DEFAULT_SCORE);
            Some(modifier(score) + prof + misc.unwrap_or(0))
        });
    }
    out
}

/// The lines of an Aurora `Companion` element (its setters as Aurora reads them).
#[must_use]
pub fn element_lines(el: &Element, proficiency: i32) -> StatLines {
    let text = |name: &str| el.setter_value(name).unwrap_or_default().trim().to_string();
    let languages = text("languages");
    StatLines {
        save_bonuses: element_saves(el, proficiency),
        senses: text("senses"),
        languages: if languages.is_empty() {
            NO_LANGUAGES.to_string()
        } else {
            languages
        },
        // Aurora reads the damage lines from `vulnerabilities`, `resistances` and `immunities`.
        defenses: defense_lines(&[
            ("Damage Vulnerabilities", text("vulnerabilities")),
            ("Damage Resistances", text("resistances")),
            ("Damage Immunities", text("immunities")),
            (
                "Condition Vulnerabilities",
                text("conditionVulnerabilities"),
            ),
            ("Condition Resistances", text("conditionResistances")),
            ("Condition Immunities", text("conditionImmunities")),
        ]),
    }
}

/// The six saving throws: the listed bonus, else the ability modifier.
#[must_use]
pub fn saves(lines: &StatLines, abilities: &[i32; 6]) -> Vec<CompanionSkill> {
    ABILITIES
        .iter()
        .enumerate()
        .map(|(i, (name, abbr, _, _))| CompanionSkill {
            name: (*name).to_string(),
            ability: (*abbr).to_string(),
            bonus: lines.save_bonuses[i].unwrap_or_else(|| modifier(abilities[i])),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn element(json: serde_json::Value) -> Element {
        serde_json::from_value(json).expect("element")
    }

    #[test]
    fn printed_saves_read_short_and_long_names() {
        let s = printed_saves("Dex +4, Constitution +3, Cha -1, nonsense");
        assert_eq!(s, [None, Some(4), Some(3), None, None, Some(-1)]);
    }

    #[test]
    fn element_saves_come_from_the_setter_else_the_proficiency_rules() {
        let el = element(serde_json::json!({
            "id": "ID_T_COMP", "name": "Zqbeast", "type": "Companion", "source": "Test",
            "setters": [{"name": "dexterity", "value": "15"}, {"name": "wisdom", "value": "12"},
                        {"name": "saves", "value": "Con +5"}],
            "rules": [
                {"kind": "stat", "name": "companion:dexterity:save:proficiency", "value": "companion:proficiency", "bonus": "base"},
                {"kind": "stat", "name": "companion:wisdom:save:proficiency", "value": "3"},
                {"kind": "stat", "name": "companion:wisdom:save:misc", "value": "1"}
            ]
        }));
        let lines = element_lines(&el, 2);
        assert_eq!(
            lines.save_bonuses,
            [None, Some(4), Some(5), None, Some(5), None]
        );
        let all = saves(&lines, &[3, 15, 10, 3, 12, 7]);
        let bonus: Vec<i32> = all.iter().map(|s| s.bonus).collect();
        assert_eq!(bonus, [-4, 4, 5, -4, 5, -2]);
        assert_eq!(all[1].name, "Dexterity");
        assert_eq!(all[1].ability, "DEX");
    }

    #[test]
    fn element_senses_languages_and_defenses_are_printed_as_given() {
        let el = element(serde_json::json!({
            "id": "ID_T_COMP", "name": "Zqbeast", "type": "Companion", "source": "Test",
            "setters": [{"name": "senses", "value": " darkvision 60 ft. "},
                        {"name": "resistances", "value": "fire"},
                        {"name": "damageImmunities", "value": "ignored, as in Aurora"},
                        {"name": "conditionImmunities", "value": "charmed"}]
        }));
        let lines = element_lines(&el, 2);
        assert_eq!(lines.senses, "darkvision 60 ft.");
        assert_eq!(lines.languages, NO_LANGUAGES);
        let defenses: Vec<(&str, &str)> = lines
            .defenses
            .iter()
            .map(|d| (d.name.as_str(), d.text.as_str()))
            .collect();
        assert_eq!(
            defenses,
            [
                ("Damage Resistances", "fire"),
                ("Condition Immunities", "charmed")
            ]
        );
    }
}
