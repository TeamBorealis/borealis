//! The form on the wire, and ids.
//!
//! The UI edits fields, blocks, children and the list of preserved parts; it never sees the [`super::Memo`] or the
//! stored bytes of preserved parts. [`to_ui`] strips them, [`merge_ui`] takes them back from the backend's copy by
//! key, so the UI can neither lose nor forge them.

use super::block::Block;
use super::kinds::kind;
use super::preserved::PreservedPart;
use super::{FormContext, FormState, Memo};
use crate::ids::IdGenerator;
use serde_json::Value;

fn strip_parts(parts: &[PreservedPart]) -> Vec<PreservedPart> {
    parts
        .iter()
        .map(|p| PreservedPart {
            kept: None,
            ..p.clone()
        })
        .collect()
}

/// The form as the UI receives it: no memo, no stored bytes.
#[must_use]
pub fn to_ui(form: &FormState) -> FormState {
    FormState {
        memo: Memo::default(),
        preserved: strip_parts(&form.preserved),
        blocks: form
            .blocks
            .iter()
            .map(|b| Block {
                preserved: strip_parts(&b.preserved),
                memo: None,
                ..b.clone()
            })
            .collect(),
        children: form.children.iter().map(to_ui).collect(),
        ..form.clone()
    }
}

/// The parts the UI still lists, with their stored bytes from `stored`; parts the UI does not list are removed,
/// parts it invents are dropped.
fn merge_parts(stored: &[PreservedPart], ui: &[PreservedPart]) -> Vec<PreservedPart> {
    ui.iter()
        .filter_map(|p| stored.iter().find(|s| s.key == p.key).cloned())
        .collect()
}

/// The UI's edit of `stored`: its fields, blocks and children, with the memo and the preserved bytes of `stored`
/// (children matched by key, blocks by key and kind; a new child or block, or a block whose kind changed, has none).
#[must_use]
pub fn merge_ui(stored: &FormState, ui: &FormState) -> FormState {
    let blocks = ui
        .blocks
        .iter()
        .map(|b| {
            let old = stored
                .blocks
                .iter()
                .find(|s| s.key == b.key && s.kind == b.kind);
            Block {
                preserved: old.map_or_else(Vec::new, |o| merge_parts(&o.preserved, &b.preserved)),
                memo: old.and_then(|o| o.memo.clone()),
                ..b.clone()
            }
        })
        .collect();
    let children = ui
        .children
        .iter()
        .map(|c| match stored.children.iter().find(|s| s.key == c.key) {
            Some(old) => merge_ui(old, c),
            None => merge_ui(&FormState::new(&c.key, &c.kind), c),
        })
        .collect();
    FormState {
        key: stored.key.clone(),
        kind: stored.kind.clone(),
        fields: ui.fields.clone(),
        blocks,
        children,
        preserved: merge_parts(&stored.preserved, &ui.preserved),
        memo: stored.memo.clone(),
    }
}

/// The form with every block's sentence (and its nested blocks') written from its current values: call it after
/// [`merge_ui`], before the form goes back to the UI.
#[must_use]
pub fn refresh_blocks(form: &FormState, cx: &FormContext<'_>) -> FormState {
    FormState {
        blocks: form
            .blocks
            .iter()
            .map(|b| cx.blocks.refresh(b, cx))
            .collect(),
        children: form
            .children
            .iter()
            .map(|c| refresh_blocks(c, cx))
            .collect(),
        ..form.clone()
    }
}

fn text_field(form: &FormState, key: &str) -> String {
    form.fields
        .get(key)
        .or_else(|| form.memo.fields.get(key).map(|m| &m.value))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn element_type(form: &FormState) -> String {
    kind(&form.kind)
        .and_then(|k| k.new_element_type())
        .map_or_else(|| text_field(form, "type"), str::to_string)
}

fn reserve_all(form: &FormState, ids: &mut IdGenerator) {
    if let Some(id) = form.id() {
        ids.reserve(id);
    }
    for c in &form.children {
        reserve_all(c, ids);
    }
}

fn assign(form: &FormState, parent: Option<&str>, ids: &mut IdGenerator) -> FormState {
    if kind(&form.kind).is_some_and(|k| k.field("target").is_some()) {
        // an append adds to another element: it has no id of its own
        return form.clone();
    }
    let name = text_field(form, "name");
    let id = form.id().map_or_else(
        || {
            let ty = element_type(form);
            match parent {
                Some(p) => ids.child_id(&ty, p, &name),
                None => ids.element_id(&ty, &name),
            }
        },
        str::to_string,
    );
    FormState {
        memo: Memo {
            id: Some(id),
            ..form.memo.clone()
        },
        children: form
            .children
            .iter()
            .map(|c| assign(c, Some(&name), ids))
            .collect(),
        ..form.clone()
    }
}

/// The form with an id for every element that has none yet (spec 5.3: generated on the first save from the pack
/// code, the type and the name; a child's from its parent's name too). Ids it has already are kept (frozen).
#[must_use]
pub fn assign_ids(form: &FormState, ids: &mut IdGenerator) -> FormState {
    reserve_all(form, ids);
    assign(form, None, ids)
}
