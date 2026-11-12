//! The spellcasting section's codecs (spec 4.6): a Class Feature "Spellcasting" (or a feat) that gives spellcasting,
//! in the shapes the bundled SRD classes store it.
//!
//! * The `spellcasting` node: on or off ([`CasterField::Casts`]), its name (the class's unless given), ability,
//!   whether it prepares spells (from its whole list, `list.known`, or from a spellbook) or knows a number of them,
//!   the spell list tag and "swap one spell per level" (`allow_replace`).
//! * Rules ([`crate::caster::section`]): the caster type's slot stats (and 2024 pact magic's `pact_magic` rule), the
//!   multiclass slot grant, prepared spells per level (2024) or by formula (2014), cantrips known and spells known per
//!   level (their choices and count stats), and when 2024 prepared spells change.
//!
//! A rule field takes stored rules only when they are exactly what it writes for the value it reads (for the
//! section's name and edition, in the stored spelling), so a stored class writes back byte for byte, and its rules
//! are always written from the current name: renaming a class renames its stats and choices. What it can't reproduce
//! stays a rule block. Rules of the section are written only while it is on.

use super::super::write::Out;
use super::super::{FieldMemo, FormProblem};
use super::caster_cx::CasterCx;
use super::caster_read::{
    all, counts_value, detect_slots, find_exact, free, multiclass_suffix, multiclass_value,
    read_cantrips, read_cx, read_known, read_prepared, rule_memo, section_node, spelling_of, take,
};
use super::{decode, to_json, Reader};
use crate::caster::section::{self, SlotTable, MULTICLASS_GRANTS};
use borealis_content::{Rule, Spellcasting, SpellcastingList};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// A field of the spellcasting section (`Codec::Caster`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CasterField {
    /// The element gives spellcasting (the `spellcasting` node).
    Casts,
    /// `spellcasting.name` (empty = the class's name).
    Name,
    /// `spellcasting.ability`.
    Ability,
    /// `spellcasting.prepare` and `list.known`: `prepared`, `spellbook` or `known`.
    Preparation,
    /// `spellcasting.list.supports` (empty = the spellcasting name).
    List,
    /// `spellcasting.allow_replace`.
    Swap,
    /// The caster type: the slot stats (and 2024 `pact_magic`).
    Type,
    /// A custom caster type's slot table (20 levels x 9 slot levels).
    Slots,
    /// The multiclass slot grant.
    Multiclass,
    /// Prepared spells per level: `<name>:spellcasting:prepare` stats.
    Prepared,
    /// 2014 prepared spells: the ability modifier plus the class level (or half of it).
    PrepareFormula,
    /// Cantrips known per level: cantrip choices (+ `cantrips known` stats).
    Cantrips,
    /// Spells known per level: spell choices (+ `spells known` stats).
    Known,
    /// 2024 `prepared_spell_change`.
    PreparedChange,
}

/// The value of a count field with choices ([`CasterField::Cantrips`], [`CasterField::Known`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelCounts {
    /// The count at levels 1-20.
    #[serde(default)]
    pub levels: Vec<u32>,
    /// The count is also written as a statistic (`cantrips known`, `spells known`).
    #[serde(default)]
    pub statistic: bool,
    /// Each pick can be swapped on a later level (spell choices' `allow_replace`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub replace: bool,
}

/// Rank of the section's new rules among the element's (after the class feature's generated choices).
const RANK: u8 = 0;

impl CasterField {
    /// The model locations, as the generated-data view shows them.
    pub(crate) fn bindings(self) -> Vec<String> {
        let v: &[&str] = match self {
            Self::Casts => &["spellcasting"],
            Self::Name => &["spellcasting.name"],
            Self::Ability => &["spellcasting.ability"],
            Self::Preparation => &["spellcasting.prepare", "spellcasting.list.known"],
            Self::List => &["spellcasting.list.supports"],
            Self::Swap => &["spellcasting.allow_replace"],
            Self::Type => &["rules:stat <name>:spellcasting:slots:N", "rules:pact_magic"],
            Self::Slots => &["rules:stat <name>:spellcasting:slots:N"],
            Self::Multiclass => &["rules:grant ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_*"],
            Self::Prepared | Self::PrepareFormula => &["rules:stat <name>:spellcasting:prepare"],
            Self::Cantrips => &[
                "rules:select Cantrip (<Name>)",
                "rules:stat <name>:spellcasting:cantrips known",
            ],
            Self::Known => &[
                "rules:select Spell (<Name>)",
                "rules:stat <name>:spellcasting:spells known",
            ],
            Self::PreparedChange => &["rules:prepared_spell_change"],
        };
        v.iter().map(|s| (*s).to_string()).collect()
    }
}

/// Read one section field.
pub(crate) fn read(field: CasterField, r: &mut Reader<'_>) -> FieldMemo {
    let e = r.e;
    let node = section_node(e);
    let plain = |value: Value, raw: Value| rule_memo(value, raw, Vec::new());
    let Some(sc) = node else {
        return plain(off_value(field), Value::Null);
    };
    let cx = read_cx(e).expect("a section node");
    let list = sc.list.as_ref();
    match field {
        CasterField::Casts => {
            r.parts_taken.insert("spellcasting");
            plain(json!(true), Value::Null)
        }
        CasterField::Name => plain(json!(cx.name), json!(cx.name)),
        CasterField::Ability => plain(
            json!(sc.ability.clone().unwrap_or_default()),
            to_json(&sc.ability),
        ),
        CasterField::Preparation => {
            let known = list.and_then(|l| l.known);
            let value = match (sc.prepare, known) {
                (Some(true), Some(true)) => "prepared",
                (Some(true), _) => "spellbook",
                _ => "known",
            };
            plain(json!(value), json!({"prepare": sc.prepare, "known": known}))
        }
        CasterField::List => {
            let supports = list.map(|l| l.supports.clone()).unwrap_or_default();
            plain(json!(supports), json!(supports))
        }
        CasterField::Swap => plain(
            json!(sc.allow_replace == Some(true)),
            to_json(&sc.allow_replace),
        ),
        CasterField::Type => match detect_slots(&free(r), &cx) {
            Some((t, at)) => {
                take(r, &at);
                rule_memo(json!(t.key()), Value::Null, at)
            }
            None => plain(json!(""), Value::Null),
        },
        // the caster type took the slot rules already: the table is read from all of them
        CasterField::Slots => match detect_slots(&all(e), &cx) {
            Some((SlotTable::Custom(g), _)) => plain(json!(*g), Value::Null),
            _ => plain(json!([]), Value::Null),
        },
        CasterField::Multiclass => {
            let rules = free(r);
            let table = detect_slots(&all(e), &cx).map(|(t, _)| t);
            let found = MULTICLASS_GRANTS.iter().find_map(|(suffix, _)| {
                find_exact(&rules, &[section::multiclass_rule(suffix)]).map(|at| (*suffix, at))
            });
            match found {
                Some((suffix, at)) => {
                    take(r, &at);
                    let value = if table.as_ref().map(SlotTable::multiclass) == Some(suffix) {
                        "auto"
                    } else {
                        multiclass_value(suffix).unwrap_or("auto")
                    };
                    rule_memo(json!(value), Value::Null, at)
                }
                None if table.is_none() => plain(json!("auto"), Value::Null),
                None => plain(json!("none"), Value::Null),
            }
        }
        CasterField::Prepared => read_prepared(r, &cx, false),
        CasterField::PrepareFormula => read_prepared(r, &cx, true),
        CasterField::Cantrips => read_cantrips(r, &cx),
        CasterField::Known => read_known(r, &cx),
        CasterField::PreparedChange => {
            let rules = free(r);
            let found = section::PREPARED_CHANGES.iter().find_map(|(key, _, _)| {
                let rule = section::prepared_change_rule(key)?;
                find_exact(&rules, &[rule]).map(|at| (*key, at))
            });
            match found {
                Some((key, at)) => {
                    take(r, &at);
                    rule_memo(json!(key), Value::Null, at)
                }
                None => plain(json!(""), Value::Null),
            }
        }
    }
}

/// A field's value when the element gives no spellcasting (a new section's starting values come from the
/// descriptor's defaults).
fn off_value(field: CasterField) -> Value {
    match field {
        CasterField::Casts | CasterField::Swap => json!(false),
        CasterField::Name | CasterField::Ability | CasterField::List | CasterField::Type => {
            json!("")
        }
        CasterField::PrepareFormula | CasterField::PreparedChange => json!(""),
        CasterField::Preparation => json!("prepared"),
        CasterField::Slots | CasterField::Prepared => json!([]),
        CasterField::Multiclass => json!("auto"),
        CasterField::Cantrips | CasterField::Known => counts_value([0; 20], false, false),
    }
}

const ABILITIES: [&str; 6] = [
    "Strength",
    "Dexterity",
    "Constitution",
    "Intelligence",
    "Wisdom",
    "Charisma",
];

fn node(out: &mut Out) -> Option<&mut Spellcasting> {
    out.element.spellcasting.as_mut()
}

fn list(sc: &mut Spellcasting) -> &mut SpellcastingList {
    sc.list.get_or_insert_with(SpellcastingList::default)
}

/// A count per level (20 whole numbers); with `rising`, never going down.
fn counts(levels: &[u32], path: &str, what: &str, rising: bool) -> Result<[u32; 20], FormProblem> {
    let mut out = [0u32; 20];
    if levels.len() > 20 {
        return Err(FormProblem::new(path, "A class has 20 levels."));
    }
    for (o, n) in out.iter_mut().zip(levels) {
        if *n > 99 {
            return Err(FormProblem::new(
                path,
                format!("Write {what} as a number from 0 to 99 at each level."),
            ));
        }
        *o = *n;
    }
    if levels.len() < 20 {
        let last = levels.last().copied().unwrap_or(0);
        for o in out.iter_mut().skip(levels.len()) {
            *o = last;
        }
    }
    if rising && out.windows(2).any(|w| w[1] < w[0]) {
        return Err(FormProblem::new(
            path,
            format!("The number of {what} can't go down from one level to the next."),
        ));
    }
    Ok(out)
}

/// Write one section field.
pub(crate) fn write(
    field: CasterField,
    value: &Value,
    memo: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
) -> Result<(), FormProblem> {
    let Some(cx) = out.caster.clone().filter(|c| c.on) else {
        return Ok(());
    };
    let unchanged = memo.filter(|m| m.value == *value);
    let raw = unchanged.map(|m| &m.raw);
    match field {
        CasterField::Casts => {
            out.element.spellcasting = Some(Spellcasting::default());
            if cx.name.is_empty() {
                return Err(FormProblem::new(
                    path,
                    "Give the element a name first: its spells are counted under it.",
                ));
            }
        }
        CasterField::Name => {
            if let Some(sc) = node(out) {
                sc.name = Some(cx.name.clone());
            }
        }
        CasterField::Ability => {
            let stored = raw.and_then(|r| serde_json::from_value::<Option<String>>(r.clone()).ok());
            let ability = match stored {
                Some(s) => s,
                None => {
                    let t: String = decode(value, path)?;
                    let word = ABILITIES.iter().find(|a| a.eq_ignore_ascii_case(t.trim()));
                    match word {
                        Some(w) => Some((*w).to_string()),
                        None => {
                            return Err(FormProblem::new(path, "Choose the spellcasting ability."))
                        }
                    }
                }
            };
            if let Some(sc) = node(out) {
                sc.ability = ability;
            }
        }
        CasterField::Preparation => {
            let (prepare, known) = match raw {
                Some(r) => (
                    r.get("prepare").and_then(Value::as_bool),
                    r.get("known").and_then(Value::as_bool),
                ),
                None => match decode::<String>(value, path)?.as_str() {
                    "prepared" => (Some(true), Some(true)),
                    "spellbook" => (Some(true), None),
                    "known" => (None, None),
                    _ => {
                        return Err(FormProblem::new(
                            path,
                            "Choose how the class gets its spells.",
                        ))
                    }
                },
            };
            if let Some(sc) = node(out) {
                sc.prepare = prepare;
                list(sc).known = known;
            }
        }
        CasterField::List => {
            // unchanged, the stored tag (even an empty one); edited, empty means the spellcasting name
            let stored = raw.and_then(|r| r.as_str().map(str::to_string));
            let given: String = decode(value, path)?;
            let supports = match (stored, given.trim()) {
                (Some(s), _) => s,
                (None, "") => cx.name.clone(),
                (None, t) => t.to_string(),
            };
            if let Some(sc) = node(out) {
                list(sc).supports = supports;
            }
        }
        CasterField::Swap => {
            let swap = match raw {
                Some(r) => serde_json::from_value::<Option<bool>>(r.clone()).unwrap_or(None),
                None => decode::<bool>(value, path)?.then_some(true),
            };
            if let Some(sc) = node(out) {
                sc.allow_replace = swap;
            }
        }
        _ => write_rules(field, value, memo, path, out, &cx)?,
    }
    Ok(())
}

fn write_rules(
    field: CasterField,
    value: &Value,
    memo: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
    cx: &CasterCx,
) -> Result<(), FormProblem> {
    let slots = memo.map(|m| m.slots.rules.clone()).unwrap_or_default();
    let spelling = spelling_of(memo.map(|m| &m.raw), cx.edition);
    let rules: Vec<Value> = match field {
        CasterField::Type => {
            let key: String = decode(value, path)?;
            match (key.as_str(), cx.table.as_ref()) {
                ("", _) => Vec::new(),
                (_, Some(t)) => section::slot_rules(t, &cx.name, cx.edition),
                ("custom", None) => Vec::new(),
                _ => {
                    return Err(FormProblem::new(
                        path,
                        "Choose a caster type from the list.",
                    ))
                }
            }
        }
        CasterField::Slots => {
            if cx.custom && super::caster_cx::grid_of(value).is_none() {
                return Err(FormProblem::new(
                    path,
                    "Write a whole number of slots from 0 to 9 in each cell of the table.",
                ));
            }
            Vec::new()
        }
        CasterField::Multiclass => {
            let key: String = decode(value, path)?;
            if !matches!(key.as_str(), "" | "auto" | "none")
                && !MULTICLASS_GRANTS.iter().any(|(_, k)| *k == key)
            {
                return Err(FormProblem::new(
                    path,
                    "Choose how the class counts for multiclass spell slots.",
                ));
            }
            multiclass_suffix(&key, cx.table.as_ref())
                .map(|s| vec![section::multiclass_rule(s)])
                .unwrap_or_default()
        }
        CasterField::Prepared => {
            let levels: Vec<u32> = decode(value, path)?;
            if levels.is_empty() {
                Vec::new()
            } else {
                let table = counts(&levels, path, "prepared spells", false)?;
                section::count_rules(&section::stat_name(&cx.name, "prepare"), &table, cx.edition)
            }
        }
        CasterField::PrepareFormula => match decode::<String>(value, path)?.as_str() {
            "" => Vec::new(),
            k @ ("level" | "half") => {
                if cx.ability.is_empty() {
                    return Err(FormProblem::new(
                        path,
                        "Choose the spellcasting ability first.",
                    ));
                }
                section::prepare_formula_rules(&cx.name, &cx.ability, k == "half")
            }
            _ => {
                return Err(FormProblem::new(
                    path,
                    "Choose how many spells it prepares.",
                ))
            }
        },
        CasterField::Cantrips | CasterField::Known => {
            let v: LevelCounts = decode(value, path)?;
            let what = if field == CasterField::Cantrips {
                "cantrips known"
            } else {
                "spells known"
            };
            let table = counts(&v.levels, path, what, true)?;
            if field == CasterField::Cantrips {
                section::cantrip_rules(&cx.name, &table, v.statistic, &spelling, cx.edition)
            } else {
                section::known_rules(
                    &cx.name,
                    &table,
                    v.statistic,
                    v.replace,
                    &spelling,
                    &cx.grid(),
                    cx.edition,
                )
            }
        }
        CasterField::PreparedChange => match decode::<String>(value, path)?.as_str() {
            "" => Vec::new(),
            k => match section::prepared_change_rule(k) {
                Some(r) => vec![r],
                None => {
                    return Err(FormProblem::new(
                        path,
                        "Choose when prepared spells change.",
                    ))
                }
            },
        },
        _ => Vec::new(),
    };
    let mut written = Vec::with_capacity(rules.len());
    for r in rules {
        let rule: Rule = serde_json::from_value(r)
            .map_err(|e| FormProblem::new(path, format!("This part can't be written ({e}).")))?;
        written.push((RANK, rule));
    }
    out.field_rules(written, &slots, path);
    Ok(())
}

#[cfg(test)]
#[path = "caster_tests.rs"]
mod tests;
