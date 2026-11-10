//! The block catalog as the form's block codec (WB-04 form x WB-05 catalog).
//!
//! A rule becomes a [`Block`] whose `fields` are the fields its catalog block ([`crate::blocks::BlockSpec`]) offers,
//! plus the gate fields `level` ("From level") and `requirements` ("Only if") where the block shows them. Fields the
//! block doesn't offer, and stored values its controls can't hold ([`crate::blocks::misfits`]), go to the block's
//! preserved bucket unchanged (spec 3.4). A rule the catalog has no block for (`Rule::Unknown`, an unknown custom
//! kind) is kept whole.
//!
//! Value shapes in `fields`, by control (TS `HomebrewBlockValueByControl`): builder controls (`condition`,
//! `tagFilter`, `equipped`, `formula`) hold an [`ExprValue`], `formulas` a list of them; `reference` a [`RefValue`],
//! `references` a list of them; `blocks` (a toggle's effects) a list of nested [`Block`]s; `group`, `rows`, `variant`
//! and `map` objects whose parts follow the same rules; every other control the stored JSON value.
//!
//! The stored-text rule: the block remembers ([`BlockMemo`]) each field's value as read and its stored JSON. A field
//! whose value is unchanged writes its stored JSON back byte for byte; an edited field writes canonically (builder
//! text through [`ExprValue::text`], references as ids, single-line text normalized). Inside an edited effect list,
//! each nested effect that is unchanged writes its stored rule back.

use super::block::{kept_part, Block, BlockCodec, BlockMemo, WHOLE};
use super::codec::decode;
use super::value::RefValue;
use super::{FormContext, FormProblem};
use crate::blocks::{self, Control, SentenceContext};
use crate::expr_build::condition::ConditionLanguage;
use crate::expr_build::equipped::EquippedLanguage;
use crate::expr_build::formula::FormulaLanguage;
use crate::expr_build::tag_filter::TagFilterLanguage;
use crate::expr_build::{BuilderProblem, ExprLanguage, ExprValue};
use borealis_content::rich::normalize_plain_text;
use borealis_content::rule_kinds;
use borealis_content::Rule;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// The block codec of the creator: rules as the catalog's blocks (spec 6), with their plain sentences.
#[derive(Debug, Clone, Copy, Default)]
pub struct CatalogBlockCodec;

/// The problem of a block whose values can't make a rule yet (its required fields say which).
pub(crate) const INCOMPLETE: &str = "This block isn't complete yet. Fill in what it needs.";

/// "From level": a class or progression level.
const LEVEL: Control = Control::Number { min: 1, max: 20 };

/// The control of a block field, the gate fields included.
fn control_of(spec: &blocks::BlockSpec, key: &str) -> Option<Control> {
    match key {
        "level" if spec.level_gated => Some(LEVEL),
        "requirements" if spec.only_if => Some(Control::Condition),
        _ => spec.field(key).map(|f| f.control),
    }
}

/// The rule kind a block kind writes (`list` is a `select` of type `List`).
fn rule_kind(kind: &str) -> &str {
    if kind == "list" {
        "select"
    } else {
        kind
    }
}

/// The form context as the sentences' context: names and types of loaded elements, the loaded class names.
struct Names<'a, 'b>(&'a FormContext<'b>);

impl SentenceContext for Names<'_, '_> {
    fn element(&self, id: &str) -> Option<(String, String)> {
        self.0.lookup.find(id).map(|r| (r.name, r.ty))
    }

    fn class_names(&self) -> Vec<String> {
        self.0.conditions.classes().map(str::to_string).collect()
    }
}

fn to_json<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

fn words(key: &str) -> String {
    key.replace(['_', '-'], " ")
}

/// Plain words for a part of a rule the block's dialog doesn't edit.
fn kept_label(kind: &str, field: &str) -> String {
    let w = words(field);
    match (rule_kind(kind), field) {
        (_, "extra") => "Unknown settings of this rule".to_string(),
        (_, "setters") => "Values set under this rule".to_string(),
        ("select", "default_behaviour" | "existing" | "stat")
        | ("stat", "not_equipped" | "minimum" | "merge") => {
            format!("A setting Borealis does not use (“{w}”)")
        }
        (k, f) => match rule_kinds::spec(k) {
            Some(s) if s.knows(f) && !s.reads(f) => {
                format!("A setting Borealis keeps but does not apply (“{w}”)")
            }
            Some(s) if !s.knows(f) => format!("An unknown setting (“{w}”)"),
            _ => format!("A value this block can't edit (“{w}”)"),
        },
    }
}

/// The text of a stored expression or formula (formulas may be stored as numbers).
fn stored_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) if n.is_i64() => Some(n.to_string()),
        _ => None,
    }
}

fn read_expr<L>(v: &Value, cx: &L::Context) -> Value
where
    L: ExprLanguage,
    L::Model: Serialize,
{
    stored_text(v).map_or_else(
        || v.clone(),
        |t| to_json(&ExprValue::<L::Model>::read::<L>(&t, cx)),
    )
}

fn write_expr<L>(v: &Value, path: &str, cx: &L::Context) -> Result<Value, FormProblem>
where
    L: ExprLanguage,
    L::Model: DeserializeOwned,
{
    let e: ExprValue<L::Model> = decode(v, path)?;
    Ok(Value::String(e.text::<L>(cx)))
}

fn sub_path(path: &str, part: impl std::fmt::Display) -> String {
    format!("{path}.{part}")
}

/// Every nested effect reads as a block with nothing kept (else the whole list stays as is).
fn effects_fit(v: &Value) -> bool {
    v.as_array().is_some_and(|items| {
        items.iter().all(|x| {
            serde_json::from_value::<Rule>(x.clone())
                .ok()
                .and_then(|r| blocks::fits(&r))
                .is_some_and(|m| m.is_empty())
        })
    })
}

/// The parts of a group or variant object as their controls hold them (`f` per part).
fn map_parts<E>(
    v: &Value,
    part: impl Fn(&str) -> Option<Control>,
    mut f: impl FnMut(&Control, &str, &Value) -> Result<Value, E>,
) -> Result<Value, E> {
    let Some(m) = v.as_object() else {
        return Ok(v.clone());
    };
    let mut out = Map::new();
    for (k, x) in m {
        if x.is_null() {
            continue;
        }
        let y = match part(k) {
            Some(c) => f(&c, k, x)?,
            None => x.clone(),
        };
        out.insert(k.clone(), y);
    }
    Ok(Value::Object(out))
}

fn group_part(fields: &[blocks::BlockField], key: &str) -> Option<Control> {
    fields.iter().find(|f| f.key == key).map(|f| f.control)
}

fn variant_part(variants: &[blocks::Variant], v: &Value, key: &str) -> Option<Control> {
    let ty = v.get("type").and_then(Value::as_str)?;
    variants
        .iter()
        .find(|x| x.value == ty)
        .and_then(|x| group_part(x.fields, key))
}

fn map_items<E>(
    v: &Value,
    mut f: impl FnMut(usize, &Value) -> Result<Value, E>,
) -> Result<Value, E> {
    match v.as_array() {
        Some(items) => items
            .iter()
            .enumerate()
            .map(|(i, x)| f(i, x))
            .collect::<Result<Vec<_>, E>>()
            .map(Value::Array),
        None => Ok(v.clone()),
    }
}

fn never<T>(r: Result<T, std::convert::Infallible>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => match e {},
    }
}

impl CatalogBlockCodec {
    /// A stored value as the form holds it.
    fn read_value(&self, control: &Control, v: &Value, cx: &FormContext<'_>) -> Value {
        match control {
            Control::Condition => read_expr::<ConditionLanguage>(v, &cx.conditions),
            Control::TagFilter => read_expr::<TagFilterLanguage>(v, &()),
            Control::Equipped => read_expr::<EquippedLanguage>(v, &()),
            Control::Formula(d) => read_expr::<FormulaLanguage>(v, d),
            Control::Formulas(d) => {
                never(map_items(v, |_, x| Ok(read_expr::<FormulaLanguage>(x, d))))
            }
            Control::Reference { .. } => v.as_str().map_or_else(
                || v.clone(),
                |id| to_json(&RefValue::resolve(id, cx.lookup)),
            ),
            Control::References { .. } => never(map_items(v, |_, x| {
                Ok(self.read_value(&Control::Reference { types: &[] }, x, cx))
            })),
            Control::Group(fields) => never(map_parts(
                v,
                |k| group_part(fields, k),
                |c, _, x| Ok(self.read_value(c, x, cx)),
            )),
            Control::Rows(fields) => never(map_items(v, |_, row| {
                map_parts(
                    row,
                    |k| group_part(fields, k),
                    |c, _, x| Ok(self.read_value(c, x, cx)),
                )
            })),
            Control::Variant(variants) => never(map_parts(
                v,
                |k| variant_part(variants, v, k),
                |c, _, x| Ok(self.read_value(c, x, cx)),
            )),
            Control::Map([_, value]) => never(map_parts(
                v,
                |_| Some(value.control),
                |c, _, x| Ok(self.read_value(c, x, cx)),
            )),
            Control::Blocks(_) => never(map_items(v, |i, x| {
                Ok(match serde_json::from_value::<Rule>(x.clone()) {
                    Ok(rule) => {
                        let nested = self.read(&rule, format!("e{i}"), cx);
                        to_json(&Block {
                            memo: None,
                            ..nested
                        })
                    }
                    Err(_) => x.clone(),
                })
            })),
            _ => v.clone(),
        }
    }

    /// A form value as the rule stores it, canonically. `memo` is the field's value as read and its stored JSON
    /// (used by effect lists, whose unchanged effects write their stored rule).
    fn write_value(
        &self,
        control: &Control,
        v: &Value,
        path: &str,
        cx: &FormContext<'_>,
        memo: Option<(&Value, &Value)>,
    ) -> Result<Value, FormProblem> {
        match control {
            Control::Condition => write_expr::<ConditionLanguage>(v, path, &cx.conditions),
            Control::TagFilter => write_expr::<TagFilterLanguage>(v, path, &()),
            Control::Equipped => write_expr::<EquippedLanguage>(v, path, &()),
            Control::Formula(d) => write_expr::<FormulaLanguage>(v, path, d),
            Control::Formulas(d) => map_items(v, |i, x| {
                write_expr::<FormulaLanguage>(x, &sub_path(path, i), d)
            }),
            Control::Reference { .. } => {
                let r: RefValue = decode(v, path)?;
                Ok(Value::String(r.id))
            }
            Control::References { .. } => {
                let rs: Vec<RefValue> = decode(v, path)?;
                Ok(Value::Array(
                    rs.into_iter().map(|r| Value::String(r.id)).collect(),
                ))
            }
            Control::Text
            | Control::Name {
                multiple: false, ..
            } => {
                let s: String = decode(v, path)?;
                Ok(Value::String(normalize_plain_text(&s)))
            }
            Control::Name { multiple: true, .. } => {
                let names: Vec<String> = decode(v, path)?;
                Ok(Value::Array(
                    names
                        .iter()
                        .map(|s| Value::String(normalize_plain_text(s)))
                        .collect(),
                ))
            }
            Control::Group(fields) => map_parts(
                v,
                |k| group_part(fields, k),
                |c, k, x| self.write_value(c, x, &sub_path(path, k), cx, None),
            ),
            Control::Rows(fields) => map_items(v, |i, row| {
                let at = sub_path(path, i);
                map_parts(
                    row,
                    |k| group_part(fields, k),
                    |c, k, x| self.write_value(c, x, &sub_path(&at, k), cx, None),
                )
            }),
            Control::Variant(variants) => map_parts(
                v,
                |k| variant_part(variants, v, k),
                |c, k, x| self.write_value(c, x, &sub_path(path, k), cx, None),
            ),
            Control::Map([_, value]) => map_parts(
                v,
                |_| Some(value.control),
                |c, k, x| self.write_value(c, x, &sub_path(path, k), cx, None),
            ),
            Control::Blocks(_) => self.write_effects(v, path, cx, memo),
            _ => Ok(v.clone()),
        }
    }

    /// A toggle's effects: each nested block as a rule; an effect unchanged since it was read writes its stored rule.
    fn write_effects(
        &self,
        v: &Value,
        path: &str,
        cx: &FormContext<'_>,
        memo: Option<(&Value, &Value)>,
    ) -> Result<Value, FormProblem> {
        let read: &[Value] = memo
            .and_then(|(r, _)| r.as_array())
            .map_or(&[], Vec::as_slice);
        let raw: &[Value] = memo
            .and_then(|(_, r)| r.as_array())
            .map_or(&[], Vec::as_slice);
        let items: Vec<Block> = decode(v, path)?;
        let mut out = Vec::new();
        for (i, b) in items.iter().enumerate() {
            let at = sub_path(path, i);
            let stored = read.iter().position(|x| {
                serde_json::from_value::<Block>(x.clone())
                    .is_ok_and(|r| r.key == b.key && r.kind == b.kind && r.fields == b.fields)
            });
            if let Some(stored) = stored.and_then(|j| raw.get(j)) {
                out.push(stored.clone());
                continue;
            }
            match self.write(b, cx) {
                Ok(Some(rule)) => out.push(to_json(&rule)),
                Ok(None) => {}
                Err(p) => return Err(FormProblem::new(sub_path(&at, &p.path), p.message)),
            }
        }
        Ok(Value::Array(out))
    }

    fn check_value(
        &self,
        control: &Control,
        v: &Value,
        path: &str,
        cx: &FormContext<'_>,
        out: &mut Vec<FormProblem>,
    ) {
        match control {
            Control::Condition => check_expr::<ConditionLanguage>(v, path, &cx.conditions, out),
            Control::TagFilter => check_expr::<TagFilterLanguage>(v, path, &(), out),
            Control::Equipped => check_expr::<EquippedLanguage>(v, path, &(), out),
            Control::Formula(d) => check_expr::<FormulaLanguage>(v, path, d, out),
            Control::Formulas(d) => {
                for (i, x) in v.as_array().into_iter().flatten().enumerate() {
                    check_expr::<FormulaLanguage>(x, &sub_path(path, i), d, out);
                }
            }
            Control::Group(fields) => self.check_parts(v, path, |k| group_part(fields, k), cx, out),
            Control::Rows(fields) => {
                for (i, row) in v.as_array().into_iter().flatten().enumerate() {
                    let at = sub_path(path, i);
                    self.check_parts(row, &at, |k| group_part(fields, k), cx, out);
                }
            }
            Control::Variant(variants) => {
                self.check_parts(v, path, |k| variant_part(variants, v, k), cx, out);
            }
            Control::Map([_, value]) => self.check_parts(v, path, |_| Some(value.control), cx, out),
            Control::Blocks(_) => {
                for (i, x) in v.as_array().into_iter().flatten().enumerate() {
                    if let Ok(b) = serde_json::from_value::<Block>(x.clone()) {
                        let at = sub_path(path, i);
                        out.extend(
                            self.check(&b, cx)
                                .into_iter()
                                .map(|p| FormProblem::new(sub_path(&at, &p.path), p.message)),
                        );
                    }
                }
            }
            _ => {}
        }
    }

    fn check_parts(
        &self,
        v: &Value,
        path: &str,
        part: impl Fn(&str) -> Option<Control>,
        cx: &FormContext<'_>,
        out: &mut Vec<FormProblem>,
    ) {
        for (k, x) in v.as_object().into_iter().flatten() {
            if let Some(c) = part(k) {
                self.check_value(&c, x, &sub_path(path, k), cx, out);
            }
        }
    }

    /// A rule the catalog has no block for, kept whole.
    fn kept_whole(rule: &Rule, key: String, cx: &FormContext<'_>) -> Block {
        let json = to_json(rule);
        let kind = json
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let name = match rule {
            Rule::Unknown(u) => u.tag.clone(),
            _ => kind.clone(),
        };
        Block {
            key,
            sentence: blocks::sentence(rule, &Names(cx)),
            fields: BTreeMap::new(),
            preserved: vec![kept_part(
                WHOLE,
                format!("A rule Borealis does not read (“{}”)", words(&name)),
                json,
            )],
            kind,
            memo: None,
        }
    }
}

fn check_expr<L>(v: &Value, path: &str, cx: &L::Context, out: &mut Vec<FormProblem>)
where
    L: ExprLanguage,
    L::Model: DeserializeOwned,
{
    let Ok(e) = serde_json::from_value::<ExprValue<L::Model>>(v.clone()) else {
        return;
    };
    out.extend(e.check::<L>(cx).into_iter().map(|p: BuilderProblem| {
        let at = if p.path.is_empty() {
            path.to_string()
        } else {
            sub_path(path, &p.path)
        };
        FormProblem::new(at, p.message)
    }));
}

impl BlockCodec for CatalogBlockCodec {
    fn read(&self, rule: &Rule, key: String, cx: &FormContext<'_>) -> Block {
        let Some(spec) = blocks::kind_of(rule).and_then(blocks::block) else {
            return Self::kept_whole(rule, key, cx);
        };
        let Value::Object(mut stored) = to_json(rule) else {
            return Self::kept_whole(rule, key, cx);
        };
        stored.remove("kind");
        let mut misfit: BTreeSet<String> = blocks::misfits(spec, &stored).into_iter().collect();
        for (k, v) in &stored {
            if matches!(control_of(spec, k), Some(Control::Blocks(_))) && !effects_fit(v) {
                misfit.insert(k.clone());
            }
        }
        let mut fields = BTreeMap::new();
        let mut raw = BTreeMap::new();
        let mut preserved = Vec::new();
        for (k, v) in stored {
            match control_of(spec, &k).filter(|_| !misfit.contains(&k)) {
                Some(control) => {
                    fields.insert(k.clone(), self.read_value(&control, &v, cx));
                    raw.insert(k, v);
                }
                None => preserved.push(kept_part(&k, kept_label(spec.kind, &k), v)),
            }
        }
        Block {
            key,
            kind: spec.kind.to_string(),
            sentence: blocks::sentence(rule, &Names(cx)),
            memo: Some(BlockMemo {
                kind: spec.kind.to_string(),
                read: fields.clone(),
                raw,
            }),
            fields,
            preserved,
        }
    }

    fn write(&self, block: &Block, cx: &FormContext<'_>) -> Result<Option<Rule>, FormProblem> {
        let kept = || block.preserved.iter().filter_map(|p| p.kept.as_ref());
        if let Some(whole) = kept().find(|k| k.location == WHOLE) {
            return serde_json::from_value(whole.value.clone())
                .map(Some)
                .map_err(|_| {
                    FormProblem::new(
                        "preserved",
                        "This rule, kept from the original, can't be read any more. Remove it or undo the last change.",
                    )
                });
        }
        let Some(spec) = blocks::block(&block.kind) else {
            // a kept rule whose part was removed writes nothing
            return if block.fields.is_empty() {
                Ok(None)
            } else {
                Err(FormProblem::new(
                    "kind",
                    format!("There is no block called “{}”.", words(&block.kind)),
                ))
            };
        };
        let memo = block.memo.as_ref().filter(|m| m.kind == block.kind);
        let mut map = Map::new();
        for (k, v) in &block.fields {
            // the levels a block is gained again at are the form's (`repeat.rs`), not the rule's
            if v.is_null() || k == super::AGAIN_AT {
                continue;
            }
            let path = format!("fields.{k}");
            let read = memo.and_then(|m| m.read.get(k));
            let raw = memo.and_then(|m| m.raw.get(k));
            if let (Some(r), Some(stored)) = (read, raw) {
                if r == v {
                    map.insert(k.clone(), stored.clone());
                    continue;
                }
            }
            let Some(control) = control_of(spec, k) else {
                return Err(FormProblem::new(
                    path,
                    format!("This block has no setting “{}”.", words(k)),
                ));
            };
            let value = self.write_value(&control, v, &path, cx, read.zip(raw))?;
            map.insert(k.clone(), value);
        }
        for k in kept() {
            map.insert(k.location.clone(), k.value.clone());
        }
        if spec.kind == "list" {
            map.entry("type")
                .or_insert_with(|| Value::String("List".into()));
        }
        map.insert("kind".into(), Value::String(rule_kind(spec.kind).into()));
        serde_json::from_value(Value::Object(map))
            .map(Some)
            .map_err(|_| FormProblem::new("fields", INCOMPLETE))
    }

    fn refresh(&self, block: &Block, cx: &FormContext<'_>) -> Block {
        let mut fields = block.fields.clone();
        if let Some(spec) = blocks::block(&block.kind) {
            for (k, v) in &mut fields {
                if matches!(control_of(spec, k), Some(Control::Blocks(_))) {
                    *v = never(map_items(v, |_, x| {
                        Ok(match serde_json::from_value::<Block>(x.clone()) {
                            Ok(b) => to_json(&self.refresh(&b, cx)),
                            Err(_) => x.clone(),
                        })
                    }));
                }
            }
        }
        let next = Block {
            fields,
            ..block.clone()
        };
        match self.write(&next, cx) {
            Ok(Some(rule)) => Block {
                sentence: blocks::sentence(&rule, &Names(cx)),
                ..next
            },
            _ => next,
        }
    }

    fn check(&self, block: &Block, cx: &FormContext<'_>) -> Vec<FormProblem> {
        let mut out = Vec::new();
        if let Some(spec) = blocks::block(&block.kind) {
            for (k, v) in &block.fields {
                if let Some(c) = control_of(spec, k) {
                    self.check_value(&c, v, &format!("fields.{k}"), cx, &mut out);
                }
            }
        }
        out
    }
}

#[cfg(test)]
#[path = "block_codec_tests.rs"]
mod tests;
