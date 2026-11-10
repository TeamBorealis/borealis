//! Form to elements.

use super::codec::{self, Codec};
use super::kinds::{kind, KindDef};
use super::preserved;
use super::{FormContext, FormProblem, FormState};
use borealis_content::{Element, Setter};

/// Sort key of a setter or tag: its stored position, or after every stored one in the order written.
type SortKey = (usize, usize);

/// The element being written, with its setters and tags collected with their positions.
pub(crate) struct Out {
    pub(crate) element: Element,
    setters: Vec<(SortKey, Setter)>,
    supports: Vec<(SortKey, String)>,
    seq: usize,
}

impl Out {
    fn new(element: Element) -> Self {
        Self {
            element,
            setters: Vec::new(),
            supports: Vec::new(),
            seq: 0,
        }
    }

    fn key(&mut self, slots: &[usize], i: usize) -> SortKey {
        if let Some(&p) = slots.get(i) {
            (p, 0)
        } else {
            self.seq += 1;
            (usize::MAX, self.seq)
        }
    }

    /// Add setters; the `i`-th goes to the stored position `slots[i]`, the rest after every stored one.
    pub(crate) fn setters(&mut self, items: impl IntoIterator<Item = Setter>, slots: &[usize]) {
        for (i, s) in items.into_iter().enumerate() {
            let k = self.key(slots, i);
            self.setters.push((k, s));
        }
    }

    /// Add support tags, placed like [`Out::setters`].
    pub(crate) fn supports(&mut self, items: impl IntoIterator<Item = String>, slots: &[usize]) {
        for (i, t) in items.into_iter().enumerate() {
            let k = self.key(slots, i);
            self.supports.push((k, t));
        }
    }

    fn finish(mut self) -> Element {
        self.setters.sort_by_key(|(k, _)| *k);
        self.supports.sort_by_key(|(k, _)| *k);
        self.element.setters = self.setters.into_iter().map(|(_, s)| s).collect();
        self.element.supports = self.supports.into_iter().map(|(_, t)| t).collect();
        self.element
    }
}

fn prefixed(prefix: &str, mut p: FormProblem) -> FormProblem {
    if !prefix.is_empty() {
        p.path = if p.path.is_empty() {
            prefix.trim_end_matches('.').to_string()
        } else {
            format!("{prefix}{}", p.path)
        };
    }
    p
}

/// The element one form writes (its children are not included). Problems carry form paths.
///
/// # Errors
/// The problems that keep the form from making an element: an unknown kind, no id yet (see
/// [`super::assign_ids`]), values of the wrong shape, rules that are not complete.
pub fn form_to_element(
    form: &FormState,
    cx: &FormContext<'_>,
) -> Result<Element, Vec<FormProblem>> {
    let Some(kind) = kind(&form.kind) else {
        return Err(vec![FormProblem::new(
            "",
            format!("There is no form called {:?}.", form.kind),
        )]);
    };
    write_as(kind, form, cx)
}

pub(crate) fn write_as(
    kind: &KindDef,
    form: &FormState,
    cx: &FormContext<'_>,
) -> Result<Element, Vec<FormProblem>> {
    let mut problems = Vec::new();
    let mut base = Element::default();
    match form.id() {
        Some(id) => base.id = id.to_string(),
        None if kind.field("target").is_some() => {}
        None => problems.push(FormProblem::new("id", "This element has no id yet.")),
    }
    base.ty = kind.new_element_type().unwrap_or_default().to_string();
    let mut out = Out::new(base);
    for f in kind.fields.iter().filter(|f| f.codec != Codec::Blocks) {
        let path = format!("fields.{}", f.key);
        let memo = form.memo.fields.get(f.key);
        let Some(value) = form.fields.get(f.key).or(memo.map(|m| &m.value)) else {
            continue;
        };
        if let Err(p) = codec::write(f.codec, value, memo, &path, &mut out, cx) {
            problems.push(p);
        }
    }
    for (i, b) in form.blocks.iter().enumerate() {
        match cx.blocks.write(b, cx) {
            Ok(Some(rule)) => out.element.rules.push(rule),
            Ok(None) => {}
            Err(p) => problems.push(prefixed(&format!("blocks.{i}."), p)),
        }
    }
    if let Err(p) = preserved::restore(&form.preserved, &mut out) {
        problems.push(p);
    }
    if problems.is_empty() {
        Ok(out.finish())
    } else {
        Err(problems)
    }
}

/// Every element a form writes: the element, then each child's (depth first). Problems carry form paths
/// (`children.0.fields.name`).
///
/// # Errors
/// Every problem of the form and its children.
pub fn form_to_elements(
    form: &FormState,
    cx: &FormContext<'_>,
) -> Result<Vec<Element>, Vec<FormProblem>> {
    let mut elements = Vec::new();
    let mut problems = Vec::new();
    collect(form, "", cx, &mut elements, &mut problems);
    if problems.is_empty() {
        Ok(elements)
    } else {
        Err(problems)
    }
}

fn collect(
    form: &FormState,
    prefix: &str,
    cx: &FormContext<'_>,
    elements: &mut Vec<Element>,
    problems: &mut Vec<FormProblem>,
) {
    match form_to_element(form, cx) {
        Ok(e) => elements.push(e),
        Err(ps) => problems.extend(ps.into_iter().map(|p| prefixed(prefix, p))),
    }
    for (i, c) in form.children.iter().enumerate() {
        collect(c, &format!("{prefix}children.{i}."), cx, elements, problems);
    }
}
