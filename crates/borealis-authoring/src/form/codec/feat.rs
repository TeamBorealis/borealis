//! The feat form's codecs (spec 4.2), for both shapes a feat is stored in:
//!
//! * **2014** (Aurora): `requirements` + the display `prerequisite`, the setter `allow duplicate`.
//! * **2024** (SRD 5.2.1): the setter `category` with the support tags `"<Category> Feat"` and `"Feat"` (what a 2024
//!   feat choice offers by), the same `requirements`/`prerequisite`, and the `data` keys `category` (mirror),
//!   `prerequisite` and `repeatable` (`allowed`, `distinct_param`).
//!
//! Each field binds every location that holds it (spec 3.3). The category tag and mirror are written only when the
//! element has a category; mirrors only when the element already had them. Repeatable is engine data
//! (`data.repeatable.allowed` gives each take its own choices), so a new feat that can be taken again gets it; a feat
//! stored with the `allow duplicate` setter keeps that setter.

use super::super::write::Out;
use super::super::{FieldMemo, FormContext, FormProblem, Slots};
use super::feat_prereq as prereq;
use super::{
    decode, decode_raw, edited_setter, memo, plain, present, raw_setters, stored, to_json, Reader,
};
use crate::expr_build::condition::{Condition, ConditionLanguage};
use crate::expr_build::{Built, ExprValue};
use borealis_content::rich::normalize_plain_text;
use serde_json::{json, Map, Value};

const CATEGORY: &str = "category";
const PREREQUISITE: &str = "prerequisite";
const REPEATABLE: &str = "repeatable";
const ALLOWED: &str = "allowed";
const DISTINCT: &str = "distinct_param";
const ALLOW_DUPLICATE: &str = "allow duplicate";
/// The tag every 2024 feat carries besides its category's.
const FEAT_TAG: &str = "Feat";
/// The 2024 categories: a `"<Category> Feat"` tag of one of them is the category of a feat with no `category` setter.
const KNOWN_CATEGORIES: [&str; 4] = ["Origin", "General", "Fighting Style", "Epic Boon"];
/// Characters a tag can't hold (IM15): a category is also a tag (`"<Category> Feat"`).
const TAG_FORBIDDEN: &[char] = &[',', '|', '&', '!', '(', ')', '[', ']', '{', '}', '$'];

/// A feat field (`Codec::Feat`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatField {
    /// Setter `category`, the tags `"<Category> Feat"` and `"Feat"`, `data.category`.
    Category,
    /// The condition: `requirements` + `data.prerequisite`.
    Prerequisite,
    /// The display `prerequisite`, written from the condition until reworded.
    Shown,
    /// Setter `allow duplicate` + `data.repeatable.allowed`.
    Repeatable,
    /// `data.repeatable.distinct_param`: the choice that must differ each time.
    RepeatDistinct,
}

impl FeatField {
    /// The model locations, as the generated-data view shows them.
    pub(crate) fn bindings(self) -> Vec<String> {
        let v: &[&str] = match self {
            Self::Category => &[
                "setter:category",
                "supports:<Category> Feat|Feat",
                "data.category",
            ],
            Self::Prerequisite => &["requirements", "data.prerequisite"],
            Self::Shown => &["prerequisite"],
            Self::Repeatable => &["setter:allow duplicate", "data.repeatable.allowed"],
            Self::RepeatDistinct => &["data.repeatable.distinct_param"],
        };
        v.iter().map(|s| (*s).to_string()).collect()
    }

    /// The setter names it takes, lower case.
    pub(crate) fn setter_names(self) -> Vec<&'static str> {
        match self {
            Self::Category => vec![CATEGORY],
            Self::Repeatable => vec![ALLOW_DUPLICATE],
            _ => Vec::new(),
        }
    }

    /// The `data` keys it owns (`data.repeatable` belongs to Repeatable; the distinct choice is a part of it).
    pub(crate) fn data_keys(self) -> Vec<&'static str> {
        match self {
            Self::Category => vec![CATEGORY],
            Self::Prerequisite => vec![PREREQUISITE],
            Self::Repeatable => vec![REPEATABLE],
            Self::Shown | Self::RepeatDistinct => Vec::new(),
        }
    }
}

fn setter_slots(i: Option<usize>) -> Slots {
    Slots {
        setters: i.into_iter().collect(),
        ..Slots::default()
    }
}

/// `spell_list` as the form shows it ("Spell List").
fn param_words(key: &str) -> String {
    key.split(['_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// "Spell List" as the engine keys a choice (`custom_rules::param_key`).
fn param_key(words: &str) -> String {
    words.trim().to_lowercase().replace([' ', '-'], "_")
}

/// Read one feat field.
pub(crate) fn read(field: FeatField, r: &mut Reader<'_>, cx: &FormContext<'_>) -> FieldMemo {
    let e = r.e;
    match field {
        FeatField::Category => read_category(r),
        FeatField::Prerequisite => {
            let data = r.take_data(PREREQUISITE);
            let value = prereq::condition(&e.requirements, data, &cx.conditions);
            plain(
                to_json(&value),
                json!({ "text": e.requirements, "data": present(data) }),
            )
        }
        FeatField::Shown => {
            let value =
                prereq::condition(&e.requirements, e.data.get(PREREQUISITE), &cx.conditions);
            let generated = prereq::sentence(&value, cx.lookup).unwrap_or_default();
            plain(
                json!(e.prerequisite),
                json!({ "text": e.prerequisite, "generated": e.prerequisite == generated }),
            )
        }
        FeatField::Repeatable => {
            let setter = r.take_setter(ALLOW_DUPLICATE);
            let data = r.take_data(REPEATABLE);
            let on = setter.and_then(|(_, s)| s.as_bool()) == Some(true)
                || data.and_then(|d| d.get(ALLOWED)).and_then(Value::as_bool) == Some(true);
            memo(
                json!(on),
                json!({
                    "setter": setter.map_or(Value::Null, |(_, s)| to_json(s)),
                    "data": present(data),
                }),
                setter_slots(setter.map(|(i, _)| i)),
            )
        }
        FeatField::RepeatDistinct => {
            let part = e.data.get(REPEATABLE).and_then(|d| d.get(DISTINCT));
            let shown = part.and_then(Value::as_str).map(param_words);
            plain(json!(shown.unwrap_or_default()), present(part))
        }
    }
}

fn read_category(r: &mut Reader<'_>) -> FieldMemo {
    let e = r.e;
    let setter = r.take_setter(CATEGORY);
    let data = r.take_data(CATEGORY);
    // a tag names the category of a 2024 feat (one of the four, or any with the 2024 mirror, as imports write it);
    // other "... Feat" tags of a 2014 feat are plain tags
    let from_tag = || {
        e.supports.iter().find_map(|t| {
            t.strip_suffix(" Feat")
                .map(str::trim)
                .filter(|c| {
                    !c.is_empty()
                        && (data.is_some()
                            || KNOWN_CATEGORIES.iter().any(|k| k.eq_ignore_ascii_case(c)))
                })
                .map(str::to_string)
        })
    };
    let value = setter
        .map(|(_, s)| s.value.trim().to_string())
        .or_else(from_tag)
        .or_else(|| data.and_then(Value::as_str).map(|s| s.trim().to_string()))
        .unwrap_or_default();
    let (tags, supports) = if value.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        let own = format!("{value} {FEAT_TAG}");
        r.take_tags(|t| t.eq_ignore_ascii_case(&own) || t == FEAT_TAG)
    };
    memo(
        json!(value),
        json!({
            "setter": setter.map_or(Value::Null, |(_, s)| to_json(s)),
            "tags": tags,
            "data": present(data),
        }),
        Slots {
            setters: setter.map(|(i, _)| i).into_iter().collect(),
            supports,
            ..Slots::default()
        },
    )
}

/// The condition the element being written holds now (its `requirements` and `data.prerequisite`).
fn written_condition(out: &Out, cx: &FormContext<'_>) -> ExprValue<Condition> {
    prereq::condition(
        &out.element.requirements,
        out.element.data.get(PREREQUISITE),
        &cx.conditions,
    )
}

/// Write a feat field. Unchanged fields write their stored bytes, except the display sentence of a condition, which
/// follows the condition while it was the generated one.
pub(crate) fn write(
    field: FeatField,
    value: &Value,
    memo: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
    cx: &FormContext<'_>,
) -> Result<(), FormProblem> {
    let unchanged = memo.filter(|m| m.value == *value);
    let raw = memo.map(|m| &m.raw);
    match (field, unchanged) {
        (FeatField::Shown, _) => write_shown(value, unchanged, path, out, cx),
        (_, Some(m)) => {
            write_raw(field, m, out);
            Ok(())
        }
        (FeatField::Category, None) => {
            let slots = memo.map(|m| m.slots.clone()).unwrap_or_default();
            write_category(value, raw, &slots, path, out)
        }
        (FeatField::Prerequisite, None) => {
            let v: ExprValue<Condition> = decode(value, path)?;
            out.element.requirements = v.text::<ConditionLanguage>(&cx.conditions);
            if let Some(d) = raw.and_then(|r| stored(&r["data"])) {
                let model = match &v.value {
                    Built::Model { model } => model.clone(),
                    Built::Kept { .. } => crate::expr_build::Node::empty(),
                };
                out.element
                    .data
                    .insert(PREREQUISITE.into(), prereq::mirror(d, &model));
            }
            Ok(())
        }
        (FeatField::Repeatable, None) => {
            let slots = memo.map(|m| m.slots.clone()).unwrap_or_default();
            write_repeatable(value, raw, &slots, path, out)
        }
        (FeatField::RepeatDistinct, None) => write_distinct(value, path, out),
    }
}

fn write_shown(
    value: &Value,
    unchanged: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
    cx: &FormContext<'_>,
) -> Result<(), FormProblem> {
    let now = || prereq::sentence(&written_condition(out, cx), cx.lookup).unwrap_or_default();
    out.element.prerequisite = match unchanged {
        Some(m) if m.raw["generated"] == json!(true) => now(),
        Some(m) => m.raw["text"].as_str().unwrap_or_default().to_string(),
        None => {
            let text: String = decode(value, path)?;
            let text = normalize_plain_text(&text);
            if text.is_empty() {
                now()
            } else {
                text
            }
        }
    };
    Ok(())
}

fn write_category(
    value: &Value,
    raw: Option<&Value>,
    slots: &Slots,
    path: &str,
    out: &mut Out,
) -> Result<(), FormProblem> {
    let text: String = decode(value, path)?;
    let category = normalize_plain_text(&text);
    if category.contains(TAG_FORBIDDEN) {
        return Err(FormProblem::new(
            path,
            "A category can't contain , | & ! ( ) [ ] { } or $.",
        ));
    }
    if category.is_empty() {
        return Ok(());
    }
    let stored_setter = raw.map(|r| &r["setter"]).filter(|s| !s.is_null());
    out.setters(
        Some(edited_setter(CATEGORY, category.clone(), stored_setter)),
        &slots.setters,
    );
    out.supports(
        [format!("{category} {FEAT_TAG}"), FEAT_TAG.to_string()],
        &slots.supports,
    );
    if raw.and_then(|r| stored(&r["data"])).is_some() {
        out.element.data.insert(CATEGORY.into(), json!(category));
    }
    Ok(())
}

/// The stored `data.repeatable` keys Repeatable owns (all but the distinct choice).
fn owned(stored: &Value) -> Map<String, Value> {
    stored
        .as_object()
        .map(|o| {
            o.iter()
                .filter(|(k, _)| k.as_str() != DISTINCT)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn write_repeatable(
    value: &Value,
    raw: Option<&Value>,
    slots: &Slots,
    path: &str,
    out: &mut Out,
) -> Result<(), FormProblem> {
    let on: bool = decode(value, path)?;
    let stored_setter = raw.map(|r| &r["setter"]).filter(|s| !s.is_null());
    let data = raw.and_then(|r| stored(&r["data"]));
    if stored_setter.is_some() {
        let s = on.then(|| edited_setter(ALLOW_DUPLICATE, "true".into(), stored_setter));
        out.setters(s, &slots.setters);
    }
    match data {
        Some(d) => {
            let mut entries = owned(d);
            entries.insert(ALLOWED.into(), json!(on));
            out.element
                .data
                .insert(REPEATABLE.into(), Value::Object(entries));
        }
        // a feat stored with the setter keeps its shape; a new one gets the engine's repeatable data
        None if on && stored_setter.is_none() => {
            out.element
                .data
                .insert(REPEATABLE.into(), json!({ ALLOWED: true }));
        }
        None => {}
    }
    Ok(())
}

fn write_distinct(value: &Value, path: &str, out: &mut Out) -> Result<(), FormProblem> {
    let text: String = decode(value, path)?;
    let key = param_key(&text);
    if key.is_empty() {
        if let Some(Value::Object(o)) = out.element.data.get_mut(REPEATABLE) {
            o.remove(DISTINCT);
        }
        return Ok(());
    }
    match out.element.data.get_mut(REPEATABLE) {
        Some(Value::Object(o)) if o.get(ALLOWED) == Some(&json!(true)) => {
            o.insert(DISTINCT.into(), json!(key));
            Ok(())
        }
        _ => Err(FormProblem::new(
            path,
            "This only applies to a feat that can be taken more than once.",
        )),
    }
}

/// Put the stored bytes of an unchanged feat field back (the display sentence is [`write_shown`]'s).
fn write_raw(field: FeatField, m: &FieldMemo, out: &mut Out) {
    let raw = &m.raw;
    match field {
        FeatField::Category => {
            out.setters(raw_setters(&raw["setter"]), &m.slots.setters);
            let tags: Vec<String> = decode_raw(&raw["tags"]).unwrap_or_default();
            out.supports(tags, &m.slots.supports);
            if let Some(v) = stored(&raw["data"]) {
                out.element.data.insert(CATEGORY.into(), v.clone());
            }
        }
        FeatField::Prerequisite => {
            out.element.requirements = raw["text"].as_str().unwrap_or_default().to_string();
            if let Some(v) = stored(&raw["data"]) {
                out.element.data.insert(PREREQUISITE.into(), v.clone());
            }
        }
        FeatField::Repeatable => {
            out.setters(raw_setters(&raw["setter"]), &m.slots.setters);
            if let Some(v) = stored(&raw["data"]) {
                out.element.data.insert(REPEATABLE.into(), v.clone());
            }
        }
        // back in the object when Repeatable was edited (an edited Repeatable writes its own keys only)
        FeatField::RepeatDistinct => {
            if let (Some(v), Some(Value::Object(o))) =
                (stored(raw), out.element.data.get_mut(REPEATABLE))
            {
                o.entry(DISTINCT).or_insert_with(|| v.clone());
            }
        }
        FeatField::Shown => {}
    }
}

#[cfg(test)]
#[path = "feat_tests.rs"]
mod tests;
