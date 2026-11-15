//! Subclass and class option fields (spec 4.7, 4.8).
//!
//! * **Class** (`parentClass`, a picker over classes): `data.class` (the class id) + `data.class_name`. A stored
//!   subclass without them (the 2014 SRD's) shows the class whose choice offers it, and writes nothing while unchanged.
//! * **Offered by** (`subclassTag`, read-only): the support tag the class's `select Archetype` offers subclasses by
//!   (`"Fighter Subclass"`, `"Martial Archetype"`), written from the class picked ([`super::super::refresh_subclass`]).
//! * **Chosen at level** (`levelGained`, read-only): `data.level_gained`, from the class's choice.
//! * **Subclass feature levels** (`subclassLevels`, read-only): derived from the class, never written.
//! * **Group** of a class option: its first support tag (what a class's choice offers it by: `Eldritch Invocation`)
//!   + the `data.group` mirror when the element has it.
//!
//! A subclass of a class picked since it was read writes its class in every location the 2024 SRD uses; a stored 2014
//! subclass without `data` keeps its shape (spec 3.3).

use super::super::subclass::stored_class;
use super::super::value::RefValue;
use super::super::write::Out;
use super::super::{FieldMemo, FormContext, FormProblem, Slots};
use super::{decode, present, stored, to_json, Reader};
use borealis_content::rich::normalize_plain_text;
use borealis_content::Ruleset;
use serde_json::{json, Value};

/// A subclass form's field (`Codec::Subclass`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubclassField {
    /// The class: `data.class` + `data.class_name`.
    Parent,
    /// The tag its class's choice offers it by (`supports`).
    Tag,
    /// `data.level_gained`.
    LevelGained,
    /// The levels its class gives subclass features at (derived, not stored).
    FeatureLevels,
    /// A class option's group: the first support tag + the `data.group` mirror.
    OptionGroup,
}

const CLASS: &str = "class";
const CLASS_NAME: &str = "class_name";
const LEVEL_GAINED: &str = "level_gained";
const GROUP: &str = "group";
const TAG_FORBIDDEN: &[char] = &[',', '|', '&', '!', '(', ')', '[', ']', '{', '}', '$'];

impl SubclassField {
    /// The model locations, as the generated-data view shows them.
    pub(crate) fn bindings(self) -> Vec<String> {
        let v: &[&str] = match self {
            Self::Parent => &["data.class", "data.class_name"],
            Self::Tag => &["supports:<class's subclass tag>"],
            Self::LevelGained => &["data.level_gained"],
            Self::FeatureLevels => &["class:subclass feature levels (derived, not stored)"],
            Self::OptionGroup => &["supports:<group>", "data.group"],
        };
        v.iter().map(|s| (*s).to_string()).collect()
    }

    /// The `data` keys it owns.
    pub(crate) fn data_keys(self) -> Vec<&'static str> {
        match self {
            Self::Parent => vec![CLASS, CLASS_NAME],
            Self::LevelGained => vec![LEVEL_GAINED],
            Self::OptionGroup => vec![GROUP],
            Self::Tag | Self::FeatureLevels => Vec::new(),
        }
    }
}

fn plain(value: Value, raw: Value) -> FieldMemo {
    FieldMemo {
        value,
        raw,
        slots: Slots::default(),
    }
}

/// Take the first untaken support tag `want` accepts.
fn take_tag(r: &mut Reader<'_>, want: impl Fn(&str) -> bool) -> FieldMemo {
    let e = r.e;
    match (0..e.supports.len()).find(|&i| !r.supports_taken[i] && want(&e.supports[i])) {
        Some(i) => {
            r.supports_taken[i] = true;
            FieldMemo {
                value: json!(e.supports[i]),
                raw: json!(e.supports[i]),
                slots: Slots {
                    supports: vec![i],
                    ..Slots::default()
                },
            }
        }
        None => plain(json!(""), Value::Null),
    }
}

/// Read one field.
pub(crate) fn read(field: SubclassField, r: &mut Reader<'_>, cx: &FormContext<'_>) -> FieldMemo {
    let e = r.e;
    match field {
        SubclassField::Parent => {
            let class = r.take_data(CLASS);
            let name = r.take_data(CLASS_NAME);
            let value = match class.and_then(Value::as_str) {
                Some(id) => to_json(&RefValue::resolve(id, cx.lookup)),
                None => stored_class(e, cx.lookup).map_or(Value::Null, |c| {
                    to_json(&RefValue::resolve(&c.class_id, cx.lookup))
                }),
            };
            plain(
                value,
                json!({ "class": present(class), "name": present(name) }),
            )
        }
        SubclassField::Tag => {
            // the tag the class's choice offers, else "<class name> Subclass", else the first tag
            let offered = stored_class(e, cx.lookup).map(|c| c.tag);
            let named = e
                .data
                .get(CLASS_NAME)
                .and_then(Value::as_str)
                .map(|n| format!("{n} Subclass"));
            let want = offered
                .or(named)
                .filter(|t| e.supports.iter().any(|s| s == t));
            match want {
                Some(t) => take_tag(r, |s| s == t),
                None => take_tag(r, |_| true),
            }
        }
        SubclassField::LevelGained => {
            let v = r.take_data(LEVEL_GAINED);
            plain(v.cloned().unwrap_or(Value::Null), present(v))
        }
        SubclassField::FeatureLevels => {
            let levels = stored_class(e, cx.lookup)
                .map(|c| c.levels)
                .unwrap_or_default();
            plain(json!(levels), Value::Null)
        }
        SubclassField::OptionGroup => {
            let data = r.take_data(GROUP);
            let mut m = take_tag(r, |_| true);
            m.raw = json!({ "tag": m.raw, "data": present(data) });
            m
        }
    }
}

/// Put an unchanged field's stored parts back.
pub(crate) fn write_raw(field: SubclassField, m: &FieldMemo, out: &mut Out) {
    let raw = &m.raw;
    let data = |out: &mut Out, key: &str, v: &Value| {
        if let Some(v) = stored(v) {
            out.element.data.insert(key.to_string(), v.clone());
        }
    };
    match field {
        SubclassField::Parent => {
            data(out, CLASS, &raw["class"]);
            data(out, CLASS_NAME, &raw["name"]);
        }
        SubclassField::Tag => {
            if let Some(t) = raw.as_str() {
                out.supports(Some(t.to_string()), &m.slots.supports);
            }
        }
        SubclassField::LevelGained => data(out, LEVEL_GAINED, raw),
        SubclassField::FeatureLevels => {}
        SubclassField::OptionGroup => {
            if let Some(t) = raw["tag"].as_str() {
                out.supports(Some(t.to_string()), &m.slots.supports);
            }
            data(out, GROUP, &raw["data"]);
        }
    }
}

fn tag_text(value: &Value, path: &str) -> Result<String, FormProblem> {
    let t = normalize_plain_text(&decode::<String>(value, path)?);
    if t.contains(TAG_FORBIDDEN) {
        return Err(FormProblem::new(
            path,
            "A tag can't contain , | & ! ( ) [ ] { } or $.",
        ));
    }
    Ok(t)
}

/// The class field's path in the same form (`fields.subclassTag` → `fields.parentClass`).
fn parent_path(path: &str) -> String {
    match path.rsplit_once("fields.") {
        Some((head, _)) => format!("{head}fields.{}", super::super::subclass::PARENT),
        None => path.to_string(),
    }
}

/// Write one field.
pub(crate) fn write(
    field: SubclassField,
    value: &Value,
    memo: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
    cx: &FormContext<'_>,
) -> Result<(), FormProblem> {
    if let Some(m) = memo.filter(|m| m.value == *value) {
        write_raw(field, m, out);
        return Ok(());
    }
    let raw = memo.map(|m| &m.raw);
    let slots = memo.map(|m| m.slots.supports.clone()).unwrap_or_default();
    match field {
        SubclassField::Parent => {
            let picked: Option<RefValue> = decode(value, path)?;
            let Some(class) = picked.filter(|c| !c.id.trim().is_empty()) else {
                return Ok(());
            };
            let had =
                raw.is_some_and(|r| stored(&r["class"]).is_some() || stored(&r["name"]).is_some());
            // a stored 2014 subclass without the 2024 keys keeps its shape
            if memo.is_some() && !had && out.element.ruleset != Some(Ruleset::Dnd2024) {
                return Ok(());
            }
            let id = class.id.trim().to_string();
            let name = cx.lookup.find(&id).map_or(class.name, |i| i.name);
            out.element.data.insert(CLASS.into(), json!(id));
            if !name.trim().is_empty() {
                out.element
                    .data
                    .insert(CLASS_NAME.into(), json!(name.trim()));
            }
        }
        SubclassField::Tag => {
            let t = tag_text(value, path)?;
            if t.is_empty() {
                return Err(FormProblem::new(
                    parent_path(path),
                    "Choose the class this subclass belongs to: its subclass choice offers it.",
                ));
            }
            out.supports(Some(t), &slots);
        }
        SubclassField::LevelGained => match value {
            Value::Null => {}
            v => {
                let n = v.as_u64().filter(|n| (1..=20).contains(n)).ok_or_else(|| {
                    FormProblem::new(path, "A subclass is chosen at a level from 1 to 20.")
                })?;
                out.element.data.insert(LEVEL_GAINED.into(), json!(n));
            }
        },
        SubclassField::FeatureLevels => {}
        SubclassField::OptionGroup => {
            let t = tag_text(value, path)?;
            if t.is_empty() {
                return Ok(());
            }
            out.supports(Some(t.clone()), &slots);
            if raw.is_some_and(|r| stored(&r["data"]).is_some()) {
                out.element.data.insert(GROUP.into(), json!(t));
            }
        }
    }
    Ok(())
}
