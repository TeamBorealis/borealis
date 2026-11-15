//! Appends (content-format 9.2, spec 10.3 and HC5): "add to this list" on an existing element, as a form.
//!
//! An append adds tags, setters, rules or a spellcasting node to an element of another pack (adding a homebrew spell
//! to an SRD class list writes `{id, supports: [tag]}`). Its form has the target element, the tags, the setter rows
//! and the rule blocks; its `spellcasting` and `extra` parts are preserved.

use super::kinds::{append_kind, KindDef};
use super::read::read_as;
use super::write::write_as;
use super::{FormContext, FormProblem, FormState};
use borealis_content::{Append, Element};

/// The append form's kind id.
pub const APPEND_KIND: &str = "append";

/// An append as an element of the same parts (the target id as its id, the parser selector as its type).
fn as_element(a: &Append) -> Element {
    Element {
        id: a.id.clone(),
        ty: a.ty.clone().unwrap_or_default(),
        supports: a.supports.clone(),
        setters: a.setters.clone(),
        rules: a.rules.clone(),
        spellcasting: a.spellcasting.clone(),
        extra: a.extra.clone(),
        ..Element::default()
    }
}

fn kind() -> &'static KindDef {
    append_kind()
}

/// The form of a stored append (key `a`).
#[must_use]
pub fn append_to_form(append: &Append, cx: &FormContext<'_>) -> FormState {
    let mut form = read_as(kind(), &as_element(append), "a".to_string(), cx);
    // the target is a field, not the form's own id
    form.memo.id = None;
    form
}

/// The append a form writes.
///
/// # Errors
/// The form's problems, as for elements.
pub fn form_to_append(form: &FormState, cx: &FormContext<'_>) -> Result<Append, Vec<FormProblem>> {
    let e = write_as(kind(), form, cx)?;
    Ok(Append {
        id: e.id,
        ty: (!e.ty.is_empty()).then_some(e.ty),
        supports: e.supports,
        setters: e.setters,
        rules: e.rules,
        spellcasting: e.spellcasting,
        extra: e.extra,
    })
}
