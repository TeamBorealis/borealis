//! The numbers check: every number an element's mechanics state (dice, DCs, ranges, durations,
//! uses, bonuses) is printed in its source passage, as digits, a die ("d8" = "1d8") or a
//! number word ("thirty", "twenty-five"). A number the passage does not print may still come
//! from the rules library: the same-named library element (a weapon's stats when the source
//! only prices it), a library element the element points at, or a spell its passage names.

use std::collections::HashSet;

use borealis_content::ContentStore;
use serde_json::Value;

use super::{dice, failed, is_row, leading_number, norm, passed, span, tokens, Checker};
use super::{ElementCheck, ImportDraft, PlannedElement, Role, NUMBER_NOT_IN_SOURCE};

/// Smallest number checked: 0 and 1 are printed as "a", "an", "once" or not at all.
const MIN_CHECKED_NUMBER: u64 = 2;
/// Fewest letters of a spell name looked for in a passage.
const MIN_SPELL_NAME: usize = 4;

/// Rule fields whose numbers the source prints.
const RULE_NUMBER_KEYS: [&str; 20] = [
    "value",
    "range",
    "dice",
    "uses",
    "cost",
    "amount",
    "plus",
    "speed",
    "duration",
    "number",
    "bonus",
    "dc",
    "distance",
    "radius",
    "damage",
    "die",
    "max",
    "pick",
    "count",
    "dice_count",
];
/// Rule fields never checked: levels and spell levels come from the plan or the tables, and
/// an ability score cap is the rules' own.
const RULE_SKIP_KEYS: [&str; 11] = [
    "level",
    "levels",
    "cast_level",
    "slots",
    "id",
    "kind",
    "formula",
    "requirements",
    "spell",
    "replaces",
    "cap",
];
/// `data` fields whose numbers the source prints (an item's table row, a weapon's cells).
const DATA_NUMBER_KEYS: [&str; 10] = [
    "dice",
    "versatile_damage",
    "range",
    "base_ac",
    "strength_requirement",
    "speed",
    "value",
    "weight",
    "amount",
    "bonus",
];
/// `data` fields never checked (class progression and levels).
const DATA_SKIP_KEYS: [&str; 6] = [
    "progression",
    "asi_levels",
    "level",
    "level_gained",
    "subclass_level",
    "hit_die",
];
/// Setters whose values the source prints.
const SETTER_NUMBER_NAMES: [&str; 10] = [
    "range",
    "duration",
    "damage",
    "versatile",
    "enhancement",
    "armorClass",
    "speed",
    "weight",
    "cost",
    "time",
];
/// Number words a passage may print instead of digits.
const NUMBER_WORDS: [(&str, u64); 31] = [
    ("two", 2),
    ("three", 3),
    ("four", 4),
    ("five", 5),
    ("six", 6),
    ("seven", 7),
    ("eight", 8),
    ("nine", 9),
    ("ten", 10),
    ("eleven", 11),
    ("twelve", 12),
    ("thirteen", 13),
    ("fourteen", 14),
    ("fifteen", 15),
    ("sixteen", 16),
    ("seventeen", 17),
    ("eighteen", 18),
    ("nineteen", 19),
    ("twenty", 20),
    ("thirty", 30),
    ("forty", 40),
    ("fifty", 50),
    ("sixty", 60),
    ("seventy", 70),
    ("eighty", 80),
    ("ninety", 90),
    ("hundred", 100),
    ("twice", 2),
    ("thrice", 3),
    ("double", 2),
    ("triple", 3),
];

/// One number an element's mechanics state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Claimed {
    Number(u64),
    Dice((u32, u32)),
}

impl std::fmt::Display for Claimed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Number(n) => write!(f, "{n}"),
            Self::Dice((c, s)) => write!(f, "{c}d{s}"),
        }
    }
}

/// The numbers and dice a text prints, number words included.
#[derive(Debug, Default)]
struct Printed {
    numbers: HashSet<u64>,
    dice: HashSet<(u32, u32)>,
}

impl Printed {
    fn add(&mut self, text: &str) {
        let toks = tokens(text);
        let word = |t: &str| NUMBER_WORDS.iter().find(|(w, _)| *w == t).map(|(_, n)| *n);
        for (i, t) in toks.iter().enumerate() {
            if let Some(d) = dice(t) {
                self.dice.insert(d);
                self.numbers.extend([u64::from(d.0), u64::from(d.1)]);
                continue;
            }
            let Some(n) = leading_number(t).or_else(|| word(t)) else {
                continue;
            };
            self.numbers.insert(n);
            match toks.get(i + 1).and_then(|n| word(n)) {
                Some(100) => {
                    self.numbers.insert(n * 100);
                }
                Some(u) if (20..100).contains(&n) && n % 10 == 0 && u < 10 => {
                    self.numbers.insert(n + u);
                }
                _ => {}
            }
        }
    }

    fn has(&self, claim: Claimed) -> bool {
        match claim {
            Claimed::Number(n) => n < MIN_CHECKED_NUMBER || self.numbers.contains(&n),
            Claimed::Dice(d) => self.dice.contains(&d),
        }
    }
}

fn claimed_in(text: &str) -> Vec<Claimed> {
    tokens(text)
        .iter()
        .filter_map(|t| {
            dice(t)
                .map(Claimed::Dice)
                .or_else(|| leading_number(t).map(Claimed::Number))
        })
        .collect()
}

struct Fields<'a> {
    keys: &'a [&'a str],
    skip: &'a [&'a str],
}

fn walk(v: &Value, key: &str, label: &str, f: &Fields, out: &mut Vec<(String, Claimed)>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m.iter().filter(|(k, _)| !f.skip.contains(&k.as_str())) {
                let label = if k == "value" { label } else { k.as_str() };
                walk(x, k, label, f, out);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| walk(x, key, label, f, out)),
        Value::Number(n) if f.keys.contains(&key) => {
            if let Some(n) = n.as_u64() {
                out.push((label.replace('_', " "), Claimed::Number(n)));
            }
        }
        Value::String(s) if f.keys.contains(&key) => {
            let label = label.replace('_', " ");
            out.extend(claimed_in(s).into_iter().map(|c| (label.clone(), c)));
        }
        _ => {}
    }
}

/// Field names as the user reads them.
const LABELS: [(&str, &str); 6] = [
    ("armorclass", "AC"),
    ("base ac", "AC"),
    ("dc", "DC"),
    ("strength requirement", "Strength requirement"),
    ("max", "number of uses"),
    ("uses", "number of uses"),
];

fn label(field: &str) -> String {
    LABELS
        .iter()
        .find(|(k, _)| *k == field)
        .map_or_else(|| field.to_string(), |(_, l)| (*l).to_string())
}

/// The numbers an element's mechanics state, each with the field it is in (item fields and
/// setters first: they name the field best).
fn claims(el: &Value) -> Vec<(String, Claimed)> {
    let mut out = Vec::new();
    if let Some(data) = el.get("data") {
        let f = Fields {
            keys: &DATA_NUMBER_KEYS,
            skip: &DATA_SKIP_KEYS,
        };
        walk(data, "data", "value", &f, &mut out);
        for (k, v) in data
            .get("columns")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            let text = v.as_str().map_or_else(|| v.to_string(), str::to_string);
            out.extend(claimed_in(&text).into_iter().map(|c| (k.to_lowercase(), c)));
        }
    }
    for s in el
        .get("setters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = s.get("name").and_then(Value::as_str) else {
            continue;
        };
        let text = match s.get("value") {
            Some(Value::String(v)) => v.clone(),
            Some(Value::Number(n)) => n.to_string(),
            _ => continue,
        };
        if SETTER_NUMBER_NAMES.contains(&name) {
            out.extend(
                claimed_in(&text)
                    .into_iter()
                    .map(|c| (name.to_lowercase(), c)),
            );
        }
    }
    if let Some(rules) = el.get("rules") {
        let f = Fields {
            keys: &RULE_NUMBER_KEYS,
            skip: &RULE_SKIP_KEYS,
        };
        walk(rules, "rules", "value", &f, &mut out);
    }
    out.into_iter().map(|(l, c)| (label(&l), c)).collect()
}

/// The text an element's numbers are checked against: its located section, its name and the
/// table rows of its owner's passage (a feature's scaling column); `None` when unlocated.
fn passage(draft: &ImportDraft, p: &PlannedElement, lines: &[String]) -> Option<String> {
    let mut text = super::super::source::section_text(&draft.source_text, p)?;
    text.push('\n');
    text.push_str(&p.name);
    let owner = p
        .owner
        .as_deref()
        .and_then(|o| draft.plan.elements.iter().find(|x| x.id == o));
    if let Some((s, e)) = owner.and_then(span) {
        for l in lines.iter().take(e).skip(s).filter(|l| is_row(l)) {
            text.push('\n');
            text.push_str(l);
        }
    }
    Some(text)
}

/// Library elements an element's numbers may come from: the same-named element of its type
/// (spaces ignored: "Short Sword" is "Shortsword"),
/// the library elements it points at, and the spells its passage names.
fn library_text(
    store: &ContentStore,
    spells: &[(String, String)],
    el: &Value,
    ty: &str,
    name: &str,
    passage: &str,
) -> String {
    let mut ids: Vec<String> = Vec::new();
    let joined = |n: &str| norm(n).replace(' ', "");
    let key = joined(name);
    ids.extend(
        store
            .by_type(ty)
            .filter(|s| joined(&s.element.name) == key)
            .map(|s| s.element.id.clone()),
    );
    collect_ids(el, &mut ids);
    let text = format!(" {} ", norm(passage));
    ids.extend(
        spells
            .iter()
            .filter(|(n, _)| text.contains(&format!(" {n} ")))
            .map(|(_, id)| id.clone()),
    );
    ids.iter()
        .filter_map(|id| store.element(id))
        .filter_map(|e| serde_json::to_string(e).ok())
        .collect::<Vec<_>>()
        .join("\n")
}

fn collect_ids(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) if s.starts_with("ID_") => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|x| collect_ids(x, out)),
        Value::Object(m) => m.values().for_each(|x| collect_ids(x, out)),
        _ => {}
    }
}

/// The library's spell names (normalised, long enough to look for) with their ids.
pub(super) fn spell_names(store: &ContentStore) -> Vec<(String, String)> {
    store
        .by_type("Spell")
        .map(|s| (norm(&s.element.name), s.element.id.clone()))
        .filter(|(n, _)| n.len() >= MIN_SPELL_NAME)
        .collect()
}

/// The numbers check of `json` as the planned element `p`.
pub(super) fn number_checks(
    c: &Checker,
    draft: &ImportDraft,
    p: &PlannedElement,
    json: &Value,
) -> Vec<ElementCheck> {
    let all = claims(json);
    if all.is_empty() || p.role == Role::Text {
        return Vec::new();
    }
    let Some(text) = passage(draft, p, &c.lines) else {
        return Vec::new();
    };
    let mut printed = Printed::default();
    printed.add(&text);
    let mut seen = HashSet::new();
    let mut missing: Vec<(String, Claimed)> = all
        .into_iter()
        .filter(|(_, n)| !printed.has(*n) && seen.insert(*n))
        .collect();
    if !missing.is_empty() {
        printed.add(&library_text(
            &c.library, &c.spells, json, &p.ty, &p.name, &text,
        ));
        missing.retain(|(_, n)| !printed.has(*n));
    }
    if missing.is_empty() {
        return vec![passed(
            NUMBER_NOT_IN_SOURCE,
            "Every number is in the source text",
        )];
    }
    missing
        .into_iter()
        .map(|(label, n)| {
            failed(
                NUMBER_NOT_IN_SOURCE,
                format!("The {label} {n} is not in the source text"),
            )
        })
        .collect()
}
