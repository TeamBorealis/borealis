//! Field paths for problems (spec 8.3): a validator location (`requirements`, `rules[2]`, `setters[0]`, `data.tag`,
//! `setter:school` for a missing required setter) to the form path of the field that edits it (`fields.requirements`,
//! `blocks.2`, `fields.script`, `preserved.data.tag`).

use super::codec::{ClassField, Codec};
use super::kinds::{kind, Control};
use super::{FormContext, FormProblem, FormState};
use crate::expr_build::condition::{Condition, ConditionLanguage};
use crate::expr_build::tag_filter::{TagFilter, TagFilterLanguage};
use crate::expr_build::ExprValue;
use borealis_content::Element;

fn field_with(form: &FormState, want: impl Fn(Codec) -> bool) -> Option<String> {
    kind(&form.kind)?
        .fields
        .iter()
        .find(|f| want(f.codec))
        .map(|f| format!("fields.{}", f.key))
}

fn preserved(form: &FormState, key: &str) -> Option<String> {
    form.preserved
        .iter()
        .any(|p| p.key == key)
        .then(|| format!("preserved.{key}"))
}

/// The field a setter called `name` belongs to.
fn setter_field(form: &FormState, name: &str) -> Option<String> {
    let lower = name.to_lowercase();
    field_with(form, |c| c.setter_names().contains(&lower.as_str()))
        .or_else(|| field_with(form, |c| c == Codec::OtherSetters))
}

fn index(text: &str, prefix: &str) -> Option<usize> {
    text.strip_prefix(prefix)?.strip_suffix(']')?.parse().ok()
}

/// The block (`blocks.<i>`) or field (`fields.<key>`) that wrote the element's `rule`-th rule.
fn origin_of_rule(form: &FormState, rule: usize, cx: &FormContext<'_>) -> Option<String> {
    super::write::rule_origins(form, cx)?.into_iter().nth(rule)
}

/// The form path of a location in `element`, the element `form` writes; `None` when no field or kept part holds it.
#[must_use]
pub fn form_path_for_location(
    form: &FormState,
    element: &Element,
    location: &str,
    cx: &FormContext<'_>,
) -> Option<String> {
    let top = location.split(['.', '[']).next().unwrap_or(location);
    if let Some(i) = index(location, "rules[") {
        return origin_of_rule(form, i, cx);
    }
    if let Some(i) = index(location, "setters[") {
        let s = element.setters.get(i)?;
        return setter_field(form, &s.name);
    }
    if let Some(name) = location.strip_prefix("setter:") {
        return setter_field(form, name);
    }
    if let Some(key) = location.strip_prefix("data.") {
        let key = key.split(['.', '[']).next().unwrap_or(key);
        return field_with(form, |c| c.data_keys().contains(&key))
            .or_else(|| preserved(form, &format!("data.{key}")));
    }
    let codec = match top {
        "name" => Codec::Name,
        "type" => Codec::Type,
        "source" => Codec::Source,
        "ruleset" => Codec::Edition,
        "supports" => {
            return field_with(form, Codec::edits_supports).or_else(|| preserved(form, top))
        }
        "requirements" => {
            return field_with(form, Codec::is_condition).or_else(|| preserved(form, top))
        }
        "prerequisite" => {
            return field_with(form, Codec::is_prerequisite_text).or_else(|| preserved(form, top))
        }
        "description" => Codec::Description,
        "sheet" => Codec::Sheet,
        "compendium" => Codec::Compendium,
        "maps_to" => Codec::MapsTo,
        "multiclass" => {
            let part = location
                .strip_prefix("multiclass")
                .and_then(|l| l.trim_start_matches('.').split(['.', '[']).next())
                .unwrap_or_default();
            let f = match part {
                "requirements" => ClassField::McRequirements,
                "prerequisite" => ClassField::McPrerequisite,
                "rules" => ClassField::McProficiencies,
                "setters" => ClassField::McProficiencyText,
                _ => ClassField::Multiclass,
            };
            return field_with(form, |c| c == Codec::Class(f))
                .or_else(|| field_with(form, |c| c == Codec::Class(ClassField::Multiclass)))
                .or_else(|| preserved(form, top));
        }
        other => return preserved(form, other),
    };
    field_with(form, |c| c == codec).or_else(|| preserved(form, top))
}

/// The form path of a problem found on the element `element_id` of the draft (`root` and its children), at
/// `location` (`None`: the element itself, so the path of that element's form, `""` for the root).
#[must_use]
pub fn form_path_for_issue(
    root: &FormState,
    elements: &[Element],
    element_id: &str,
    location: Option<&str>,
    cx: &FormContext<'_>,
) -> Option<String> {
    let (prefix, form) = find(root, element_id, String::new())?;
    let element = elements.iter().find(|e| e.id == element_id)?;
    match location {
        None => Some(prefix.trim_end_matches('.').to_string()),
        Some(loc) => form_path_for_location(form, element, loc, cx).map(|p| format!("{prefix}{p}")),
    }
}

fn find<'f>(form: &'f FormState, id: &str, prefix: String) -> Option<(String, &'f FormState)> {
    if form.id() == Some(id) {
        return Some((prefix, form));
    }
    form.children
        .iter()
        .enumerate()
        .find_map(|(i, c)| find(c, id, format!("{prefix}children.{i}.")))
}

/// The problems of a form's own values: builder values the text cannot hold (WB-02 `check`, in fields and in
/// blocks), values of the wrong shape and blocks that cannot make a rule, each at its form path. A missing id is not
/// a problem here (ids come on the first save).
#[must_use]
pub fn check_fields(form: &FormState, cx: &FormContext<'_>) -> Vec<FormProblem> {
    let mut out = Vec::new();
    builder_problems(form, "", cx, &mut out);
    if let Err(ps) = super::form_to_elements(form, cx) {
        out.extend(
            ps.into_iter()
                .filter(|p| p.path != "id" && !p.path.ends_with(".id")),
        );
    }
    out
}

fn builder_problems(
    form: &FormState,
    prefix: &str,
    cx: &FormContext<'_>,
    out: &mut Vec<FormProblem>,
) {
    if let Some(k) = kind(&form.kind) {
        for def in k.fields.iter().filter(|d| d.control == Control::TagFilter) {
            let Some(v) = form.fields.get(def.key) else {
                continue;
            };
            let Ok(value) = serde_json::from_value::<ExprValue<TagFilter>>(v.clone()) else {
                continue;
            };
            for p in value.check::<TagFilterLanguage>(&()) {
                let sub = if p.path.is_empty() {
                    String::new()
                } else {
                    format!(".{}", p.path)
                };
                out.push(FormProblem::new(
                    format!("{prefix}fields.{}{sub}", def.key),
                    p.message,
                ));
            }
        }
        for def in k.fields.iter().filter(|d| d.codec.is_condition()) {
            let Some(v) = form.fields.get(def.key) else {
                continue;
            };
            let Ok(value) = serde_json::from_value::<ExprValue<Condition>>(v.clone()) else {
                continue;
            };
            for p in value.check::<ConditionLanguage>(&cx.conditions) {
                let sub = if p.path.is_empty() {
                    String::new()
                } else {
                    format!(".{}", p.path)
                };
                out.push(FormProblem::new(
                    format!("{prefix}fields.{}{sub}", def.key),
                    p.message,
                ));
            }
        }
    }
    for (i, b) in form.blocks.iter().enumerate() {
        out.extend(
            cx.blocks
                .check(b, cx)
                .into_iter()
                .map(|p| FormProblem::new(format!("{prefix}blocks.{i}.{}", p.path), p.message)),
        );
    }
    for (i, c) in form.children.iter().enumerate() {
        builder_problems(c, &format!("{prefix}children.{i}."), cx, out);
    }
}
