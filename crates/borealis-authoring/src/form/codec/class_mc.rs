//! The class form's MULTICLASS sub-tab (spec 4.5): the `multiclass` section the engine turns into the class's
//! Multiclass element.
//!
//! * **Can be taken as a second class**: the section exists. It holds what no other field edits (other setters, the
//!   2024 Bard's skill choice, unknown parts) byte for byte; its id is the generated `ID_<CODE>_MULTICLASS_<CLASS>`
//!   ([`super::super::Memo::multiclass_id`], never the class id, `MulticlassIdSameAsClass`).
//! * **Prerequisite**: the condition builder over ability minimums → `multiclass.requirements`
//!   (+ `data.multiclass.requirements`); **as shown**: `multiclass.prerequisite` (+ `data.multiclass.prerequisite`),
//!   written from the condition while left empty or as written ("Strength 13 or Dexterity 13").
//! * **Proficiencies**: `grant`s in `multiclass.rules`, after the `ID_INTERNAL_GRANT_MULTICLASS` grant the engine
//!   needs (always there); **as shown**: the setter `multiclass proficiencies`, written from them likewise.

use super::super::value::RefValue;
use super::super::write::Out;
use super::super::{FieldMemo, FormContext, FormProblem};
use super::class::{ability_word, ClassField, Raw, Take};
use super::{decode, to_json};
use crate::expr_build::condition::{Condition, ConditionLanguage, ConditionRow};
use crate::expr_build::{Built, ExprValue, GroupMode, Node};
use borealis_content::rich::normalize_plain_text;
use borealis_content::{GrantRule, Multiclass, Rule, Setter};
use serde_json::{json, Value};

/// The grant every multiclass section starts with (the engine's multiclass marker).
const INTERNAL: &str = "ID_INTERNAL_GRANT_MULTICLASS";
const INTERNAL_TYPE: &str = "Grants";
const TEXT_SETTER: &str = "multiclass proficiencies";
const MIRROR: &str = "multiclass";

fn is_internal(rule: &Rule) -> bool {
    matches!(rule, Rule::Grant(g) if g.id == INTERNAL)
}

fn proficiency(rule: &Rule) -> Option<&str> {
    let Rule::Grant(g) = rule else { return None };
    let plain = g.ty.as_deref() == Some("Proficiency")
        && g.requirements.is_none()
        && g.level.is_none()
        && g.spellcasting.is_none()
        && g.prepared.is_none()
        && g.setters.is_empty()
        && g.extra.is_empty();
    plain.then_some(g.id.as_str())
}

/// The sentence a multiclass prerequisite of ability minimums reads as ("Strength 13 or Dexterity 13"); `None` for
/// a condition of other rows.
pub(crate) fn sentence(value: &ExprValue<Condition>) -> Option<String> {
    let Built::Model { model } = &value.value else {
        return None;
    };
    let row = |n: &Node<ConditionRow>| match n {
        Node::Row {
            row: ConditionRow::Ability { ability, min },
        } => Some(format!("{} {min}", ability_word(*ability))),
        _ => None,
    };
    match model {
        Node::Group { items, .. } if items.is_empty() => Some(String::new()),
        Node::Group { mode, items } => {
            let parts: Option<Vec<String>> = items.iter().map(row).collect();
            let join = match mode {
                GroupMode::All => " and ",
                GroupMode::Any => " or ",
                GroupMode::None => return None,
            };
            parts.map(|p| p.join(join))
        }
        one => row(one),
    }
}

/// A proficiency's name as the multiclass line says it: "Armor Proficiency (Light Armor)" is "Light Armor".
fn short_name(name: String) -> String {
    match name.split_once(" Proficiency (") {
        Some((_, rest)) if rest.ends_with(')') => rest.trim_end_matches(')').to_string(),
        _ => name,
    }
}

fn names(ids: &[&str], cx: &FormContext<'_>) -> String {
    ids.iter()
        .filter_map(|id| cx.lookup.find(id).map(|i| short_name(i.name)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Read one multiclass field.
pub(super) fn read(field: ClassField, mut t: Take<'_, '_>, cx: &FormContext<'_>) -> FieldMemo {
    let e = t.r.e;
    let node = e.multiclass.as_ref();
    let condition = |m: &Multiclass| {
        ExprValue::<Condition>::read::<ConditionLanguage>(&m.requirements, &cx.conditions)
    };
    let value = match field {
        ClassField::Multiclass => {
            t.r.parts_taken.insert("multiclass");
            t.raw.part = node.map(to_json);
            json!(node.is_some())
        }
        ClassField::McRequirements => {
            if let Some(d) = t.r.take_data(MIRROR) {
                t.raw.data.insert(MIRROR.into(), d.clone());
            }
            node.map_or(Value::Null, |m| to_json(&condition(m)))
        }
        ClassField::McPrerequisite => {
            let text = node.map(|m| m.prerequisite.clone()).unwrap_or_default();
            let generated = node
                .and_then(|m| sentence(&condition(m)))
                .is_some_and(|s| s == text);
            t.raw.part = Some(json!({ "generated": generated }));
            json!(text)
        }
        ClassField::McProficiencies => {
            let refs: Vec<RefValue> = node
                .into_iter()
                .flat_map(|m| m.rules.iter())
                .filter_map(proficiency)
                .map(|id| RefValue::resolve(id, cx.lookup))
                .collect();
            to_json(&refs)
        }
        ClassField::McProficiencyText => {
            let text = node
                .and_then(|m| {
                    m.setters
                        .iter()
                        .find(|s| s.name.eq_ignore_ascii_case(TEXT_SETTER))
                })
                .map(|s| s.value.clone())
                .unwrap_or_default();
            let ids: Vec<&str> = node
                .into_iter()
                .flat_map(|m| m.rules.iter())
                .filter_map(proficiency)
                .collect();
            t.raw.part = Some(json!({ "generated": !text.is_empty() && names(&ids, cx) == text }));
            json!(text)
        }
        _ => unreachable!("class fields are read in class.rs"),
    };
    t.done(value)
}

fn generated(raw: &Raw) -> bool {
    raw.part
        .as_ref()
        .and_then(|p| p.get("generated"))
        .and_then(Value::as_bool)
        == Some(true)
}

/// Write one multiclass field (fields write in form order: the section first).
pub(super) fn write(
    field: ClassField,
    value: &Value,
    memo: Option<&FieldMemo>,
    unchanged: bool,
    path: &str,
    out: &mut Out,
    cx: &FormContext<'_>,
) -> Result<(), FormProblem> {
    let raw = Raw::of(memo);
    if field == ClassField::Multiclass {
        let on: bool = decode(value, path)?;
        if !on {
            out.element.multiclass = None;
            return Ok(());
        }
        let Some(id) = out.multiclass_id.clone() else {
            return Err(FormProblem::new("id", "This element has no id yet."));
        };
        let mut node: Multiclass = raw
            .part
            .as_ref()
            .and_then(|p| serde_json::from_value(p.clone()).ok())
            .unwrap_or_else(|| Multiclass {
                id: id.clone(),
                prerequisite: String::new(),
                requirements: String::new(),
                description: String::new(),
                setters: Vec::new(),
                rules: Vec::new(),
                extra: Default::default(),
            });
        node.id = id;
        out.element.multiclass = Some(node);
        return Ok(());
    }
    if field == ClassField::McRequirements {
        // the 2024 mirror goes back (or is rewritten) whether or not the section is on
        if let Some(d) = raw.data.get(MIRROR) {
            out.element.data.insert(MIRROR.into(), d.clone());
        }
    }
    let Some(mut node) = out.element.multiclass.take() else {
        return Ok(());
    };
    let result = write_part(field, value, &raw, unchanged, path, &mut node, out, cx);
    out.element.multiclass = Some(node);
    result
}

fn set_mirror(out: &mut Out, key: &str, text: &str) {
    if let Some(Value::Object(m)) = out.element.data.get_mut(MIRROR) {
        m.insert(key.into(), json!(text));
    }
}

#[allow(clippy::too_many_arguments)]
fn write_part(
    field: ClassField,
    value: &Value,
    raw: &Raw,
    unchanged: bool,
    path: &str,
    node: &mut Multiclass,
    out: &mut Out,
    cx: &FormContext<'_>,
) -> Result<(), FormProblem> {
    match field {
        ClassField::McRequirements => {
            if unchanged {
                return Ok(());
            }
            let text = if value.is_null() {
                String::new()
            } else {
                decode::<ExprValue<Condition>>(value, path)?
                    .text::<ConditionLanguage>(&cx.conditions)
            };
            node.requirements = text.clone();
            set_mirror(out, "requirements", &text);
        }
        ClassField::McPrerequisite => {
            let typed = normalize_plain_text(&decode::<String>(value, path)?);
            let regenerate = if unchanged {
                generated(raw)
            } else {
                typed.is_empty()
            };
            if unchanged && !regenerate {
                return Ok(());
            }
            let text = if regenerate {
                let condition = ExprValue::<Condition>::read::<ConditionLanguage>(
                    &node.requirements,
                    &cx.conditions,
                );
                sentence(&condition).unwrap_or(typed)
            } else {
                typed
            };
            if node.prerequisite != text {
                node.prerequisite = text.clone();
                set_mirror(out, "prerequisite", &text);
            }
        }
        ClassField::McProficiencies => {
            let has_internal = node.rules.iter().any(is_internal);
            if unchanged && (has_internal || !node.rules.is_empty()) {
                return Ok(());
            }
            let refs: Vec<RefValue> = decode(value, path)?;
            let others: Vec<Rule> = node
                .rules
                .iter()
                .filter(|r| !is_internal(r) && proficiency(r).is_none())
                .cloned()
                .collect();
            let internal = node
                .rules
                .iter()
                .find(|r| is_internal(r))
                .cloned()
                .unwrap_or_else(|| {
                    Rule::Grant(GrantRule {
                        id: INTERNAL.into(),
                        ty: Some(INTERNAL_TYPE.into()),
                        ..GrantRule::default()
                    })
                });
            node.rules = std::iter::once(internal)
                .chain(refs.iter().filter(|r| !r.id.trim().is_empty()).map(|r| {
                    Rule::Grant(GrantRule {
                        id: r.id.trim().to_string(),
                        ty: Some("Proficiency".into()),
                        ..GrantRule::default()
                    })
                }))
                .chain(others)
                .collect();
        }
        ClassField::McProficiencyText => {
            let typed = normalize_plain_text(&decode::<String>(value, path)?);
            let regenerate = if unchanged {
                generated(raw)
            } else {
                typed.is_empty()
            };
            if unchanged && !regenerate {
                return Ok(());
            }
            let text = if regenerate {
                let ids: Vec<&str> = node.rules.iter().filter_map(proficiency).collect();
                names(&ids, cx)
            } else {
                typed
            };
            let at = node
                .setters
                .iter()
                .position(|s| s.name.eq_ignore_ascii_case(TEXT_SETTER));
            match (at, text.is_empty()) {
                (Some(i), true) => {
                    node.setters.remove(i);
                }
                (Some(i), false) => node.setters[i].value = text,
                (None, false) => node.setters.push(Setter::new(TEXT_SETTER, text)),
                (None, true) => {}
            }
        }
        _ => {}
    }
    Ok(())
}
