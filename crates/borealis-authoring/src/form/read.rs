//! Element to form.

use super::codec::{self, Codec, Reader};
use super::kinds::{kind_for_type, KindDef};
use super::preserved::{self, Residue};
use super::{FormContext, FormState, Memo};
use borealis_content::Element;
use std::collections::{BTreeMap, BTreeSet};

/// Fields that take what the other fields of the kind left (tags, other setters), so they read last.
fn reads_leftovers(codec: Codec) -> bool {
    matches!(codec, Codec::Tags | Codec::OtherSetters)
}

/// The form of one stored element (key `e`).
#[must_use]
pub fn element_to_form(element: &Element, cx: &FormContext<'_>) -> FormState {
    read_one(element, "e".to_string(), cx)
}

/// The form of an element with the child elements edited inside it (a class and its features); child `i` gets the
/// key `e/<i>`.
#[must_use]
pub fn element_group_to_form(
    parent: &Element,
    children: &[&Element],
    cx: &FormContext<'_>,
) -> FormState {
    let mut form = element_to_form(parent, cx);
    form.children = children
        .iter()
        .enumerate()
        .map(|(i, c)| read_one(c, format!("e/{i}"), cx))
        .collect();
    form
}

pub(crate) fn owned_tags(kind: &KindDef) -> BTreeSet<&'static str> {
    kind.fields
        .iter()
        .flat_map(|f| f.codec.owned_tags().iter().copied())
        .collect()
}

pub(crate) fn read_one(e: &Element, key: String, cx: &FormContext<'_>) -> FormState {
    read_as(kind_for_type(&e.ty), e, key, cx)
}

pub(crate) fn read_as(kind: &KindDef, e: &Element, key: String, cx: &FormContext<'_>) -> FormState {
    let mut r = Reader::new(e, owned_tags(kind));
    let mut memo = Memo {
        id: (!e.id.is_empty()).then(|| e.id.clone()),
        fields: BTreeMap::new(),
        layout: Vec::new(),
    };
    let mut fields = BTreeMap::new();
    let ordered = kind
        .fields
        .iter()
        .filter(|f| !reads_leftovers(f.codec))
        .chain(kind.fields.iter().filter(|f| reads_leftovers(f.codec)));
    for f in ordered {
        if let Some(m) = codec::read(f.codec, &mut r, cx) {
            fields.insert(f.key.to_string(), m.value.clone());
            memo.fields.insert(f.key.to_string(), m);
        }
    }
    memo.layout = layout(&memo, e.rules.len());
    let residue = Residue {
        e,
        setters: untaken(&r.setters_taken),
        supports: untaken(&r.supports_taken),
        data: e
            .data
            .keys()
            .map(String::as_str)
            .filter(|k| !r.data_taken.contains(*k))
            .collect(),
        parts: &r.parts_taken,
    };
    FormState {
        blocks: e
            .rules
            .iter()
            .enumerate()
            .filter(|(i, _)| !r.rules_taken[*i])
            .map(|(i, rule)| cx.blocks.read(rule, format!("b{i}"), cx))
            .collect(),
        preserved: preserved::collect(&residue),
        key,
        kind: kind.id.to_string(),
        fields,
        children: Vec::new(),
        memo,
    }
}

/// [`Memo::layout`]: per stored rule, the field whose [`super::Slots::rules`] hold it, `""` for a block; empty when no
/// field took a rule.
fn layout(memo: &Memo, rules: usize) -> Vec<String> {
    let mut out = vec![String::new(); rules];
    for (key, m) in &memo.fields {
        for &i in &m.slots.rules {
            if let Some(place) = out.get_mut(i) {
                place.clone_from(key);
            }
        }
    }
    if out.iter().all(String::is_empty) {
        Vec::new()
    } else {
        out
    }
}

fn untaken(taken: &[bool]) -> Vec<usize> {
    taken
        .iter()
        .enumerate()
        .filter(|(_, t)| !**t)
        .map(|(i, _)| i)
        .collect()
}
