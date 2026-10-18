//! Raw 5etools origin and feat entries for the source-truth audit (`source_truth.rs`): the
//! entries of `race`, `subrace`, `background` and `feat` (resolved for `_copy` / `_versions` as
//! the importer does), matched to the elements they became, and what their fields say a
//! character gets. The reading here is written from the 5etools data format, not from the
//! importer's converters, so the audit compares two independent readings.
#![allow(dead_code)]

use borealis_content::{Element, Ruleset};
use borealis_import_5etools::util::id_fragment;
use borealis_import_5etools::{copy, folder};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

/// Ability abbreviations, score order.
pub const ABILITIES: [&str; 6] = ["str", "dex", "con", "int", "wis", "cha"];
/// Ability names, score order (as the engine names them).
pub const ABILITY_NAMES: [&str; 6] = [
    "Strength",
    "Dexterity",
    "Constitution",
    "Intelligence",
    "Wisdom",
    "Charisma",
];
/// Tool categories a `toolProficiencies` key can name instead of one tool.
const TOOL_CATEGORIES: [&str; 3] = ["artisan's tools", "gaming set", "musical instrument"];
/// Speed modes, in [`Speeds`] order.
pub const SPEED_MODES: [&str; 5] = ["walk", "fly", "climb", "swim", "burrow"];

/// An ability allocation: (ability index, increase), sorted by ability.
pub type Alloc = Vec<(usize, i32)>;
/// Every allocation a choice can produce.
pub type AllocSet = BTreeSet<Alloc>;

/// What a proficiency choice offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Skill,
    Tool,
    Language,
}

/// A choice of `count` proficiencies, from a named list (`None`: any of the kind).
#[derive(Clone, Debug)]
pub struct ProfChoice {
    pub kind: Kind,
    pub count: usize,
    pub from: Option<BTreeSet<String>>,
}

/// How a granted spell can be cast.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Uses {
    /// Known / prepared (cast as usual).
    Plain,
    /// At will.
    Will,
    /// As a ritual only.
    Ritual,
    /// `n` times per long rest.
    Daily(i32),
    /// `n` times per short or long rest.
    Rest(i32),
    /// Proficiency-bonus times per long rest (`daily: {pb: [...]}`).
    DailyPb,
}

/// A granted spell, or a choice of `n` spells (a `choose` filter).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpellRef {
    Named(String),
    Choose(usize),
}

/// One spell grant from `known` / `innate` / `prepared`, at a character level.
#[derive(Clone, Debug)]
pub struct Grant {
    pub level: u32,
    pub spell: SpellRef,
    pub uses: Uses,
}

/// One `additionalSpells` group (alternatives are separate groups, named when the source
/// names them).
#[derive(Clone, Debug, Default)]
pub struct SpellGroup {
    pub name: Option<String>,
    pub grants: Vec<Grant>,
}

/// What an entry's fields say a character gets.
#[derive(Clone, Debug, Default)]
pub struct Expect {
    pub fixed: [i32; 6],
    pub ability_choices: Vec<AllocSet>,
    pub skills: BTreeSet<String>,
    pub tools: BTreeSet<String>,
    pub languages: BTreeSet<String>,
    pub choices: Vec<ProfChoice>,
    /// walk, fly, climb, swim, burrow (`None`: not given).
    pub speed: Option<[Option<i32>; 5]>,
    pub resist: BTreeSet<String>,
    pub immune: BTreeSet<String>,
    pub condition_immune: BTreeSet<String>,
    /// (category, damage types offered, how many to pick)
    pub defence_choices: Vec<(&'static str, BTreeSet<String>, usize)>,
    /// `additionalSpells` of each source entry (a subrace on its race has two sets); within a
    /// set, one group applies directly and several are alternatives.
    pub spell_sets: Vec<Vec<SpellGroup>>,
    /// Proficiency fields given as alternative arrays ("one of these"): the named items of
    /// each alternative.
    pub alternatives: Vec<(Kind, Vec<BTreeSet<String>>)>,
    /// Field shapes this audit does not read (checked against a closed list).
    pub unread: Vec<String>,
}

fn count_of(v: &Value) -> usize {
    v.as_u64().map_or(1, |n| usize::try_from(n).unwrap_or(1))
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(|s| s.to_lowercase())
                .collect()
        })
        .unwrap_or_default()
}

/// A spell reference without book, cast suffix or case (`dancing lights|xphb#c`).
pub fn spell_name(s: &str) -> String {
    let base = s.split('|').next().unwrap_or(s);
    base.split('#').next().unwrap_or(base).trim().to_lowercase()
}

fn ability_index(abbr: &str) -> Option<usize> {
    ABILITIES.iter().position(|a| *a == abbr)
}

fn sorted(mut a: Alloc) -> Alloc {
    a.sort_unstable();
    a
}

/// `count` different abilities of `pool`, each `+amount`.
pub fn choose_set(pool: &[usize], count: usize, amount: i32) -> AllocSet {
    fn rec(
        pool: &[usize],
        count: usize,
        amount: i32,
        start: usize,
        cur: &mut Alloc,
        out: &mut AllocSet,
    ) {
        if cur.len() == count {
            out.insert(sorted(cur.clone()));
            return;
        }
        for i in start..pool.len() {
            cur.push((pool[i], amount));
            rec(pool, count, amount, i + 1, cur, out);
            cur.pop();
        }
    }
    let mut out = AllocSet::new();
    rec(
        pool,
        count.min(pool.len()),
        amount,
        0,
        &mut Vec::new(),
        &mut out,
    );
    out
}

/// The weights assigned to different abilities of `pool` (2024 backgrounds: `[2, 1]`).
pub fn weighted_set(pool: &[usize], weights: &[i32]) -> AllocSet {
    fn rec(
        pool: &[usize],
        weights: &[i32],
        used: &mut Vec<usize>,
        cur: &mut Alloc,
        out: &mut AllocSet,
    ) {
        let Some((w, rest)) = weights.split_first() else {
            out.insert(sorted(cur.clone()));
            return;
        };
        for &a in pool {
            if used.contains(&a) {
                continue;
            }
            used.push(a);
            cur.push((a, *w));
            rec(pool, rest, used, cur, out);
            cur.pop();
            used.pop();
        }
    }
    let mut out = AllocSet::new();
    rec(pool, weights, &mut Vec::new(), &mut Vec::new(), &mut out);
    out
}

fn pool_of(choose: &Value) -> Vec<usize> {
    let from = strings(choose.get("from"));
    if from.is_empty() {
        (0..6).collect()
    } else {
        from.iter().filter_map(|a| ability_index(a)).collect()
    }
}

/// The allocations one `choose` object offers. A hidden full-pool `+2 to one` is the 2024
/// Ability Score Improvement whose text reads "+2 to one or +1 to two".
fn choose_allocs(choose: &Value, hidden: bool) -> AllocSet {
    let pool = pool_of(choose);
    let count = count_of(choose.get("count").unwrap_or(&Value::from(1)));
    let amount = choose.get("amount").and_then(Value::as_i64).unwrap_or(1) as i32;
    let mut set = choose_set(&pool, count, amount);
    if hidden && pool.len() == 6 && amount == 2 && count == 1 {
        set.extend(choose_set(&pool, 2, 1));
    }
    set
}

/// Every allocation of one `ability` object (its fixed part plus each choose).
fn expand(obj: &Map<String, Value>) -> AllocSet {
    let hidden = obj.get("hidden") == Some(&Value::Bool(true));
    let mut fixed: Alloc = Vec::new();
    let mut chooses = Vec::new();
    for (k, v) in obj {
        match (k.as_str(), ability_index(k)) {
            ("choose", _) => chooses.push(choose_allocs(v, hidden)),
            (_, Some(i)) => fixed.push((i, v.as_i64().unwrap_or(0) as i32)),
            _ => {}
        }
    }
    let mut out: AllocSet = [sorted(fixed)].into_iter().collect();
    for c in chooses {
        let mut next = AllocSet::new();
        for a in &out {
            for b in &c {
                let mut m: BTreeMap<usize, i32> = a.iter().copied().collect();
                for (i, n) in b {
                    *m.entry(*i).or_default() += n;
                }
                next.insert(m.into_iter().collect());
            }
        }
        out = next;
    }
    out
}

impl Expect {
    /// Read the fields of one raw entry.
    pub fn read(raw: &Map<String, Value>) -> Self {
        let mut e = Self::default();
        e.ability(raw);
        e.proficiencies(raw);
        e.speed(raw.get("speed"));
        e.defences(raw);
        e.spells(raw.get("additionalSpells"));
        e
    }

    fn ability(&mut self, raw: &Map<String, Value>) {
        let list: Vec<&Map<String, Value>> = raw
            .get("ability")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_object).collect())
            .unwrap_or_default();
        if list.is_empty() {
            match raw.get("lineage") {
                // VRGR custom lineage rule (MPMM races): +2 / +1 or +1 / +1 / +1, any scores
                Some(Value::String(s)) if s == "VRGR" => {
                    let all: Vec<usize> = (0..6).collect();
                    let mut set = weighted_set(&all, &[2, 1]);
                    set.extend(weighted_set(&all, &[1, 1, 1]));
                    self.ability_choices.push(set);
                }
                Some(Value::Null) | None => {}
                Some(other) => self.unread.push(format!("lineage {other}")),
            }
            return;
        }
        let weighted: Vec<&Value> = list
            .iter()
            .filter_map(|o| o.get("choose").and_then(|c| c.get("weighted")))
            .collect();
        if !weighted.is_empty() {
            let mut set = AllocSet::new();
            for w in weighted {
                let pool = pool_of(w);
                let weights: Vec<i32> = w
                    .get("weights")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_i64)
                            .map(|n| n as i32)
                            .collect()
                    })
                    .unwrap_or_default();
                set.extend(weighted_set(&pool, &weights));
            }
            self.ability_choices.push(set);
            return;
        }
        let visible: Vec<&Map<String, Value>> = list
            .iter()
            .copied()
            .filter(|o| o.get("hidden") != Some(&Value::Bool(true)))
            .collect();
        let alternatives = if visible.is_empty() {
            list.clone()
        } else {
            visible
        };
        if alternatives.len() > 1 {
            // "one of these arrays": a single choice over all of them
            let mut set = AllocSet::new();
            for a in alternatives {
                set.extend(expand(a));
            }
            self.ability_choices.push(set);
            return;
        }
        let obj = alternatives[0];
        let hidden = obj.get("hidden") == Some(&Value::Bool(true));
        for (k, v) in obj {
            match (k.as_str(), ability_index(k)) {
                ("choose", _) => self.ability_choices.push(choose_allocs(v, hidden)),
                (_, Some(i)) => self.fixed[i] += v.as_i64().unwrap_or(0) as i32,
                ("hidden" | "max", _) => {}
                (other, _) => self.unread.push(format!("ability key {other}")),
            }
        }
    }

    /// The single object of a proficiency field; alternative arrays are kept apart.
    fn first<'a>(
        &mut self,
        raw: &'a Map<String, Value>,
        field: &str,
        kind: Kind,
    ) -> Option<&'a Map<String, Value>> {
        let list = raw.get(field)?.as_array()?;
        if list.len() > 1 {
            let alts = list
                .iter()
                .filter_map(Value::as_object)
                .map(|o| {
                    o.iter()
                        .filter(|(_, v)| **v == Value::Bool(true))
                        .map(|(k, _)| k.to_lowercase())
                        .collect()
                })
                .collect();
            self.alternatives.push((kind, alts));
            return None;
        }
        list.first()?.as_object()
    }

    fn proficiencies(&mut self, raw: &Map<String, Value>) {
        if let Some(o) = self.first(raw, "skillProficiencies", Kind::Skill) {
            for (k, v) in o {
                match k.as_str() {
                    "any" => self.choice(Kind::Skill, count_of(v), None),
                    "choose" => self.choice(
                        Kind::Skill,
                        count_of(v.get("count").unwrap_or(&Value::from(1))),
                        Some(strings(v.get("from")).into_iter().collect()),
                    ),
                    name if v == &Value::Bool(true) => {
                        self.skills.insert(name.to_lowercase());
                    }
                    other => self.unread.push(format!("skillProficiencies {other}")),
                }
            }
        }
        if let Some(o) = self.first(raw, "toolProficiencies", Kind::Tool) {
            for (k, v) in o {
                match k.as_str() {
                    "any"
                    | "anyTool"
                    | "anyArtisansTool"
                    | "anyGamingSet"
                    | "anyMusicalInstrument" => {
                        self.choice(Kind::Tool, count_of(v), None);
                    }
                    "choose" => {
                        let from: BTreeSet<String> = strings(v.get("from")).into_iter().collect();
                        let named = !from
                            .iter()
                            .any(|f| TOOL_CATEGORIES.contains(&f.as_str()) || f.starts_with("any"));
                        let count = count_of(v.get("count").unwrap_or(&Value::from(1)));
                        self.choice(Kind::Tool, count, named.then_some(from));
                    }
                    name if TOOL_CATEGORIES.contains(&name.to_lowercase().as_str()) => {
                        self.choice(Kind::Tool, 1, None);
                    }
                    name if v == &Value::Bool(true) => {
                        self.tools.insert(name.to_lowercase());
                    }
                    other => self.unread.push(format!("toolProficiencies {other}")),
                }
            }
        }
        if let Some(o) = self.first(raw, "languageProficiencies", Kind::Language) {
            for (k, v) in o {
                match k.as_str() {
                    "other" => self.choice(Kind::Language, 1, None),
                    "choose" => {
                        let from: BTreeSet<String> = strings(v.get("from")).into_iter().collect();
                        let named = !from.iter().any(|f| f.starts_with("any") || f == "other");
                        let count = count_of(v.get("count").unwrap_or(&Value::from(1)));
                        self.choice(Kind::Language, count, named.then_some(from));
                    }
                    any if any.starts_with("any") => self.choice(Kind::Language, count_of(v), None),
                    name if v == &Value::Bool(true) => {
                        self.languages.insert(name.to_lowercase());
                    }
                    other => self.unread.push(format!("languageProficiencies {other}")),
                }
            }
        }
    }

    fn choice(&mut self, kind: Kind, count: usize, from: Option<BTreeSet<String>>) {
        self.choices.push(ProfChoice { kind, count, from });
    }

    fn speed(&mut self, v: Option<&Value>) {
        let Some(v) = v else { return };
        let mut out = [None; 5];
        match v {
            Value::Number(n) => out[0] = n.as_i64().map(|n| n as i32),
            Value::Object(m) => {
                let walk = m.get("walk").and_then(Value::as_i64).map(|n| n as i32);
                for (i, mode) in SPEED_MODES.iter().enumerate() {
                    out[i] = match m.get(*mode) {
                        None => None,
                        Some(Value::Number(n)) => n.as_i64().map(|n| n as i32),
                        Some(Value::Bool(true)) => walk,
                        Some(Value::Object(o)) => {
                            o.get("number").and_then(Value::as_i64).map(|n| n as i32)
                        }
                        Some(other) => {
                            self.unread.push(format!("speed {mode} {other}"));
                            None
                        }
                    };
                }
            }
            Value::Null => return,
            other => self.unread.push(format!("speed {other}")),
        }
        self.speed = Some(out);
    }

    fn defences(&mut self, raw: &Map<String, Value>) {
        for (field, cat) in [
            ("resist", "resistance"),
            ("immune", "immunity"),
            ("conditionImmune", "condition immunity"),
        ] {
            let Some(list) = raw.get(field).and_then(Value::as_array) else {
                continue;
            };
            for item in list {
                match item {
                    Value::String(s) => {
                        let set = match field {
                            "resist" => &mut self.resist,
                            "immune" => &mut self.immune,
                            _ => &mut self.condition_immune,
                        };
                        set.insert(s.to_lowercase());
                    }
                    Value::Object(o) if o.contains_key("choose") => {
                        let from = strings(o["choose"].get("from")).into_iter().collect();
                        let count = count_of(o["choose"].get("count").unwrap_or(&Value::from(1)));
                        self.defence_choices.push((cat, from, count));
                    }
                    other => self.unread.push(format!("{field} {}", shape(other))),
                }
            }
        }
    }

    fn spells(&mut self, v: Option<&Value>) {
        let Some(groups) = v.and_then(Value::as_array) else {
            return;
        };
        let mut set = Vec::new();
        for g in groups {
            let Some(g) = g.as_object() else { continue };
            let mut group = SpellGroup {
                name: g.get("name").and_then(Value::as_str).map(str::to_string),
                grants: Vec::new(),
            };
            for (mode, default) in [
                ("known", Uses::Plain),
                ("innate", Uses::Will),
                ("prepared", Uses::Plain),
            ] {
                let Some(m) = g.get(mode).and_then(Value::as_object) else {
                    continue;
                };
                for (lk, v) in m {
                    let level = if lk == "_" {
                        1
                    } else if let Ok(n) = lk.parse::<u32>() {
                        n
                    } else {
                        self.unread.push(format!(
                            "additionalSpells {mode} key {}",
                            lk.trim_end_matches(char::is_numeric)
                        ));
                        continue;
                    };
                    self.grants_of(v, level, default, mode, &mut group.grants);
                }
            }
            set.push(group);
        }
        if !set.is_empty() {
            self.spell_sets.push(set);
        }
    }

    fn grants_of(&mut self, v: &Value, level: u32, uses: Uses, mode: &str, out: &mut Vec<Grant>) {
        match v {
            Value::Array(items) => {
                for it in items {
                    let spell = match it {
                        Value::String(s) => SpellRef::Named(spell_name(s)),
                        Value::Object(o) if o.contains_key("choose") => {
                            let count = o.get("count").or_else(|| o["choose"].get("count"));
                            SpellRef::Choose(count_of(count.unwrap_or(&Value::from(1))))
                        }
                        other => {
                            self.unread
                                .push(format!("additionalSpells {mode} item {}", shape(other)));
                            continue;
                        }
                    };
                    out.push(Grant { level, spell, uses });
                }
            }
            Value::Object(m) => {
                for (k, vv) in m {
                    match k.as_str() {
                        "_" => self.grants_of(vv, level, uses, mode, out),
                        "will" => self.grants_of(vv, level, Uses::Will, mode, out),
                        "ritual" => self.grants_of(vv, level, Uses::Ritual, mode, out),
                        "daily" | "rest" => {
                            let Some(per) = vv.as_object() else { continue };
                            for (n, list) in per {
                                let u = match (k.as_str(), n.trim_end_matches('e').parse::<i32>()) {
                                    ("daily", _) if n == "pb" => Uses::DailyPb,
                                    ("daily", Ok(n)) => Uses::Daily(n),
                                    ("rest", Ok(n)) => Uses::Rest(n),
                                    _ => {
                                        self.unread
                                            .push(format!("additionalSpells {mode} {k} {n}"));
                                        continue;
                                    }
                                };
                                self.grants_of(list, level, u, mode, out);
                            }
                        }
                        other => self.unread.push(format!("additionalSpells {mode} {other}")),
                    }
                }
            }
            other => self
                .unread
                .push(format!("additionalSpells {mode} {}", shape(other))),
        }
    }

    /// A subrace on its race: the race's fields, except those the subrace replaces
    /// (`overwrite: {field: true}`, `field: null`, or a speed of its own), plus the subrace's.
    pub fn merged(parent: &Self, sub: &Self, replaced: &BTreeSet<String>) -> Self {
        let r = |f: &str| replaced.contains(f);
        let mut e = sub.clone();
        if !r("ability") {
            for i in 0..6 {
                e.fixed[i] += parent.fixed[i];
            }
            e.ability_choices
                .extend(parent.ability_choices.iter().cloned());
        }
        let keep = |kind: Kind| match kind {
            Kind::Skill => !r("skillProficiencies"),
            Kind::Tool => !r("toolProficiencies"),
            Kind::Language => !r("languageProficiencies"),
        };
        if keep(Kind::Skill) {
            e.skills.extend(parent.skills.iter().cloned());
        }
        if keep(Kind::Tool) {
            e.tools.extend(parent.tools.iter().cloned());
        }
        if keep(Kind::Language) {
            e.languages.extend(parent.languages.iter().cloned());
        }
        e.choices
            .extend(parent.choices.iter().filter(|c| keep(c.kind)).cloned());
        e.alternatives
            .extend(parent.alternatives.iter().filter(|c| keep(c.0)).cloned());
        if e.speed.is_none() && !r("speed") {
            e.speed = parent.speed;
        }
        if !r("resist") {
            e.resist.extend(parent.resist.iter().cloned());
            e.defence_choices.extend(
                parent
                    .defence_choices
                    .iter()
                    .filter(|c| c.0 == "resistance")
                    .cloned(),
            );
        }
        if !r("immune") {
            e.immune.extend(parent.immune.iter().cloned());
        }
        if !r("conditionImmune") {
            e.condition_immune
                .extend(parent.condition_immune.iter().cloned());
        }
        if !r("additionalSpells") {
            let mut sets = parent.spell_sets.clone();
            sets.extend(sub.spell_sets.iter().cloned());
            e.spell_sets = sets;
        }
        e.unread.extend(parent.unread.iter().cloned());
        e
    }
}

fn shape(v: &Value) -> String {
    match v {
        Value::Object(o) => format!(
            "object {{{}}}",
            o.keys().cloned().collect::<Vec<_>>().join(",")
        ),
        Value::Array(_) => "array".into(),
        Value::String(_) => "string".into(),
        other => other.to_string(),
    }
}

/// Fields a subrace replaces on its race (5etools merge rule).
pub fn replaced(sub: &Map<String, Value>) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = sub
        .get("overwrite")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter(|(_, v)| v.as_bool() == Some(true))
                .map(|(k, _)| k.clone())
                .collect()
        })
        .unwrap_or_default();
    out.extend(
        sub.iter()
            .filter(|(_, v)| v.is_null())
            .map(|(k, _)| k.clone()),
    );
    if sub.get("speed").is_some_and(|v| !v.is_null()) {
        out.insert("speed".into());
    }
    out
}

/// Language dialects by lower-case name -> the language that holds them (`languages.json`
/// `dialects`: Auran, Aquan, Ignan and Terran are Primordial).
pub fn dialects(data: &Path) -> HashMap<String, String> {
    let loaded = folder::load(data, |_, _, _| {}).expect("load folder");
    let mut out = HashMap::new();
    for e in loaded.props.get("language").into_iter().flatten() {
        let Some(name) = e.value.get("name").and_then(Value::as_str) else {
            continue;
        };
        for d in strings(e.value.get("dialects")) {
            out.insert(d, name.to_lowercase());
        }
    }
    out
}

/// One audited entry: the element it became and what its fields say.
pub struct Entry {
    /// `species`, `subrace`, `background` or `feat`.
    pub kind: &'static str,
    pub id: String,
    pub ruleset: Ruleset,
    /// For a subrace: its race's element id.
    pub parent: Option<String>,
    pub expect: Expect,
}

/// Raw entries matched to elements, plus the entries that matched none or several.
#[derive(Default)]
pub struct Matched {
    pub entries: Vec<Entry>,
    pub unmatched: BTreeMap<&'static str, usize>,
    pub ambiguous: BTreeMap<&'static str, usize>,
}

fn name_of(m: &Map<String, Value>) -> &str {
    m.get("name").and_then(Value::as_str).unwrap_or("")
}

fn source_of(m: &Map<String, Value>) -> &str {
    m.get("source").and_then(Value::as_str).unwrap_or("")
}

/// Read and resolve the folder, then match each origin / feat entry to its element by type,
/// name and source id prefix.
pub fn load(data: &Path, ours: &HashMap<String, (Element, Ruleset)>) -> Matched {
    let loaded = folder::load(data, |_, _, _| {}).expect("load folder");
    let resolve = |prop: &str| -> Vec<Map<String, Value>> {
        let raw: Vec<Value> = loaded
            .props
            .get(prop)
            .map(|v| v.iter().map(|e| e.value.clone()).collect())
            .unwrap_or_default();
        copy::resolve_array(prop, &raw)
            .entries
            .into_iter()
            .filter_map(|v| match v {
                Value::Object(m) => Some(m),
                _ => None,
            })
            .collect()
    };
    let mut by_name: HashMap<(&str, &str), Vec<&str>> = HashMap::new();
    for (id, (e, _)) in ours {
        let ty = match e.ty.as_str() {
            "Race" | "Species" => "race",
            "Sub Race" => "subrace",
            "Background" => "background",
            "Feat" => "feat",
            _ => continue,
        };
        by_name
            .entry((ty, e.name.as_str()))
            .or_default()
            .push(id.as_str());
    }
    let mut out = Matched::default();
    let find = |ty: &'static str, name: &str, source: &str, out: &mut Matched| -> Option<String> {
        let prefix = format!("ID_5ET_{}_", id_fragment(source));
        let hits: Vec<&str> = by_name
            .get(&(ty, name))
            .map(|v| {
                v.iter()
                    .copied()
                    .filter(|id| id.starts_with(&prefix))
                    .collect()
            })
            .unwrap_or_default();
        match hits.as_slice() {
            [one] => Some((*one).to_string()),
            [] => {
                *out.unmatched.entry(ty).or_default() += 1;
                None
            }
            _ => {
                *out.ambiguous.entry(ty).or_default() += 1;
                None
            }
        }
    };
    let races = resolve("race");
    let mut race_ids: HashMap<(String, String), (String, Expect)> = HashMap::new();
    for r in &races {
        let Some(id) = find("race", name_of(r), source_of(r), &mut out) else {
            continue;
        };
        let expect = Expect::read(r);
        race_ids.insert(
            (name_of(r).to_lowercase(), source_of(r).to_uppercase()),
            (id.clone(), expect.clone()),
        );
        if r.contains_key(copy::VERSIONS) {
            // a race whose versions are its options: audited through each version
            continue;
        }
        // a version of such a race is the race with its option picked (the former species
        // element, `id`, is no longer offered)
        let option = r.contains_key(copy::VERSION_OF).then(|| {
            ours.iter()
                .find(|(_, (el, _))| el.data.get("former_id").and_then(Value::as_str) == Some(&id))
                .map(|(oid, (el, _))| {
                    (
                        oid.clone(),
                        el.data
                            .get("version_of")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    )
                })
        });
        let (id, parent) = match option {
            Some(Some((oid, parent))) => (oid, parent),
            Some(None) => {
                *out.unmatched.entry("race version option").or_default() += 1;
                continue;
            }
            None => (id, None),
        };
        out.entries.push(Entry {
            kind: "species",
            ruleset: ours[&id].1,
            id,
            parent,
            expect,
        });
    }
    for s in resolve("subrace") {
        let race = s.get("raceName").and_then(Value::as_str).unwrap_or("");
        let race_src = s
            .get("raceSource")
            .and_then(Value::as_str)
            .unwrap_or(source_of(&s));
        let own = name_of(&s);
        let shown = format!("{race} ({})", if own.is_empty() { "Standard" } else { own });
        let Some(id) = find("subrace", &shown, source_of(&s), &mut out) else {
            continue;
        };
        let Some((pid, pexp)) = race_ids.get(&(race.to_lowercase(), race_src.to_uppercase()))
        else {
            *out.unmatched.entry("subrace parent").or_default() += 1;
            continue;
        };
        let expect = Expect::merged(pexp, &Expect::read(&s), &replaced(&s));
        out.entries.push(Entry {
            kind: "subrace",
            ruleset: ours[&id].1,
            id,
            parent: Some(pid.clone()),
            expect,
        });
    }
    for (prop, kind) in [("background", "background"), ("feat", "feat")] {
        for b in resolve(prop) {
            let Some(id) = find(kind, name_of(&b), source_of(&b), &mut out) else {
                continue;
            };
            out.entries.push(Entry {
                kind,
                ruleset: ours[&id].1,
                id,
                parent: None,
                expect: Expect::read(&b),
            });
        }
    }
    out.entries
        .sort_by(|a, b| (a.kind, &a.id).cmp(&(b.kind, &b.id)));
    out
}
