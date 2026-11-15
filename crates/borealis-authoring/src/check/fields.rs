//! Form checks (spec 8.1 step 1): required fields of the form and of every block (`FieldDef::required`,
//! `BlockField::required`, `KindSpec::required`), numbers in their range ("From level" 1-20...), dice that read as
//! dice, values from the engine's closed sets (`KindSpec::enums`), each at the form path of its field
//! (`fields.<key>`, `blocks.N.fields.<key>`, `blocks.N.fields.effects.M.fields.<key>`, `children.C.` before them).
//!
//! Blocks are checked on the rule they write (the stored shapes), so a builder value is judged by its text. Parts
//! kept from the original are never judged: they are what the original had (spec 3.4).

use super::messages::{with_article, Finding};
use crate::blocks::{self, BlockField, BlockSpec, Control};
use crate::form::kinds::{kind, KindDef, OTHER_KIND};
use crate::form::{Block, FormContext, FormState};
use borealis_content::rule_kinds;
use borealis_content::Rule;
use serde_json::{Map, Value};
use std::collections::BTreeSet;

/// A finding at a form path.
pub type At = (String, Finding);

/// "From level" of gated blocks.
const LEVELS: (i32, i32) = (1, 20);
/// The Skill form ([`crate::form::kinds`]).
const SKILL_KIND: &str = "skill";

/// Nothing filled in: absent, null, blank text, an empty list or object, a reference with no element picked.
fn empty(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => s.trim().is_empty(),
        Some(Value::Array(a)) => a.is_empty(),
        Some(Value::Object(o)) => {
            o.is_empty()
                || o.get("id")
                    .is_some_and(|id| id.as_str().is_some_and(|t| t.trim().is_empty()))
        }
        Some(_) => false,
    }
}

/// "a deity", "this element" (the generic form has no noun of its own).
fn subject(k: &KindDef) -> String {
    if k.id == OTHER_KIND {
        "this element".into()
    } else {
        with_article(&k.label.to_lowercase())
    }
}

/// A block's label as the start of a sentence ("Gives ..." -> "Gives").
fn block_label(spec: &BlockSpec) -> String {
    spec.label.trim_end_matches(['.', ' ']).to_string()
}

/// The findings of a draft's forms (the root and its children), paths from the root.
#[must_use]
pub fn form_findings(form: &FormState, cx: &FormContext<'_>) -> Vec<At> {
    let mut out = Vec::new();
    one_form(form, "", cx, &mut out);
    out
}

fn one_form(form: &FormState, prefix: &str, cx: &FormContext<'_>, out: &mut Vec<At>) {
    if let Some(k) = kind(&form.kind) {
        for def in k.fields.iter().filter(|d| d.required && !d.read_only) {
            // a picked reference is the codec's to report ("Choose the element to add to.")
            if def.control == crate::form::Control::Reference || !empty(form.fields.get(def.key)) {
                continue;
            }
            let finding = if def.key == "name" {
                Finding::NoName
            } else {
                Finding::RequiredField {
                    subject: subject(k),
                    field: def.label.to_string(),
                }
            };
            out.push((format!("{prefix}fields.{}", def.key), finding));
        }
    }
    if let Some(name) = skill_name_refused(form) {
        out.push((format!("{prefix}fields.name"), name));
    }
    if let Some(r) = crate::form::type_refusal(form) {
        let finding = Finding::TypeNotHere {
            ty: r.ty,
            made_with: r.made_with,
        };
        out.push((format!("{prefix}fields.type"), finding));
    }
    for (i, b) in form.blocks.iter().enumerate() {
        block_findings(b, &format!("{prefix}blocks.{i}"), cx, out);
    }
    for (i, c) in form.children.iter().enumerate() {
        one_form(c, &format!("{prefix}children.{i}."), cx, out);
    }
}

/// A Skill form's name that the engine can't make a skill of (HC1, item HtCY8: a homebrew skill is a character skill
/// keyed by its name).
fn skill_name_refused(form: &FormState) -> Option<Finding> {
    if form.kind != SKILL_KIND {
        return None;
    }
    let name = form.fields.get("name").and_then(Value::as_str)?.trim();
    (!name.is_empty() && borealis_engine::content_skills::skill_key(name).is_none())
        .then_some(Finding::SkillName)
}

/// The keys of a block's rule kept from the original.
fn kept_keys(block: &Block) -> BTreeSet<String> {
    block
        .preserved
        .iter()
        .filter_map(|p| p.kept.as_ref().map(|k| k.location.clone()))
        .collect()
}

fn rule_map(rule: &Rule) -> Option<Map<String, Value>> {
    match serde_json::to_value(rule) {
        Ok(Value::Object(mut m)) => {
            m.remove("kind");
            Some(m)
        }
        _ => None,
    }
}

/// One block at `path` (`blocks.N`): the rule it writes, checked against its catalog block.
fn block_findings(block: &Block, path: &str, cx: &FormContext<'_>, out: &mut Vec<At>) {
    let Some(spec) = blocks::block(&block.kind) else {
        return;
    };
    let rule = match cx.blocks.write(block, cx) {
        Ok(Some(rule)) => rule,
        Ok(None) => return,
        // the values can't make a rule yet: say which required ones are missing
        Err(_) => return form_required(spec, block, path, out),
    };
    let Some(map) = rule_map(&rule) else {
        return;
    };
    let nested: Vec<Block> = spec
        .fields
        .iter()
        .filter(|f| matches!(f.control, Control::Blocks(_)))
        .filter_map(|f| block.fields.get(f.key))
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(|v| serde_json::from_value(v.clone()).ok())
        .collect();
    // a skill block names a skill loaded content adds by its name (HC16)
    let skills = crate::form::skill_keys(cx.lookup);
    let rule = RuleAt {
        spec,
        kept: &kept_keys(block),
        nested: &nested,
        skills: &skills,
        path,
    };
    rule_findings(&rule, &map, out);
}

/// One rule checked against its catalog block: the parts kept from the original, the form's effect blocks (for their
/// kept parts, in order), the skills loaded content adds and the rule's form path.
struct RuleAt<'a> {
    spec: &'a BlockSpec,
    kept: &'a BTreeSet<String>,
    nested: &'a [Block],
    skills: &'a [String],
    path: &'a str,
}

/// The required fields of a block that has no value in the form (a reference not picked yet, an empty text).
fn form_required(spec: &BlockSpec, block: &Block, path: &str, out: &mut Vec<At>) {
    let kept = kept_keys(block);
    for f in spec
        .fields
        .iter()
        .filter(|f| f.required && f.control != Control::Auto && !kept.contains(f.key))
    {
        if empty(block.fields.get(f.key)) {
            out.push((
                format!("{path}.fields.{}", f.key),
                Finding::BlockRequired {
                    block: block_label(spec),
                    field: f.label.to_string(),
                },
            ));
        }
    }
}

/// The fields of one rule (`map`, without `kind`) of block `rule.spec`.
fn rule_findings(rule: &RuleAt<'_>, map: &Map<String, Value>, out: &mut Vec<At>) {
    let RuleAt {
        spec,
        kept,
        nested,
        skills,
        path,
    } = *rule;
    let label = block_label(spec);
    let at = |key: &str| format!("{path}.fields.{key}");
    let live: Map<String, Value> = map
        .iter()
        .filter(|(k, _)| !kept.contains(*k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mut required: Vec<&BlockField> = spec
        .fields
        .iter()
        .filter(|f| f.required && f.control != Control::Auto)
        .collect();
    if let Some(ks) = rule_kinds::spec(spec.kind) {
        for r in ks.required {
            if let Some(f) = spec.field(r).filter(|f| !required.contains(f)) {
                required.push(f);
            }
        }
    }
    for f in required {
        if !kept.contains(f.key) && empty(live.get(f.key)) {
            out.push((
                at(f.key),
                Finding::BlockRequired {
                    block: label.clone(),
                    field: f.label.to_string(),
                },
            ));
        }
    }
    let misfit: BTreeSet<String> = blocks::misfits_with(spec, &live, skills)
        .into_iter()
        .collect();
    let mut effect = 0;
    for (k, v) in &live {
        let p = at(k);
        if k == "level" && spec.level_gated {
            if misfit.contains(k) {
                out.push((p, range(&label, "From level", LEVELS)));
            }
            continue;
        }
        let Some(f) = spec.field(k) else {
            continue;
        };
        if let Control::Blocks(_) = f.control {
            for (i, rule) in v.as_array().into_iter().flatten().enumerate() {
                let own = nested.get(effect + i);
                effect_findings(rule, own, skills, &format!("{p}.{i}"), out);
            }
            effect += v.as_array().map_or(0, Vec::len);
            continue;
        }
        // what fits was judged by `misfits` (ranges, choices, values shown verbatim); parts and dice need a closer look
        let compound = matches!(
            f.control,
            Control::Group(_)
                | Control::Rows(_)
                | Control::Variant(_)
                | Control::Map(_)
                | Control::Dice
        );
        if !misfit.contains(k) && !compound {
            continue;
        }
        let before = out.len();
        value_findings(&f.control, v, &p, &label, f.label, out);
        if misfit.contains(k) && out.len() == before {
            out.push((p, Finding::BlockShape));
        }
    }
    enum_findings(spec, &live, &label, path, out);
}

/// A nested effect rule (a toggle's effect) at `path` (`blocks.N.fields.effects.M`).
fn effect_findings(
    rule: &Value,
    own: Option<&Block>,
    skills: &[String],
    path: &str,
    out: &mut Vec<At>,
) {
    let Ok(parsed) = serde_json::from_value::<Rule>(rule.clone()) else {
        out.push((path.to_string(), Finding::BlockShape));
        return;
    };
    let Some(spec) = blocks::kind_of(&parsed).and_then(blocks::block) else {
        return;
    };
    let Some(map) = rule_map(&parsed) else {
        return;
    };
    let kept = own.map(kept_keys).unwrap_or_default();
    let at = RuleAt {
        spec,
        kept: &kept,
        nested: &[],
        skills,
        path,
    };
    rule_findings(&at, &map, out);
}

fn range(block: &str, field: &str, (min, max): (i32, i32)) -> Finding {
    Finding::BlockRange {
        block: block.to_string(),
        field: field.to_string(),
        min,
        max,
    }
}

fn int_in(v: &Value, min: i32, max: i32) -> bool {
    v.as_i64()
        .is_some_and(|n| (i64::from(min)..=i64::from(max)).contains(&n))
}

/// Dice text: terms joined by `+`/`-`, each a whole number or `NdM` (`2d6`, `1d8+2`, `d4`).
#[must_use]
pub fn is_dice(text: &str) -> bool {
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if t.is_empty() {
        return false;
    }
    let t = t
        .strip_prefix(['+', '-'])
        .unwrap_or(&t)
        .to_ascii_lowercase();
    t.split(['+', '-']).all(|term| {
        let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
        match term.split_once('d') {
            Some((n, m)) => (n.is_empty() || digits(n)) && digits(m),
            None => digits(term),
        }
    })
}

/// One value of a field at `path`: ranges, dice, choices; the parts of groups, rows, variants and maps.
fn value_findings(
    control: &Control,
    v: &Value,
    path: &str,
    block: &str,
    field: &str,
    out: &mut Vec<At>,
) {
    let push = |out: &mut Vec<At>, f: Finding| out.push((path.to_string(), f));
    match control {
        Control::Number { min, max } if !int_in(v, *min, *max) => {
            push(out, num_or_shape(v, block, field, (*min, *max)));
        }
        Control::NumberOr { min, max, .. } if v.is_number() && !int_in(v, *min, *max) => {
            push(out, range(block, field, (*min, *max)));
        }
        Control::Numbers { min, max } => {
            if v.as_array()
                .is_some_and(|a| a.iter().any(|x| !int_in(x, *min, *max)))
            {
                push(out, range(block, field, (*min, *max)));
            }
        }
        Control::Dice => match v.as_str() {
            Some(s) if !is_dice(s) => push(
                out,
                Finding::BlockDice {
                    block: block.to_string(),
                    field: field.to_string(),
                },
            ),
            _ => {}
        },
        // a block's skill field gets here only with a value no choice and no loaded skill has (`misfits_with`)
        Control::Combo { .. } | Control::Chips { .. } | Control::Skills { .. }
            if !blocks::value_fits(control, v) =>
        {
            push(
                out,
                Finding::BlockChoice {
                    block: block.to_string(),
                    field: field.to_string(),
                },
            );
        }
        Control::Group(parts) => match v.as_object() {
            Some(m) => part_findings(parts, m, path, block, out),
            None => push(out, Finding::BlockShape),
        },
        Control::Rows(parts) => {
            for (i, row) in v.as_array().into_iter().flatten().enumerate() {
                match row.as_object() {
                    Some(m) => part_findings(parts, m, &format!("{path}.{i}"), block, out),
                    None => out.push((format!("{path}.{i}"), Finding::BlockShape)),
                }
            }
        }
        Control::Variant(variants) => {
            let ty = v.get("type").and_then(Value::as_str);
            match (v.as_object(), variants.iter().find(|x| Some(x.value) == ty)) {
                (Some(m), Some(x)) => {
                    let rest: Map<String, Value> = m
                        .iter()
                        .filter(|(k, _)| k.as_str() != "type")
                        .map(|(k, x)| (k.clone(), x.clone()))
                        .collect();
                    part_findings(x.fields, &rest, path, block, out);
                }
                _ => push(
                    out,
                    Finding::BlockChoice {
                        block: block.to_string(),
                        field: field.to_string(),
                    },
                ),
            }
        }
        Control::Map([_, value]) => {
            for (k, x) in v.as_object().into_iter().flatten() {
                value_findings(&value.control, x, &format!("{path}.{k}"), block, field, out);
            }
        }
        _ => {}
    }
}

fn num_or_shape(v: &Value, block: &str, field: &str, bounds: (i32, i32)) -> Finding {
    if v.is_number() {
        range(block, field, bounds)
    } else {
        Finding::BlockShape
    }
}

/// The parts of an object: required ones filled, each value checked.
fn part_findings(
    parts: &[BlockField],
    m: &Map<String, Value>,
    path: &str,
    block: &str,
    out: &mut Vec<At>,
) {
    for f in parts
        .iter()
        .filter(|f| f.required && f.control != Control::Auto)
    {
        if empty(m.get(f.key)) {
            out.push((
                format!("{path}.{}", f.key),
                Finding::BlockRequired {
                    block: block.to_string(),
                    field: f.label.to_string(),
                },
            ));
        }
    }
    for (k, x) in m {
        if let Some(f) = parts.iter().find(|f| f.key == k) {
            value_findings(&f.control, x, &format!("{path}.{k}"), block, f.label, out);
        }
    }
}

/// Values outside the engine's closed sets (`KindSpec::enums`: `field`, `field.sub`, `field[]`).
fn enum_findings(
    spec: &BlockSpec,
    live: &Map<String, Value>,
    label: &str,
    path: &str,
    out: &mut Vec<At>,
) {
    let Some(ks) = rule_kinds::spec(spec.kind) else {
        return;
    };
    for (p, allowed) in ks.enums {
        let ok = |v: &Value| v.as_str().is_some_and(|t| allowed.contains(&t));
        let (field, bad) = if let Some(list) = p.strip_suffix("[]") {
            (
                list,
                live.get(list)
                    .is_some_and(|v| !v.as_array().is_some_and(|a| a.iter().all(ok))),
            )
        } else if let Some((field, key)) = p.split_once('.') {
            (
                field,
                live.get(field).is_some_and(|v| !v.get(key).is_some_and(ok)),
            )
        } else {
            (*p, live.get(*p).is_some_and(|v| !ok(v)))
        };
        let shown = spec.field(field).map_or(field, |f| f.label);
        let at = format!("{path}.fields.{field}");
        let already = out.iter().any(|(q, _)| *q == at);
        if bad && !already {
            out.push((
                at,
                Finding::BlockChoice {
                    block: label.to_string(),
                    field: shown.to_string(),
                },
            ));
        }
    }
}

#[cfg(test)]
#[path = "fields_tests.rs"]
mod tests;
