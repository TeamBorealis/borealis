//! Form to elements.

use super::codec::{self, Codec};
use super::kinds::{kind, KindDef};
use super::preserved;
use super::{FormContext, FormProblem, FormState};
use borealis_content::{Element, Rule, Setter};
use std::collections::{BTreeMap, VecDeque};

/// Sort key of a setter or tag: its stored position, or after every stored one in the order written.
type SortKey = (usize, usize);

/// The element being written, with its setters and tags collected with their positions.
pub(crate) struct Out {
    pub(crate) element: Element,
    setters: Vec<(SortKey, Setter)>,
    supports: Vec<(SortKey, String)>,
    /// Rules fields write (a species' size and speed), with the key of the field.
    field_rules: Vec<(&'static str, Rule)>,
    /// The field being written.
    current: &'static str,
    seq: usize,
}

impl Out {
    fn new(element: Element) -> Self {
        Self {
            element,
            setters: Vec::new(),
            supports: Vec::new(),
            field_rules: Vec::new(),
            current: "",
            seq: 0,
        }
    }

    /// Add rules the field being written holds; [`arrange`] places them among the blocks' rules.
    pub(crate) fn rules(&mut self, items: impl IntoIterator<Item = Rule>) {
        let key = self.current;
        self.field_rules.extend(items.into_iter().map(|r| (key, r)));
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
        out.current = f.key;
        let path = format!("fields.{}", f.key);
        let memo = form.memo.fields.get(f.key);
        let Some(value) = form.fields.get(f.key).or(memo.map(|m| &m.value)) else {
            continue;
        };
        if let Err(p) = codec::write(f.codec, value, memo, &path, &mut out, cx) {
            problems.push(p);
        }
    }
    let mut block_rules = Vec::new();
    for (i, b) in form.blocks.iter().enumerate() {
        match cx.blocks.write(b, cx) {
            Ok(Some(rule)) => block_rules.push(rule),
            Ok(None) => {}
            Err(p) => problems.push(prefixed(&format!("blocks.{i}."), p)),
        }
    }
    let order: Vec<&'static str> = kind.fields.iter().map(|f| f.key).collect();
    let field_rules = std::mem::take(&mut out.field_rules);
    out.element.rules = arrange(&form.memo.layout, &order, field_rules, block_rules);
    if let Err(p) = preserved::restore(&form.preserved, &mut out) {
        problems.push(p);
    }
    if problems.is_empty() {
        Ok(out.finish())
    } else {
        Err(problems)
    }
}

/// The element's rules: the fields' and the blocks' in the stored order (`layout`, see [`super::Memo::layout`]).
///
/// Blocks fill the `""` places in their current order (more blocks go last). A field's rules take its places, and
/// rules it has more of follow its last place. A field with no place (set since the element was read) goes before the
/// next field in form order that has one, or else after the last; with no places at all, fields lead in form order.
pub(crate) fn arrange(
    layout: &[String],
    order: &[&'static str],
    field_rules: Vec<(&'static str, Rule)>,
    blocks: Vec<Rule>,
) -> Vec<Rule> {
    fn drain(by_field: &mut BTreeMap<&str, VecDeque<Rule>>, k: &str, out: &mut Vec<Rule>) {
        out.extend(by_field.remove(k).unwrap_or_default());
    }
    let mut by_field: BTreeMap<&str, VecDeque<Rule>> = BTreeMap::new();
    for (k, r) in field_rules {
        by_field.entry(k).or_default().push_back(r);
    }
    let first = |k: &str| layout.iter().position(|e| e == k);
    let last = |k: &str| layout.iter().rposition(|e| e == k);
    let mut before: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut waiting: Vec<&str> = Vec::new();
    let mut prev: Option<&str> = None;
    for k in order {
        if first(k).is_some() {
            before.entry(k).or_default().append(&mut waiting);
            prev = Some(k);
        } else {
            waiting.push(k);
        }
    }
    let mut out = Vec::new();
    // fields after the last placed one follow it; with nothing placed they lead
    let trailing = (prev, waiting);
    if trailing.0.is_none() {
        for k in &trailing.1 {
            drain(&mut by_field, k, &mut out);
        }
    }
    let mut blocks = blocks.into_iter();
    for (i, entry) in layout.iter().enumerate() {
        if entry.is_empty() {
            out.extend(blocks.next());
            continue;
        }
        if first(entry) == Some(i) {
            for k in before.get(entry.as_str()).into_iter().flatten() {
                drain(&mut by_field, k, &mut out);
            }
        }
        if last(entry) == Some(i) {
            drain(&mut by_field, entry, &mut out);
            if trailing.0 == Some(entry.as_str()) {
                for k in &trailing.1 {
                    drain(&mut by_field, k, &mut out);
                }
            }
        } else if let Some(r) = by_field
            .get_mut(entry.as_str())
            .and_then(VecDeque::pop_front)
        {
            out.push(r);
        }
    }
    out.extend(blocks);
    for k in order {
        drain(&mut by_field, k, &mut out);
    }
    out
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
