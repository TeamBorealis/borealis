//! Multi-select edits (spec 12.5) and the spell-list check (12.4) on saved elements: each change goes through the
//! element's own form (read with [`element_to_form`], one field changed, written with [`form_to_element`]), so the
//! fields own what they write (a spell's lists write its tags and its `data.classes` mirror) and every part no field
//! changes comes back byte for byte (the identity rule, spec 18.2). Deterministic, no AI (G7).

use super::kinds::{kind, Control, FieldDef, KindDef, CLASS_KIND};
use super::{element_to_form, form_to_element, FormContext, FormProblem, FormState};
use borealis_content::{Element, Ruleset};
use serde_json::{json, Value};

/// What a multi-select edit changes on each element (`HomebrewBulkPatch` without the pack moves).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BulkChange {
    /// `Some(None)`: the element follows the pack's edition; `Some(Some(r))`: its own edition; `None`: unchanged.
    pub edition: Option<Option<Ruleset>>,
    /// Tags to add (each once, any case).
    pub add_tags: Vec<String>,
    /// Tags to remove (any case).
    pub remove_tags: Vec<String>,
}

impl BulkChange {
    /// Nothing to change.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.edition.is_none() && self.add_tags.is_empty() && self.remove_tags.is_empty()
    }
}

/// The field that holds a kind's tags: its Tags chips, or a spell's lists (every support tag, WB-11).
#[must_use]
pub fn tag_field(kind: &KindDef) -> Option<&FieldDef> {
    kind.fields
        .iter()
        .find(|f| matches!(f.control, Control::Tags | Control::SpellLists))
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn tags_changed(form: &FormState, field: &FieldDef, change: &BulkChange) -> Option<Vec<String>> {
    let now = strings(form.fields.get(field.key));
    let mut next: Vec<String> = now
        .iter()
        .filter(|t| {
            !change
                .remove_tags
                .iter()
                .any(|r| r.trim().eq_ignore_ascii_case(t))
        })
        .cloned()
        .collect();
    for t in change
        .add_tags
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
    {
        if !next.iter().any(|n| n.eq_ignore_ascii_case(t)) {
            next.push(t.to_string());
        }
    }
    (next != now).then_some(next)
}

fn class_keeps_edition(el: &Element) -> FormProblem {
    let year = match el.ruleset {
        Some(Ruleset::Dnd2014) => "2014",
        _ => "2024",
    };
    FormProblem::new(
        "fields.edition",
        format!(
            "{} is a class, and a class keeps its edition ({year}). Make a new class for the other edition.",
            el.name
        ),
    )
}

/// The element with `change` applied through its form; `Ok(None)` when nothing changes (it already has that edition
/// and those tags, or its form has no such field).
///
/// # Errors
/// A class asked to change edition (R4), or the form's problems when it can't write the element.
pub fn bulk_patched(
    el: &Element,
    change: &BulkChange,
    cx: &FormContext<'_>,
) -> Result<Option<Element>, Vec<FormProblem>> {
    let form = element_to_form(el, cx);
    let Some(k) = kind(&form.kind) else {
        return Ok(None);
    };
    let mut next = form.clone();
    if let Some(edition) = change.edition {
        if el.ruleset != edition && k.field("edition").is_some() {
            if k.id == CLASS_KIND {
                return Err(vec![class_keeps_edition(el)]);
            }
            next.fields
                .insert("edition".into(), json!(edition.map_or("", Ruleset::as_str)));
        }
    }
    if let Some(field) = tag_field(k) {
        if let Some(tags) = tags_changed(&form, field, change) {
            next.fields.insert(field.key.into(), json!(tags));
        }
    }
    if next.fields == form.fields {
        return Ok(None);
    }
    form_to_element(&next, cx).map(Some)
}

/// The element with `tag` added to (or removed from) its tags, as [`bulk_patched`] writes them; `Ok(None)` when it
/// has (or lacks) it already.
///
/// # Errors
/// The form's problems.
pub fn with_tag(
    el: &Element,
    tag: &str,
    add: bool,
    cx: &FormContext<'_>,
) -> Result<Option<Element>, Vec<FormProblem>> {
    let change = BulkChange {
        add_tags: if add {
            vec![tag.to_string()]
        } else {
            Vec::new()
        },
        remove_tags: if add {
            Vec::new()
        } else {
            vec![tag.to_string()]
        },
        ..BulkChange::default()
    };
    bulk_patched(el, &change, cx)
}

#[cfg(test)]
#[path = "bulk_tests.rs"]
mod tests;
