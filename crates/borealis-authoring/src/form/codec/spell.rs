//! The spell form's codecs (spec 4.1): each field bound to its engine setter and, when the element already has it,
//! its 2024 display mirror in `data` (spec 3.3). Spell lists are the spell's support tags, the way Aurora content puts
//! a spell on a class list (`supports` + `data.classes`).
//!
//! Two mirrors are shared by several fields: `data.components` (`v`, `s`, `m`: one per component check box) and
//! `data.material_cost` (`amount` and `currency` from the cost field, `consumed` from its check box). The first field
//! of each owns the object's other keys, so a key no field knows is written back as it was.

use super::super::write::Out;
use super::super::{FieldMemo, FormProblem, Slots};
use super::{
    decode, decode_raw, edited_setter, memo, present, raw_setters, stored, to_json, Reader,
};
use borealis_content::Ruleset;
use serde_json::{json, Map, Value};

/// One component check box (`data.components` key, engine setter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component {
    /// V.
    Verbal,
    /// S.
    Somatic,
    /// M.
    Material,
}

impl Component {
    fn setter(self) -> &'static str {
        match self {
            Self::Verbal => "hasVerbalComponent",
            Self::Somatic => "hasSomaticComponent",
            Self::Material => "hasMaterialComponent",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Verbal => "v",
            Self::Somatic => "s",
            Self::Material => "m",
        }
    }
}

const COMPONENTS: &str = "components";
const MATERIAL_COST: &str = "material_cost";
const AMOUNT: &str = "amount";
const CURRENCY: &str = "currency";
const CONSUMED: &str = "consumed";
const GOLD: &str = "gp";
/// The school setter's attribute that names a ritual in 2024 content (`<set name="school" addition="ritual">`).
const ADDITION: &str = "addition";
const RITUAL: &str = "ritual";
const LEVELS: std::ops::RangeInclusive<u8> = 0..=9;

/// A spell field (`Codec::Spell`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellField {
    /// Setter `level` (`"0"` = cantrip) + `data.level`.
    Level,
    /// Setter `school` + `data.school`.
    School,
    /// One component: its setter (`"true"`/`"false"`) + its key of `data.components`.
    Component(Component),
    /// Setter `isConcentration` + `data.concentration`.
    Concentration,
    /// Setter `isRitual` + `data.ritual`.
    Ritual,
    /// The spell lists: every support tag + `data.classes`.
    Lists,
    /// The material cost in gp: `data.material_cost.amount` (and its currency).
    MaterialCost,
    /// The material is used up: `data.material_cost.consumed`.
    MaterialConsumed,
}

impl SpellField {
    /// The engine setter.
    fn setter(self) -> Option<&'static str> {
        match self {
            Self::Level => Some("level"),
            Self::School => Some("school"),
            Self::Component(c) => Some(c.setter()),
            Self::Concentration => Some("isConcentration"),
            Self::Ritual => Some("isRitual"),
            Self::Lists | Self::MaterialCost | Self::MaterialConsumed => None,
        }
    }

    /// The `data` key it reads (shared keys included).
    fn data_key(self) -> &'static str {
        match self {
            Self::Level => "level",
            Self::School => "school",
            Self::Component(_) => COMPONENTS,
            Self::Concentration => "concentration",
            Self::Ritual => "ritual",
            Self::Lists => "classes",
            Self::MaterialCost | Self::MaterialConsumed => MATERIAL_COST,
        }
    }

    /// The part of a shared `data` object it edits.
    fn part(self) -> Option<&'static str> {
        match self {
            Self::Component(c) => Some(c.key()),
            Self::MaterialCost => Some(AMOUNT),
            Self::MaterialConsumed => Some(CONSUMED),
            _ => None,
        }
    }

    /// The model locations, as the generated-data view shows them.
    pub(crate) fn bindings(self) -> Vec<String> {
        let data = match self.part() {
            Some(p) => format!("data.{}.{p}", self.data_key()),
            None => format!("data.{}", self.data_key()),
        };
        match self {
            Self::Lists => vec!["supports".into(), data],
            _ => self
                .setter()
                .map(|s| format!("setter:{s}"))
                .into_iter()
                .chain(std::iter::once(data))
                .collect(),
        }
    }

    /// The setter names it takes, lower case.
    pub(crate) fn setter_names(self) -> Vec<&'static str> {
        match self {
            Self::Level => vec!["level"],
            Self::School => vec!["school"],
            Self::Component(Component::Verbal) => vec!["hasverbalcomponent"],
            Self::Component(Component::Somatic) => vec!["hassomaticcomponent"],
            Self::Component(Component::Material) => vec!["hasmaterialcomponent"],
            Self::Concentration => vec!["isconcentration"],
            Self::Ritual => vec!["isritual"],
            Self::Lists | Self::MaterialCost | Self::MaterialConsumed => Vec::new(),
        }
    }

    /// The `data` keys it owns (a shared object belongs to its first field).
    pub(crate) fn data_keys(self) -> Vec<&'static str> {
        match self {
            Self::Component(Component::Somatic | Component::Material) | Self::MaterialConsumed => {
                Vec::new()
            }
            _ => vec![self.data_key()],
        }
    }

    /// It holds every support tag.
    pub(crate) fn owns_supports(self) -> bool {
        self == Self::Lists
    }
}

fn data_bool(v: Option<&Value>) -> Option<bool> {
    match v? {
        Value::Bool(b) => Some(*b),
        Value::String(s) => borealis_content::setter::parse_bool(s),
        _ => None,
    }
}

fn data_text(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Read one spell field.
pub(crate) fn read(field: SpellField, r: &mut Reader<'_>) -> FieldMemo {
    if field == SpellField::Lists {
        let (tags, slots) = r.take_tags(|_| true);
        let data = r.take_data(field.data_key());
        return memo(
            json!(tags),
            json!({ "tags": tags, "data": present(data) }),
            Slots {
                supports: slots,
                ..Slots::default()
            },
        );
    }
    let setter = field.setter().and_then(|n| r.take_setter(n));
    let data = r.take_data(field.data_key());
    let part = match field.part() {
        Some(p) => data.and_then(|o| o.get(p)),
        None => data,
    };
    let value = match field {
        SpellField::Level | SpellField::School => json!(setter
            .map(|(_, s)| s.value.trim().to_string())
            .or_else(|| data_text(data))
            .unwrap_or_default()),
        SpellField::MaterialCost => json!(data_text(part).unwrap_or_default()),
        _ => json!(setter
            .and_then(|(_, s)| s.as_bool())
            .or_else(|| data_bool(part))
            .unwrap_or(false)),
    };
    let raw = json!({
        "setter": setter.map_or(Value::Null, |(_, s)| to_json(s)),
        "data": present(data),
    });
    let slots = Slots {
        setters: setter.map(|(i, _)| i).into_iter().collect(),
        ..Slots::default()
    };
    memo(value, raw, slots)
}

/// The stored `data` value of a field, if the element had it.
fn stored_data(raw: Option<&Value>) -> Option<&Value> {
    raw.and_then(|r| stored(&r["data"]))
}

/// Merge `entries` into the object at `data[key]` (created when absent).
fn merge(out: &mut Out, key: &str, entries: Map<String, Value>) {
    let slot = out
        .element
        .data
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if let Value::Object(o) = slot {
        o.extend(entries);
    }
}

/// The stored object's keys that belong to the field owning the shared object (all but the other fields' parts).
fn owned_entries(stored: &Value, others: &[&str]) -> Map<String, Value> {
    stored
        .as_object()
        .map(|o| {
            o.iter()
                .filter(|(k, _)| !others.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// Write the shared-object part of an unchanged field back.
fn write_shared_raw(field: SpellField, data: &Value, out: &mut Out) {
    let key = field.data_key();
    match field {
        SpellField::Component(Component::Verbal) if !data.is_object() => {
            out.element.data.insert(key.into(), data.clone());
        }
        SpellField::Component(Component::Verbal) => {
            merge(out, key, owned_entries(data, &["s", "m"]));
        }
        SpellField::MaterialCost if !data.is_object() => {
            out.element.data.insert(key.into(), data.clone());
        }
        SpellField::MaterialCost => merge(out, key, owned_entries(data, &[CONSUMED])),
        _ => {
            let part = field.part().unwrap_or_default();
            if let Some(v) = data.get(part) {
                // "used up" alone stays when the price was cleared (see `write`)
                let alone = field == SpellField::MaterialConsumed && v == &json!(true);
                if alone || out.element.data.get(key).is_some_and(Value::is_object) {
                    merge(out, key, Map::from_iter([(part.to_string(), v.clone())]));
                }
            }
        }
    }
}

/// Put the stored bytes of an unchanged spell field back.
pub(crate) fn write_raw(field: SpellField, m: &FieldMemo, out: &mut Out) {
    let raw = &m.raw;
    if field == SpellField::Lists {
        let tags: Vec<String> = decode_raw(&raw["tags"]).unwrap_or_default();
        out.supports(tags, &m.slots.supports);
    } else {
        out.setters(raw_setters(&raw["setter"]), &m.slots.setters);
    }
    let Some(data) = stored(&raw["data"]) else {
        return;
    };
    if field.part().is_some() {
        write_shared_raw(field, data, out);
    } else {
        out.element
            .data
            .insert(field.data_key().into(), data.clone());
    }
}

fn level(text: &str, path: &str) -> Result<Option<u8>, FormProblem> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    t.parse::<u8>()
        .ok()
        .filter(|n| LEVELS.contains(n))
        .map(Some)
        .ok_or_else(|| FormProblem::new(path, "Choose the spell's level from the list."))
}

/// A gold amount as typed ("1,500", "2.5") as a JSON number; `None` for empty text.
fn gold(text: &str, path: &str) -> Result<Option<Value>, FormProblem> {
    let t: String = text.trim().chars().filter(|c| *c != ',').collect();
    if t.is_empty() {
        return Ok(None);
    }
    let bad = || FormProblem::new(path, "Write the cost as a number of gold pieces, like 50.");
    if let Ok(n) = t.parse::<u64>() {
        return Ok(Some(json!(n)));
    }
    let f: f64 = t.parse().map_err(|_| bad())?;
    if f.is_finite() && f >= 0.0 {
        serde_json::Number::from_f64(f)
            .map(|n| Some(Value::Number(n)))
            .ok_or_else(bad)
    } else {
        Err(bad())
    }
}

/// The mirror value written for an edited field: same JSON type as the stored one.
fn mirror_text(stored: &Value, text: &str, number: Option<u8>) -> Value {
    match (stored, number) {
        (Value::Number(_), Some(n)) => json!(n),
        _ => json!(text),
    }
}

/// Write an edited spell field to every location it is bound to.
pub(crate) fn write(
    field: SpellField,
    value: &Value,
    memo: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
) -> Result<(), FormProblem> {
    let raw = memo.map(|m| &m.raw);
    let slots = memo.map(|m| m.slots.clone()).unwrap_or_default();
    let stored_setter = raw.map(|r| &r["setter"]).filter(|s| !s.is_null());
    let data = stored_data(raw);
    match field {
        SpellField::Lists => {
            let tags: Vec<String> = decode(value, path)?;
            let mut clean: Vec<String> = Vec::new();
            for t in tags.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
                if !clean.iter().any(|c| c.eq_ignore_ascii_case(t)) {
                    clean.push(t.to_string());
                }
            }
            if data.is_some() {
                out.element
                    .data
                    .insert(field.data_key().into(), json!(clean));
            }
            out.supports(clean, &slots.supports);
        }
        SpellField::Level | SpellField::School => {
            let text: String = decode(value, path)?;
            let text = text.trim().to_string();
            let number = if field == SpellField::Level {
                level(&text, path)?
            } else {
                None
            };
            let text = number.map_or(text, |n| n.to_string());
            if text.is_empty() {
                return Ok(());
            }
            let name = field.setter().unwrap_or_default();
            out.setters(
                Some(edited_setter(name, text.clone(), stored_setter)),
                &slots.setters,
            );
            if let Some(d) = data {
                out.element
                    .data
                    .insert(field.data_key().into(), mirror_text(d, &text, number));
            }
        }
        SpellField::MaterialCost => {
            let text: String = decode(value, path)?;
            let Some(amount) = gold(&text, path)? else {
                return Ok(());
            };
            let mut entries = data
                .map(|d| owned_entries(d, &[CONSUMED]))
                .unwrap_or_default();
            entries.insert(AMOUNT.into(), amount);
            entries
                .entry(CURRENCY.to_string())
                .or_insert_with(|| json!(GOLD));
            if data.is_none() {
                entries.insert(CONSUMED.into(), json!(false));
            }
            merge(out, MATERIAL_COST, entries);
        }
        SpellField::MaterialConsumed => {
            let on: bool = decode(value, path)?;
            // with a cost it is part of the cost; alone, only "used up" is worth keeping (a material with no price)
            if on
                || out
                    .element
                    .data
                    .get(MATERIAL_COST)
                    .is_some_and(Value::is_object)
            {
                merge(
                    out,
                    MATERIAL_COST,
                    Map::from_iter([(CONSUMED.into(), json!(on))]),
                );
            }
        }
        SpellField::Component(_) | SpellField::Concentration | SpellField::Ritual => {
            let on: bool = decode(value, path)?;
            let name = field.setter().unwrap_or_default();
            out.setters(
                Some(edited_setter(name, on.to_string(), stored_setter)),
                &slots.setters,
            );
            match (field, data) {
                (_, None) => {}
                (SpellField::Component(c), Some(d)) => {
                    let mut entries = if c == Component::Verbal {
                        owned_entries(d, &["s", "m"])
                    } else {
                        Map::new()
                    };
                    entries.insert(c.key().into(), json!(on));
                    merge(out, COMPONENTS, entries);
                }
                (_, Some(_)) => {
                    out.element.data.insert(field.data_key().into(), json!(on));
                }
            }
            if field == SpellField::Ritual {
                let is_2024 = data.is_some() || out.element.ruleset == Some(Ruleset::Dnd2024);
                ritual_addition(on, is_2024, out);
            }
        }
    }
    Ok(())
}

/// Keep the 2024 school setter's `addition="ritual"` in step with an edited Ritual (the school is written first): on
/// adds it to a 2024 spell's school with no addition, off takes it away. An addition of the author's own words stays.
fn ritual_addition(on: bool, is_2024: bool, out: &mut Out) {
    let Some(school) = out.setter_mut(SpellField::School.setter().unwrap_or_default()) else {
        return;
    };
    let current = school
        .attributes
        .get(ADDITION)
        .map(|a| a.trim().to_string());
    match current {
        None if on && is_2024 => {
            school.attributes.insert(ADDITION.into(), RITUAL.into());
        }
        Some(a) if !on && a.eq_ignore_ascii_case(RITUAL) => {
            school.attributes.remove(ADDITION);
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "spell_tests.rs"]
mod tests;
