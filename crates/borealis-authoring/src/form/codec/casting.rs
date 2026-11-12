//! A `spellcasting` node that is not a spellcasting section (spec 4.4, 4.7; WB-27a re-creation):
//!
//! * a trait's casting ability, `{name, ability}`: the ability a 2014 race's cantrip is cast with (the High Elf's
//!   Intelligence, the Tiefling's Charisma), counted under the race's name;
//! * a subclass feature's expanded spell list, `{name, extend: true, extends: [{supports: <spell id>}]}`: spells
//!   added to the class's list (the Fiend's), under the class's name.
//!
//! The node's name is the parent's (the race the trait belongs to, the subclass's class), the stored one while the
//! field is unchanged. A node of another shape is no field's: it stays in the preserved bucket.

use super::super::value::RefValue;
use super::super::write::Out;
use super::super::{FieldMemo, FormContext, FormProblem, Slots};
use super::{decode, to_json, Reader};
use borealis_content::{Spellcasting, SpellcastingList};
use serde_json::{json, Value};

/// A field of the node (`Codec::Casting`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastingField {
    /// `spellcasting.ability` (and its name).
    Ability,
    /// `spellcasting.extend` + `extends`: spells added to the class's list.
    Expanded,
}

const ABILITIES: [&str; 6] = [
    "Strength",
    "Dexterity",
    "Constitution",
    "Intelligence",
    "Wisdom",
    "Charisma",
];

impl CastingField {
    pub(crate) fn bindings(self) -> Vec<String> {
        let v: &[&str] = match self {
            Self::Ability => &["spellcasting.name", "spellcasting.ability"],
            Self::Expanded => &[
                "spellcasting.name",
                "spellcasting.extend",
                "spellcasting.extends",
            ],
        };
        v.iter().map(|s| (*s).to_string()).collect()
    }
}

/// An expanded list's entry: one spell id, nothing else.
fn plain_entry(l: &SpellcastingList) -> bool {
    l.supports.starts_with("ID_")
        && !l.supports.contains(['|', ',', ' '])
        && l.known.is_none()
        && l.prepared.is_none()
        && l.extra.is_empty()
}

/// The node, when it has only the parts these fields edit.
fn simple_node(sc: Option<&Spellcasting>) -> Option<&Spellcasting> {
    let sc = sc?;
    let named = sc.name.as_deref().is_some_and(|n| !n.trim().is_empty());
    let expanded = match sc.extend {
        None => sc.extends.is_empty(),
        Some(true) => !sc.extends.is_empty() && sc.extends.iter().all(plain_entry),
        Some(false) => false,
    };
    let ability = sc.ability.as_deref().is_none_or(|a| ABILITIES.contains(&a));
    let plain = named
        && expanded
        && ability
        && (sc.ability.is_some() || sc.extend.is_some())
        && sc.prepare.is_none()
        && sc.allow_replace.is_none()
        && sc.all.is_none()
        && sc.list.is_none()
        && sc.extra.is_empty();
    plain.then_some(sc)
}

fn memo(value: Value, raw: Value) -> FieldMemo {
    FieldMemo {
        value,
        raw,
        slots: Slots::default(),
    }
}

/// Read one field.
pub(crate) fn read(field: CastingField, r: &mut Reader<'_>, cx: &FormContext<'_>) -> FieldMemo {
    let Some(sc) = simple_node(r.e.spellcasting.as_ref()) else {
        return memo(empty(field), Value::Null);
    };
    r.parts_taken.insert("spellcasting");
    let value = match field {
        CastingField::Ability => json!(sc.ability.clone().unwrap_or_default()),
        CastingField::Expanded => Value::Array(
            sc.extends
                .iter()
                .map(|l| to_json(&RefValue::resolve(&l.supports, cx.lookup)))
                .collect(),
        ),
    };
    memo(value, to_json(sc))
}

fn empty(field: CastingField) -> Value {
    match field {
        CastingField::Ability => json!(""),
        CastingField::Expanded => json!([]),
    }
}

/// The node's name: the one it was read with, else the parent's (the subclass's class for an expanded list, the race for a
/// trait), else the element's own.
fn name(field: CastingField, stored: Option<&Spellcasting>, out: &Out) -> String {
    stored
        .and_then(|s| s.name.clone())
        .or_else(|| {
            let p = out.parent.as_ref()?;
            match field {
                CastingField::Expanded => p.class.clone().or_else(|| Some(p.name.clone())),
                CastingField::Ability => Some(p.name.clone()),
            }
        })
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| out.element.name.clone())
}

/// Write one field: an unchanged one puts its stored parts back, an edited one writes them canonically.
pub(crate) fn write(
    field: CastingField,
    value: &Value,
    memo: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
) -> Result<(), FormProblem> {
    let read: Option<Spellcasting> = memo.and_then(|m| serde_json::from_value(m.raw.clone()).ok());
    let stored = read
        .clone()
        .filter(|_| memo.is_some_and(|m| m.value == *value));
    match field {
        CastingField::Ability => {
            let ability = match &stored {
                Some(s) => s.ability.clone(),
                None => {
                    let t: String = decode(value, path)?;
                    let t = t.trim();
                    if t.is_empty() {
                        None
                    } else {
                        let word = ABILITIES.iter().find(|a| a.eq_ignore_ascii_case(t));
                        match word {
                            Some(w) => Some((*w).to_string()),
                            None => {
                                return Err(FormProblem::new(
                                    path,
                                    "Choose the ability its spells are cast with.",
                                ))
                            }
                        }
                    }
                }
            };
            let Some(ability) = ability else {
                return Ok(());
            };
            let name = name(field, read.as_ref(), out);
            let sc = out
                .element
                .spellcasting
                .get_or_insert_with(Spellcasting::default);
            sc.name.get_or_insert(name);
            sc.ability = Some(ability);
        }
        CastingField::Expanded => {
            let (extend, extends) = match &stored {
                Some(s) => (s.extend, s.extends.clone()),
                None => {
                    let refs: Vec<RefValue> = decode(value, path)?;
                    let ids: Vec<SpellcastingList> = refs
                        .into_iter()
                        .map(|r| r.id.trim().to_string())
                        .filter(|id| !id.is_empty())
                        .map(SpellcastingList::new)
                        .collect();
                    (Some(true), ids)
                }
            };
            if extends.is_empty() {
                return Ok(());
            }
            let name = name(field, read.as_ref(), out);
            let sc = out
                .element
                .spellcasting
                .get_or_insert_with(Spellcasting::default);
            sc.name.get_or_insert(name);
            sc.extend = extend;
            sc.extends = extends;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "casting_tests.rs"]
mod tests;
