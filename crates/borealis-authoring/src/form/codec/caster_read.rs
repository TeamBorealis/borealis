//! Reading the spellcasting section (spec 4.6): which stored rules each field holds. A rule field takes rules only
//! when they are exactly what it writes for the value it reads (in the stored spelling); the rest stay blocks.

use super::super::{FieldMemo, Slots};
use super::caster::LevelCounts;
use super::{to_json, Reader};
use crate::caster::section::{
    self, SlotGrid, SlotTable, Spelling, MULTICLASS_GRANTS, STANDARD_TABLES,
};
use crate::Edition;
use borealis_content::{Element, Ruleset, Spellcasting};
use serde_json::{json, Value};

/// A node the section edits as a whole: named, not a list extension, nothing kept besides.
pub(super) fn section_node(e: &Element) -> Option<&Spellcasting> {
    let sc = e.spellcasting.as_ref()?;
    let list = sc.list.as_ref()?;
    let plain = sc.name.as_deref().is_some_and(|n| !n.trim().is_empty())
        && sc.extend.is_none()
        && sc.all.is_none()
        && sc.extends.is_empty()
        && sc.extra.is_empty()
        && list.prepared.is_none()
        && list.extra.is_empty();
    plain.then_some(sc)
}

/// What reading a rule field needs: the section's stored name and edition.
pub(super) struct ReadCx {
    pub(super) name: String,
    pub(super) edition: Edition,
    pub(super) ability: String,
}

pub(super) fn read_cx(e: &Element) -> Option<ReadCx> {
    let sc = section_node(e)?;
    Some(ReadCx {
        name: sc.name.clone().unwrap_or_default(),
        edition: if e.ruleset == Some(Ruleset::Dnd2024) {
            Edition::Y2024
        } else {
            Edition::Y2014
        },
        ability: sc.ability.clone().unwrap_or_default(),
    })
}

pub(super) fn text_of<'v>(v: &'v Value, key: &str) -> &'v str {
    v.get(key).and_then(Value::as_str).unwrap_or_default()
}

pub(super) fn level_of(v: &Value) -> u32 {
    v.get("level")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(1)
}

pub(super) fn is_stat_named(v: &Value, name: &str) -> bool {
    text_of(v, "kind") == "stat" && text_of(v, "name") == name
}

/// The stored rules equal to `wanted`, one each, in `wanted`'s order; `None` unless every one is there.
pub(super) fn find_exact(rules: &[(usize, Value)], wanted: &[Value]) -> Option<Vec<usize>> {
    let mut picked: Vec<usize> = Vec::new();
    for w in wanted {
        let (i, _) = rules.iter().find(|(i, r)| r == w && !picked.contains(i))?;
        picked.push(*i);
    }
    Some(picked)
}

pub(super) fn free(r: &Reader<'_>) -> Vec<(usize, Value)> {
    r.free_rules().map(|(i, x)| (i, to_json(x))).collect()
}

pub(super) fn all(e: &Element) -> Vec<(usize, Value)> {
    e.rules
        .iter()
        .enumerate()
        .map(|(i, x)| (i, to_json(x)))
        .collect()
}

/// A slot stat of the section: `<name>:spellcasting:slots:N`, 2014 pact magic's `slots:count` and `slot`.
pub(super) fn is_slot_stat(v: &Value, lc: &str) -> bool {
    if text_of(v, "kind") != "stat" {
        return false;
    }
    let name = text_of(v, "name");
    let Some(part) = name
        .strip_prefix(lc)
        .and_then(|n| n.strip_prefix(":spellcasting:"))
    else {
        return false;
    };
    part == "slot"
        || part
            .strip_prefix("slots:")
            .is_some_and(|k| k == "count" || matches!(k.parse::<u8>(), Ok(1..=9)))
}

/// A custom table from per-level slot stats (whole numbers only).
pub(super) fn custom_grid(stats: &[&Value], lc: &str) -> Option<SlotGrid> {
    let mut grid = [[0i64; 9]; 20];
    for v in stats {
        let k: usize = text_of(v, "name")
            .strip_prefix(&format!("{lc}:spellcasting:slots:"))?
            .parse()
            .ok()
            .filter(|k| (1..=9).contains(k))?;
        let n: i64 = text_of(v, "value").parse().ok()?;
        let level = level_of(v) as usize;
        if !(1..=20).contains(&level) {
            return None;
        }
        for row in grid.iter_mut().skip(level - 1) {
            row[k - 1] += n;
        }
    }
    let mut out = [[0u32; 9]; 20];
    for (o, g) in out.iter_mut().zip(grid) {
        for (x, n) in o.iter_mut().zip(g) {
            *x = u32::try_from(n).ok().filter(|n| *n <= 9)?;
        }
    }
    Some(out)
}

/// The caster type of the stored slot rules among `rules`, and their positions: a standard type when they are
/// exactly its rules, else a custom table when they reproduce one.
pub(super) fn detect_slots(
    rules: &[(usize, Value)],
    cx: &ReadCx,
) -> Option<(SlotTable, Vec<usize>)> {
    let lc = cx.name.to_lowercase();
    let stats: Vec<&Value> = rules
        .iter()
        .map(|(_, v)| v)
        .filter(|v| is_slot_stat(v, &lc))
        .collect();
    if stats.is_empty() {
        return None;
    }
    let custom = custom_grid(&stats, &lc).map(|g| SlotTable::Custom(Box::new(g)));
    STANDARD_TABLES.iter().cloned().chain(custom).find_map(|t| {
        let wanted = section::slot_rules(&t, &cx.name, cx.edition);
        let stat_count = wanted
            .iter()
            .filter(|w| text_of(w, "kind") == "stat")
            .count();
        (stat_count == stats.len())
            .then(|| find_exact(rules, &wanted))
            .flatten()
            .map(|at| (t, at))
    })
}

pub(super) fn take(r: &mut Reader<'_>, at: &[usize]) {
    for &i in at {
        r.take_rule(i);
    }
}

pub(super) fn rule_memo(value: Value, raw: Value, at: Vec<usize>) -> FieldMemo {
    FieldMemo {
        value,
        raw,
        slots: Slots {
            rules: at,
            ..Slots::default()
        },
    }
}

/// The form value of a multiclass grant suffix.
pub(super) fn multiclass_value(suffix: &str) -> Option<&'static str> {
    MULTICLASS_GRANTS
        .iter()
        .find(|(s, _)| *s == suffix)
        .map(|(_, v)| *v)
}

/// The multiclass grant suffix a form value gives (`auto`: the caster type's; `none` or no type: none).
pub(crate) fn multiclass_suffix(value: &str, table: Option<&SlotTable>) -> Option<&'static str> {
    match value {
        "" | "auto" => table.map(SlotTable::multiclass),
        "none" => None,
        v => MULTICLASS_GRANTS
            .iter()
            .find(|(_, k)| *k == v)
            .map(|(s, _)| *s),
    }
}

/// A cumulative count per level from choices (`number` at `level`).
pub(super) fn table_from_choices(choices: &[&Value]) -> [u32; 20] {
    let mut out = [0u32; 20];
    for c in choices {
        let n = c
            .get("number")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(1);
        let level = level_of(c).clamp(1, 20) as usize;
        for x in out.iter_mut().skip(level - 1) {
            *x += n;
        }
    }
    out
}

/// A cumulative count per level from delta stats (whole numbers only).
pub(super) fn table_from_stats(stats: &[&Value]) -> Option<[u32; 20]> {
    let mut acc = [0i64; 20];
    for s in stats {
        let n: i64 = text_of(s, "value").parse().ok()?;
        let level = level_of(s).clamp(1, 20) as usize;
        for x in acc.iter_mut().skip(level - 1) {
            *x += n;
        }
    }
    let mut out = [0u32; 20];
    for (o, a) in out.iter_mut().zip(acc) {
        *o = u32::try_from(a).ok()?;
    }
    Some(out)
}

/// The name of one of the section's choices, as [`section`] writes it.
pub(super) fn choice_word<'w>(
    name: &str,
    class: &str,
    level: u32,
    edition: Edition,
    words: &[&'w str],
) -> Option<&'w str> {
    words.iter().copied().find(|w| match edition {
        Edition::Y2014 => name == format!("{w} ({class})"),
        Edition::Y2024 => name == format!("{w} ({class} {level})"),
    })
}

pub(super) fn is_choice_of(v: &Value, class: &str) -> bool {
    text_of(v, "kind") == "select"
        && text_of(v, "type") == "Spell"
        && text_of(v, "spellcasting") == class
}

pub(super) const LIST: &str = "$(spellcasting:list)";
pub(super) const KNOWN_WORDS: [&str; 3] = ["Spell", "Spell Known", "Spells Known"];

/// The filter's part after the list: `0` for cantrips, the spell levels for spells; with the spelling's space.
pub(super) fn filter_rest(v: &Value) -> Option<(bool, &str)> {
    let rest = text_of(v, "supports")
        .strip_prefix(LIST)?
        .strip_prefix(',')?;
    Some(match rest.strip_prefix(' ') {
        Some(r) => (true, r),
        None => (false, rest),
    })
}

pub(super) fn spelling_raw(s: &Spelling) -> Value {
    json!({"spaced": s.spaced, "word": s.word, "dynamic": s.dynamic})
}

pub(super) fn spelling_of(raw: Option<&Value>, edition: Edition) -> Spelling {
    let base = Spelling::new(edition);
    let Some(raw) = raw.filter(|r| r.is_object()) else {
        return base;
    };
    Spelling {
        spaced: raw
            .get("spaced")
            .and_then(Value::as_bool)
            .unwrap_or(base.spaced),
        word: raw
            .get("word")
            .and_then(Value::as_str)
            .map_or(base.word, str::to_string),
        dynamic: raw
            .get("dynamic")
            .and_then(Value::as_bool)
            .unwrap_or(base.dynamic),
    }
}

pub(super) fn counts_value(levels: [u32; 20], statistic: bool, replace: bool) -> Value {
    to_json(&LevelCounts {
        levels: levels.to_vec(),
        statistic,
        replace,
    })
}

pub(super) fn read_cantrips(r: &mut Reader<'_>, cx: &ReadCx) -> FieldMemo {
    let none = || {
        let empty = counts_value([0; 20], cx.edition == Edition::Y2014, false);
        rule_memo(empty, Value::Null, Vec::new())
    };
    let rules = free(r);
    let stat = section::stat_name(&cx.name, "cantrips known");
    let choices: Vec<&Value> = rules
        .iter()
        .map(|(_, v)| v)
        .filter(|v| {
            is_choice_of(v, &cx.name)
                && choice_word(
                    text_of(v, "name"),
                    &cx.name,
                    level_of(v),
                    cx.edition,
                    &["Cantrip"],
                )
                .is_some()
                && filter_rest(v).is_some_and(|(_, rest)| rest == "0")
        })
        .collect();
    let stats: Vec<&Value> = rules
        .iter()
        .map(|(_, v)| v)
        .filter(|v| is_stat_named(v, &stat))
        .collect();
    if choices.is_empty() && stats.is_empty() {
        return none();
    }
    let table = if choices.is_empty() {
        table_from_stats(&stats)
    } else {
        Some(table_from_choices(&choices))
    };
    let Some(table) = table else { return none() };
    let spelling = Spelling {
        spaced: choices
            .first()
            .and_then(|c| filter_rest(c))
            .map_or(cx.edition == Edition::Y2024, |(s, _)| s),
        ..Spelling::new(cx.edition)
    };
    let statistic = !stats.is_empty();
    let wanted = section::cantrip_rules(&cx.name, &table, statistic, &spelling, cx.edition);
    match find_exact(&rules, &wanted).filter(|at| at.len() == choices.len() + stats.len()) {
        Some(at) => {
            take(r, &at);
            rule_memo(
                counts_value(table, statistic, false),
                spelling_raw(&spelling),
                at,
            )
        }
        None => none(),
    }
}

pub(super) fn read_known(r: &mut Reader<'_>, cx: &ReadCx) -> FieldMemo {
    let none = || rule_memo(counts_value([0; 20], true, false), Value::Null, Vec::new());
    let rules = free(r);
    let stat = section::stat_name(&cx.name, "spells known");
    let choices: Vec<&Value> = rules
        .iter()
        .map(|(_, v)| v)
        .filter(|v| {
            is_choice_of(v, &cx.name)
                && choice_word(
                    text_of(v, "name"),
                    &cx.name,
                    level_of(v),
                    cx.edition,
                    &KNOWN_WORDS,
                )
                .is_some()
                && filter_rest(v).is_some_and(|(_, rest)| rest != "0")
        })
        .collect();
    let stats: Vec<&Value> = rules
        .iter()
        .map(|(_, v)| v)
        .filter(|v| is_stat_named(v, &stat))
        .collect();
    if choices.is_empty() && stats.is_empty() {
        return none();
    }
    let table = if choices.is_empty() {
        table_from_stats(&stats)
    } else {
        Some(table_from_choices(&choices))
    };
    let Some(table) = table else { return none() };
    let first = choices.first();
    let spelling = Spelling {
        spaced: first.and_then(|c| filter_rest(c)).is_some_and(|(s, _)| s),
        word: first
            .and_then(|c| {
                choice_word(
                    text_of(c, "name"),
                    &cx.name,
                    level_of(c),
                    cx.edition,
                    &KNOWN_WORDS,
                )
            })
            .unwrap_or("Spell")
            .to_string(),
        dynamic: first
            .and_then(|c| filter_rest(c))
            .is_some_and(|(_, rest)| rest == "$(spellcasting:slots)"),
    };
    let replace = first.is_some_and(|c| c.get("allow_replace") == Some(&json!(true)));
    let statistic = !stats.is_empty();
    let grid = detect_slots(&all(r.e), cx).map_or([[0; 9]; 20], |(t, _)| t.grid());
    let wanted = section::known_rules(
        &cx.name, &table, statistic, replace, &spelling, &grid, cx.edition,
    );
    match find_exact(&rules, &wanted).filter(|at| at.len() == choices.len() + stats.len()) {
        Some(at) => {
            take(r, &at);
            rule_memo(
                counts_value(table, statistic, replace),
                spelling_raw(&spelling),
                at,
            )
        }
        None => none(),
    }
}

pub(super) fn read_prepared(r: &mut Reader<'_>, cx: &ReadCx, formula: bool) -> FieldMemo {
    let rules = free(r);
    let stat = section::stat_name(&cx.name, "prepare");
    let stats: Vec<&Value> = rules
        .iter()
        .map(|(_, v)| v)
        .filter(|v| is_stat_named(v, &stat))
        .collect();
    let empty = if formula { json!("") } else { json!([]) };
    let found = if formula {
        [("level", false), ("half", true)]
            .into_iter()
            .find_map(|(key, half)| {
                let wanted = section::prepare_formula_rules(&cx.name, &cx.ability, half);
                (stats.len() == wanted.len())
                    .then(|| find_exact(&rules, &wanted))
                    .flatten()
                    .map(|at| (json!(key), at))
            })
    } else {
        table_from_stats(&stats)
            .filter(|_| !stats.is_empty())
            .and_then(|table| {
                let wanted = section::count_rules(&stat, &table, cx.edition);
                find_exact(&rules, &wanted)
                    .filter(|at| at.len() == stats.len())
                    .map(|at| (json!(table), at))
            })
    };
    match found {
        Some((value, at)) => {
            take(r, &at);
            rule_memo(value, Value::Null, at)
        }
        None => rule_memo(empty, Value::Null, Vec::new()),
    }
}
