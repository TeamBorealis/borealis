//! The statistic catalog of the homebrew creator (`specs/homebrew-creator-spec.md` section 5.2).
//!
//! A `stat` rule names a statistic. The statistic picker shows them in plain words, grouped. The catalog holds every
//! name the bundled packs use: the regular families (abilities, saving throws, skills, speeds, spellcasting...) are
//! generated from their parts, the rest is the curated table in `table.rs`. Class-scoped names
//! (`paladin:spellcasting:slots:1`) are patterns with `{class}` and `{n}`: the picker writes them from a class picker and
//! a number, never typed. Names loaded packs use that the catalog doesn't know are listed by [`with_found`] under
//! "Other statistics your content uses". `tests/stat_catalog.rs` diffs the catalog against the bundled packs.

mod table;

use borealis_content::rules::Rule;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;

/// One statistic of the picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatEntry {
    /// The name the `stat` rule writes; a pattern with `{class}` / `{n}` when `class_scoped`.
    pub name: String,
    /// Plain words ("Darkvision range").
    pub label: String,
    /// `name` is a pattern: written from a class picker for `{class}` and a number for `{n}`.
    pub class_scoped: bool,
}

/// A group of the picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatGroup {
    /// Stable id (`senses`).
    pub id: String,
    /// Heading ("Senses").
    pub label: String,
    /// The statistics, in picker order.
    pub stats: Vec<StatEntry>,
}

/// The id of the group [`with_found`] adds.
pub const OTHER_GROUP: &str = "yourContent";

/// The class placeholder of a class-scoped pattern.
pub const CLASS: &str = "{class}";
/// The number placeholder of a pattern (a spell slot level).
pub const NUMBER: &str = "{n}";

/// The six abilities: rule name and label.
pub(crate) const ABILITIES: [(&str, &str); 6] = [
    ("strength", "Strength"),
    ("dexterity", "Dexterity"),
    ("constitution", "Constitution"),
    ("intelligence", "Intelligence"),
    ("wisdom", "Wisdom"),
    ("charisma", "Charisma"),
];

/// Ability abbreviations of the spellcasting statistics, in [`ABILITIES`] order.
const ABBREVIATIONS: [&str; 6] = ["str", "dex", "con", "int", "wis", "cha"];

/// The skills: rule name (lower case) and label.
const SKILLS: [(&str, &str); 18] = [
    ("acrobatics", "Acrobatics"),
    ("animal handling", "Animal Handling"),
    ("arcana", "Arcana"),
    ("athletics", "Athletics"),
    ("deception", "Deception"),
    ("history", "History"),
    ("insight", "Insight"),
    ("intimidation", "Intimidation"),
    ("investigation", "Investigation"),
    ("medicine", "Medicine"),
    ("nature", "Nature"),
    ("perception", "Perception"),
    ("performance", "Performance"),
    ("persuasion", "Persuasion"),
    ("religion", "Religion"),
    ("sleight of hand", "Sleight of Hand"),
    ("stealth", "Stealth"),
    ("survival", "Survival"),
];

/// Movement modes besides walking: rule word and label.
const MODES: [(&str, &str); 4] = [
    ("fly", "flying"),
    ("swim", "swimming"),
    ("climb", "climbing"),
    ("burrow", "burrowing"),
];

fn entry(name: impl Into<String>, label: impl Into<String>) -> StatEntry {
    StatEntry {
        name: name.into(),
        label: label.into(),
        class_scoped: false,
    }
}

fn scoped(name: &str, label: &str) -> StatEntry {
    StatEntry {
        name: name.to_string(),
        label: label.to_string(),
        class_scoped: true,
    }
}

fn group(id: &str, label: &str, stats: Vec<StatEntry>) -> StatGroup {
    StatGroup {
        id: id.to_string(),
        label: label.to_string(),
        stats,
    }
}

fn abilities() -> Vec<StatEntry> {
    let mut out = Vec::new();
    for (a, l) in ABILITIES {
        out.push(entry(a, format!("{l} score")));
        out.push(entry(format!("{a}:modifier"), format!("{l} modifier")));
        out.push(entry(format!("{a}:max"), format!("{l} maximum")));
        out.push(entry(
            format!("{a}:score:set"),
            format!("{l} score becomes"),
        ));
    }
    out
}

fn saves() -> Vec<StatEntry> {
    let mut out = Vec::new();
    for (a, l) in ABILITIES {
        out.push(entry(
            format!("{a}:save:misc"),
            format!("{l} saving throw bonus"),
        ));
        out.push(entry(
            format!("{a}:save:proficiency"),
            format!("{l} saving throw proficiency"),
        ));
    }
    out
}

fn skills() -> Vec<StatEntry> {
    let mut out = Vec::new();
    for (s, l) in SKILLS {
        out.push(entry(format!("{s}:misc"), format!("{l} bonus")));
        out.push(entry(
            format!("{s}:proficiency"),
            format!("{l} proficiency"),
        ));
    }
    out
}

fn speeds() -> Vec<StatEntry> {
    let mut out = vec![
        entry("speed", "Walking speed"),
        entry("speed:misc", "Walking speed bonus"),
        entry("innate speed", "Base walking speed"),
        entry("innate speed:misc", "Base walking speed bonus"),
        entry(
            "innate speed:calculation",
            "Base walking speed (calculated)",
        ),
    ];
    for (m, l) in MODES {
        out.push(entry(format!("speed:{m}"), format!("Speed, {l}")));
        out.push(entry(
            format!("speed:{m}:misc"),
            format!("Speed bonus, {l}"),
        ));
        out.push(entry(
            format!("innate speed:{m}"),
            format!("Base speed, {l}"),
        ));
        out.push(entry(
            format!("innate speed:{m}:misc"),
            format!("Base speed bonus, {l}"),
        ));
        out.push(entry(
            format!("innate speed:{m}:calculation"),
            format!("Base speed, {l} (calculated)"),
        ));
    }
    out
}

fn spellcasting() -> Vec<StatEntry> {
    let mut out = Vec::new();
    for (key, l) in [("attack", "Spell attack bonus"), ("dc", "Spell save DC")] {
        out.push(entry(format!("spellcasting:{key}"), l));
        out.push(entry(
            format!("spellcasting:{key}:misc"),
            format!("{l}, other bonus"),
        ));
        out.push(entry(
            format!("spellcasting:{key}:proficiency"),
            format!("{l}, proficiency part"),
        ));
        for (abbr, (_, ability)) in ABBREVIATIONS.iter().zip(ABILITIES) {
            out.push(entry(
                format!("spellcasting:{key}:{abbr}"),
                format!("{l} ({ability})"),
            ));
            out.push(entry(
                format!("spellcasting:{key}:{abbr}:misc"),
                format!("{l} ({ability}), other bonus"),
            ));
        }
    }
    out.push(entry("spellcasting:dc:base", "Spell save DC, base"));
    out.extend([
        scoped("{class}:spellcasting:slots:{n}", "Spell slots of level N"),
        scoped("{class}:spellcasting:prepare", "Spells prepared"),
        scoped("{class}:spellcasting:cantrips known", "Cantrips known"),
        scoped("{class}:spellcasting:spells known", "Spells known"),
        scoped("{class}:spellcasting:slot", "Pact Magic slot level"),
        scoped("{class}:spellcasting:slots:count", "Pact Magic slots"),
        scoped(
            "spell save dc:{class}",
            "Spell save DC of the class's spells",
        ),
        entry(
            "multiclass:spellcasting:level",
            "Multiclass spellcaster level",
        ),
        scoped(
            "multiclass:spellcasting:slot:{n}",
            "Multiclass spell slots of level N",
        ),
    ]);
    out
}

fn levels() -> Vec<StatEntry> {
    vec![
        entry("level", "Character level"),
        scoped("level:{class}", "Class level"),
        scoped("level:{class}:half", "Half the class level"),
        entry("proficiency", "Proficiency bonus"),
        entry(
            "proficiency:half",
            "Half the proficiency bonus (rounded down)",
        ),
        entry(
            "proficiency:half:up",
            "Half the proficiency bonus (rounded up)",
        ),
    ]
}

fn companion() -> Vec<StatEntry> {
    ABILITIES
        .iter()
        .map(|(a, l)| entry(format!("companion:{a}"), format!("Companion's {l} score")))
        .collect()
}

/// The catalog, in picker order.
#[must_use]
pub fn catalog() -> &'static [StatGroup] {
    static ALL: std::sync::OnceLock<Vec<StatGroup>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let curated = |g: &str| {
            table::CURATED
                .iter()
                .filter(|(_, _, group)| *group == g)
                .map(|(n, l, _)| entry(*n, *l))
                .collect::<Vec<_>>()
        };
        let with = |mut v: Vec<StatEntry>, g: &str| {
            v.extend(curated(g));
            v
        };
        vec![
            group("abilities", "Abilities", abilities()),
            group("savingThrows", "Saving throws", saves()),
            group("skills", "Skills", skills()),
            group("armorClass", "Armor Class", curated("armorClass")),
            group("hitPoints", "Hit points", curated("hitPoints")),
            group("speed", "Speed", speeds()),
            group("senses", "Senses", curated("senses")),
            group("spellcasting", "Spellcasting", spellcasting()),
            group("attacks", "Attacks", curated("attacks")),
            group("levels", "Level and proficiency", with(levels(), "levels")),
            group("classCounters", "Class counters", curated("classCounters")),
            group("companion", "Companion", companion()),
            group("other", "Other", curated("other")),
        ]
    })
}

/// A catalog entry matched by a concrete name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatMatch {
    /// The group id.
    pub group: &'static str,
    /// The entry (a pattern for class-scoped names).
    pub entry: &'static StatEntry,
    /// The class of a class-scoped name, as written.
    pub class: Option<String>,
    /// The number of a `{n}` pattern.
    pub number: Option<u32>,
}

/// Does `name` match `pattern` (`{class}` = one or more words without `:`, `{n}` = a number 1-9)?
fn matches(pattern: &str, name: &str) -> Option<(Option<String>, Option<u32>)> {
    let p: Vec<&str> = pattern.split(':').collect();
    let n: Vec<&str> = name.split(':').collect();
    if p.len() != n.len() {
        return None;
    }
    let mut class = None;
    let mut number = None;
    for (pp, nn) in p.iter().zip(&n) {
        if let Some(prefix) = pp.strip_suffix(CLASS) {
            let c = nn.strip_prefix(prefix)?;
            if c.is_empty() || c.contains(['{', '}']) {
                return None;
            }
            class = Some(c.to_string());
        } else if *pp == NUMBER {
            let v: u32 = nn.parse().ok()?;
            if !(1..=9).contains(&v) {
                return None;
            }
            number = Some(v);
        } else if pp != nn {
            return None;
        }
    }
    Some((class, number))
}

/// The catalog entry of a statistic name (any case; a leading `-` is ignored).
#[must_use]
pub fn find(name: &str) -> Option<StatMatch> {
    let name = name.trim().trim_start_matches('-');
    if name.chars().any(char::is_uppercase) {
        return None;
    }
    let groups = catalog();
    for g in groups {
        if let Some(e) = g.stats.iter().find(|e| !e.class_scoped && e.name == name) {
            return Some(StatMatch {
                group: &g.id,
                entry: e,
                class: None,
                number: None,
            });
        }
    }
    groups.iter().find_map(|g| {
        g.stats.iter().filter(|e| e.class_scoped).find_map(|e| {
            let (class, number) = matches(&e.name, name)?;
            // `multiclass:spellcasting:level` and friends are their own entries, never a class called "multiclass".
            (class.as_deref() != Some("multiclass")).then(|| StatMatch {
                group: &g.id,
                entry: e,
                class,
                number,
            })
        })
    })
}

fn title(words: &str) -> String {
    words
        .split(' ')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Plain words for a statistic name: the catalog label (class-scoped ones name the class and the slot level), else
/// the name's own words (`breath-weapon:dc` -> "Breath weapon dc").
#[must_use]
pub fn label(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    let negated = lower.starts_with('-');
    let base = match find(&lower) {
        Some(m) => {
            let mut l = m.entry.label.clone();
            if let Some(n) = m.number {
                l = l.replace(" N", &format!(" {n}"));
            }
            match m.class {
                Some(c) => format!("{l} ({})", title(&c)),
                None => l,
            }
        }
        None => humanize(lower.trim_start_matches('-')),
    };
    if negated {
        format!("minus {base}")
    } else {
        base
    }
}

/// The words of an unknown statistic name: `:`, `-` and `_` become spaces, first letter upper case.
fn humanize(name: &str) -> String {
    let words: Vec<&str> = name
        .split([':', '-', '_', ' '])
        .filter(|w| !w.is_empty())
        .collect();
    let text = words.join(" ");
    let mut c = text.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// The catalog plus a group "Other statistics your content uses" with the names in `found` it doesn't know (sorted,
/// each once; empty and upper-case names are left out, since rule names are lower case).
#[must_use]
pub fn with_found<'a>(found: impl IntoIterator<Item = &'a str>) -> Vec<StatGroup> {
    let others: BTreeSet<String> = found
        .into_iter()
        .map(|n| n.trim().trim_start_matches('-').to_string())
        .filter(|n| !n.is_empty() && find(n).is_none() && !n.chars().any(char::is_uppercase))
        .filter(|n| n.parse::<i64>().is_err())
        .collect();
    let mut out = catalog().to_vec();
    if !others.is_empty() {
        out.push(group(
            OTHER_GROUP,
            "Other statistics your content uses",
            others
                .iter()
                .map(|n| entry(n.clone(), humanize(n)))
                .collect(),
        ));
    }
    out
}

/// The statistic names `rules` use (lower case, each once): `stat` names, statistic values and maximums (also inside
/// toggle effects), `resource` maxima of type `stat`, `extra_damage.dice_count.stat`,
/// `damage_bonus_spell.damage_type_stat`. Numbers are left out.
#[must_use]
pub fn used_names<'a>(rules: impl IntoIterator<Item = &'a Rule>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut add = |n: &str| {
        let n = n.trim().trim_start_matches('-').to_lowercase();
        if !n.is_empty() && n.parse::<i64>().is_err() {
            out.insert(n);
        }
    };
    for r in rules {
        match r {
            Rule::Stat(s) => {
                add(&s.name);
                if s.inline != Some(true) {
                    add(&s.value);
                }
                if let Some(m) = &s.maximum {
                    add(m);
                }
            }
            Rule::Custom(c) => {
                let f = |k: &str| c.fields.get(k);
                for e in f("effects").and_then(Value::as_array).into_iter().flatten() {
                    if e["kind"] == "stat" {
                        e["name"].as_str().into_iter().for_each(&mut add);
                        e["value"].as_str().into_iter().for_each(&mut add);
                    }
                }
                for (key, ty) in [("max", "stat"), ("dice_count", "stat")] {
                    if let Some(v) = f(key).filter(|v| v["type"] == ty) {
                        v["stat"].as_str().into_iter().for_each(&mut add);
                    }
                }
                f("damage_type_stat")
                    .and_then(Value::as_str)
                    .into_iter()
                    .for_each(&mut add);
            }
            Rule::Grant(_) | Rule::Select(_) | Rule::Unknown(_) => {}
        }
    }
    out
}

#[cfg(test)]
mod tests;
