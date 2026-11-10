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
    let (r, fields, field_memos) = read_fields(kind, e, cx);
    let mut memo = Memo {
        id: (!e.id.is_empty()).then(|| e.id.clone()),
        fields: field_memos,
        layout: Vec::new(),
    };
    if r.rule_owner.iter().any(Option::is_some) {
        memo.layout = r
            .rule_owner
            .iter()
            .map(|o| o.unwrap_or_default().to_string())
            .collect();
    }
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
    };
    FormState {
        blocks: r
            .free_rules()
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

type ReadFields<'e> = (
    Reader<'e>,
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, super::FieldMemo>,
);

/// Every field of `kind` read from `e`, with what the fields took.
fn read_fields<'e>(kind: &KindDef, e: &'e Element, cx: &FormContext<'_>) -> ReadFields<'e> {
    let mut r = Reader::new(e, owned_tags(kind));
    let mut fields = BTreeMap::new();
    let mut memos = BTreeMap::new();
    let ordered = kind
        .fields
        .iter()
        .filter(|f| !reads_leftovers(f.codec))
        .chain(kind.fields.iter().filter(|f| reads_leftovers(f.codec)));
    for f in ordered {
        r.current = f.key;
        if let Some(m) = codec::read(f.codec, &mut r, cx) {
            fields.insert(f.key.to_string(), m.value.clone());
            memos.insert(f.key.to_string(), m);
        }
    }
    (r, fields, memos)
}

/// Per rule of `e` read in the form of `kind`: the key of the field that holds it, `None` for a block's.
pub(crate) fn rule_owners(
    kind: &KindDef,
    e: &Element,
    cx: &FormContext<'_>,
) -> Vec<Option<&'static str>> {
    read_fields(kind, e, cx).0.rule_owner
}

fn untaken(taken: &[bool]) -> Vec<usize> {
    taken
        .iter()
        .enumerate()
        .filter(|(_, t)| !**t)
        .map(|(i, _)| i)
        .collect()
}
