//! Does a stored value fit the control of its block field? The form model uses this to decide what a dialog edits and
//! what it keeps as is (spec 3.4): a field the catalog doesn't offer, or a value its control can't hold, goes to the
//! preserved bucket unchanged.

use super::{block, BlockField, BlockSpec, Control};
use borealis_content::rule_kinds::spec;
use borealis_content::rules::{GrantRule, StatRule};
use serde_json::{Map, Value};

const ABILITIES: [&str; 6] = [
    "strength",
    "dexterity",
    "constitution",
    "intelligence",
    "wisdom",
    "charisma",
];

/// The reader of these kinds shows every field verbatim (`notes_d` "fields shown verbatim"): a single value where the
/// block writes a list reads the same.
fn shown_verbatim(kind: &str) -> bool {
    spec(kind).is_some_and(|s| s.reader.contains("verbatim"))
}

fn int_in(v: &Value, min: i32, max: i32) -> bool {
    v.as_i64()
        .is_some_and(|n| (i64::from(min)..=i64::from(max)).contains(&n))
}

fn is_choice(choices: &[super::Choice], custom: bool, v: &str) -> bool {
    custom || choices.iter().any(|c| c.value == v)
}

/// One of the eighteen (`choices`), or the name of a skill loaded content adds (`skills`: their statistic keys).
fn is_skill(choices: &[super::Choice], skills: &[String], v: &str) -> bool {
    choices.iter().any(|c| c.value == v)
        || borealis_engine::content_skills::skill_key(v).is_some_and(|k| skills.contains(&k))
}

fn strings_fit(v: &Value, ok: impl Fn(&str) -> bool) -> bool {
    v.as_array()
        .is_some_and(|a| a.iter().all(|x| x.as_str().is_some_and(&ok)))
}

fn object_fits(fields: &[BlockField], v: &Value, skills: &[String]) -> bool {
    v.as_object().is_some_and(|m| {
        m.iter().all(|(k, x)| {
            fields
                .iter()
                .find(|f| f.key == k)
                .is_some_and(|f| value_fits_with(&f.control, x, skills))
        })
    })
}

/// A map key read as the key field's value (numbers are written as text keys: `{"6": 10}`).
fn key_fits(control: &Control, key: &str, skills: &[String]) -> bool {
    match control {
        Control::Number { .. } => key
            .parse::<i64>()
            .is_ok_and(|n| value_fits_with(control, &Value::from(n), skills)),
        _ => value_fits_with(control, &Value::from(key), skills),
    }
}

fn effect_fits(kinds: &[&str], v: &Value, skills: &[String]) -> bool {
    let Some(m) = v.as_object() else {
        return false;
    };
    let Some(kind) = m.get("kind").and_then(Value::as_str) else {
        return false;
    };
    if !kinds.contains(&kind) {
        return false;
    }
    let mut rest = m.clone();
    rest.remove("kind");
    match kind {
        "grant" => serde_json::from_value::<GrantRule>(Value::Object(rest)).is_ok(),
        "stat" => serde_json::from_value::<StatRule>(Value::Object(rest)).is_ok(),
        _ => block(kind).is_some_and(|b| misfits_with(b, &rest, skills).is_empty()),
    }
}

/// Does `v` fit `control`? (Shape only: required fields are the validator's.) No skill content adds is loaded:
/// [`value_fits_with`].
#[must_use]
pub fn value_fits(control: &Control, v: &Value) -> bool {
    value_fits_with(control, v, &[])
}

/// [`value_fits`] with the skills loaded content adds (`skills`: their statistic keys, `alchemy`), which a
/// [`Control::Skills`] field also takes by name (HC16).
#[must_use]
pub fn value_fits_with(control: &Control, v: &Value, skills: &[String]) -> bool {
    match control {
        Control::Auto => !v.is_null(),
        Control::Text
        | Control::Multiline
        | Control::Dice
        | Control::ClassCounter
        | Control::Statistic
        | Control::StatValue
        | Control::Condition
        | Control::TagFilter
        | Control::Equipped
        | Control::Reference { .. }
        | Control::Name {
            multiple: false, ..
        } => v.is_string(),
        Control::Name { multiple: true, .. } | Control::References { .. } => {
            strings_fit(v, |_| true)
        }
        Control::Formula(_) => v.is_string() || v.is_i64(),
        Control::Formulas(_) => strings_fit(v, |_| true),
        Control::Number { min, max } => int_in(v, *min, *max),
        Control::NumberOr {
            min,
            max,
            words,
            custom,
        } => int_in(v, *min, *max) || v.as_str().is_some_and(|s| is_choice(words, *custom, s)),
        Control::Numbers { min, max } => v
            .as_array()
            .is_some_and(|a| a.iter().all(|x| int_in(x, *min, *max))),
        Control::Check => v.is_boolean(),
        Control::Combo { choices, custom } => {
            v.as_str().is_some_and(|s| is_choice(choices, *custom, s))
        }
        Control::Chips { choices, custom } => strings_fit(v, |s| is_choice(choices, *custom, s)),
        Control::Skills {
            choices,
            multiple: false,
        } => v.as_str().is_some_and(|s| is_skill(choices, skills, s)),
        Control::Skills {
            choices,
            multiple: true,
        } => strings_fit(v, |s| is_skill(choices, skills, s)),
        Control::NumberOrStatistic => {
            v.is_i64()
                || v.as_object().is_some_and(|m| {
                    m.get("type").and_then(Value::as_str) == Some("stat")
                        && m.get("stat").is_some_and(Value::is_string)
                        && m.len() == 2
                })
        }
        Control::Uses => {
            v.is_i64()
                || v.as_str()
                    .is_some_and(|s| s == "proficiency" || ABILITIES.contains(&s))
                || v.as_object().is_some_and(|m| {
                    m.get("type").and_then(Value::as_str) == Some("table")
                        && m.get("key").is_some_and(Value::is_string)
                        && m.len() == 2
                })
        }
        // `{param}`, maybe with the abilities that choice offers (`choice`, which the engine reads only without a
        // `param`): the 2024 SRD lineages store both
        Control::AbilitySource => {
            v.as_str().is_some_and(|s| ABILITIES.contains(&s))
                || v.as_object().is_some_and(|m| {
                    m.get("param").is_some_and(Value::is_string)
                        && m.iter().all(|(k, x)| match k.as_str() {
                            "param" => true,
                            "choice" => strings_fit(x, |s| ABILITIES.contains(&s)),
                            _ => false,
                        })
                })
        }
        Control::AbilityChoice => int_in(v, 1, 6) || strings_fit(v, |s| ABILITIES.contains(&s)),
        Control::Splits => v.as_array().is_some_and(|a| {
            a.iter().all(|s| {
                s.as_array()
                    .is_some_and(|p| !p.is_empty() && p.iter().all(|n| int_in(n, 1, 10)))
            })
        }),
        Control::Group(fields) => object_fits(fields, v, skills),
        Control::Rows(fields) => v
            .as_array()
            .is_some_and(|a| a.iter().all(|x| object_fits(fields, x, skills))),
        Control::Variant(variants) => v.as_object().is_some_and(|m| {
            let ty = m.get("type").and_then(Value::as_str);
            variants
                .iter()
                .find(|x| Some(x.value) == ty)
                .is_some_and(|x| {
                    m.iter()
                        .filter(|(k, _)| k.as_str() != "type")
                        .all(|(k, val)| {
                            x.fields
                                .iter()
                                .find(|f| f.key == k)
                                .is_some_and(|f| value_fits_with(&f.control, val, skills))
                        })
                })
        }),
        Control::Map([key, value]) => v.as_object().is_some_and(|m| {
            m.iter().all(|(k, x)| {
                key_fits(&key.control, k, skills) && value_fits_with(&value.control, x, skills)
            })
        }),
        Control::Blocks(kinds) => v
            .as_array()
            .is_some_and(|a| a.iter().all(|x| effect_fits(kinds, x, skills))),
    }
}

/// The fields of a rule (the custom rule's fields, or a core rule's JSON without `kind`) its block doesn't edit:
/// fields the block doesn't offer, and values their control can't hold. Empty = the dialog edits the whole rule. No
/// skill content adds is loaded: [`misfits_with`].
#[must_use]
pub fn misfits(block: &BlockSpec, fields: &Map<String, Value>) -> Vec<String> {
    misfits_with(block, fields, &[])
}

/// [`misfits`] with the skills loaded content adds (`skills`: their statistic keys), see [`value_fits_with`].
#[must_use]
pub fn misfits_with(
    block: &BlockSpec,
    fields: &Map<String, Value>,
    skills: &[String],
) -> Vec<String> {
    let verbatim = shown_verbatim(block.kind);
    fields
        .iter()
        .filter(|(k, v)| {
            let ok = match k.as_str() {
                "level" if block.level_gated => int_in(v, 1, 20),
                "requirements" if block.only_if => v.is_string(),
                _ => block.field(k).is_some_and(|f| {
                    value_fits_with(&f.control, v, skills)
                        || (verbatim
                            && v.is_string()
                            && matches!(f.control, Control::Chips { .. } | Control::Numbers { .. })
                            && value_fits_with(
                                &f.control,
                                &Value::Array(vec![(*v).clone()]),
                                skills,
                            ))
                }),
            };
            !ok
        })
        .map(|(k, _)| k.clone())
        .collect()
}

/// [`misfits`] of a rule against its block; `None` when the catalog has no block for it (a `Rule::Unknown` or an
/// unknown custom kind: the whole rule is kept as is).
#[must_use]
pub fn fits(rule: &borealis_content::rules::Rule) -> Option<Vec<String>> {
    fits_with(rule, &[])
}

/// [`fits`] with the skills loaded content adds (`skills`: their statistic keys), see [`value_fits_with`].
#[must_use]
pub fn fits_with(rule: &borealis_content::rules::Rule, skills: &[String]) -> Option<Vec<String>> {
    let kind = super::kind_of(rule)?;
    let b = block(kind)?;
    let Ok(Value::Object(mut m)) = serde_json::to_value(rule) else {
        return None;
    };
    m.remove("kind");
    Some(misfits_with(b, &m, skills))
}
