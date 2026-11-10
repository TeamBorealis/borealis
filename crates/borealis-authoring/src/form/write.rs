//! Form to elements.

use super::codec::{self, Codec};
use super::kinds::{kind, KindDef};
use super::preserved;
use super::{FormContext, FormProblem, FormState};
use borealis_content::{Element, Rule, Setter};

/// Sort key of a setter or tag: its stored position, or after every stored one in the order written.
type SortKey = (usize, usize);

/// The element being written, with its setters and tags collected with their positions.
pub(crate) struct Out {
    pub(crate) element: Element,
    setters: Vec<(SortKey, Setter)>,
    supports: Vec<(SortKey, String)>,
    seq: usize,
    /// Rules written by blocks, in block order (`blocks.<i>`).
    block_rules: Vec<(String, Rule)>,
    /// Rules written by fields: stored position (`None` = new), rank among new ones, origin (`fields.<key>`).
    field_rules: Vec<FieldRule>,
}

/// A rule a field writes (an armor's base AC, its stealth disadvantage).
struct FieldRule {
    slot: Option<usize>,
    rank: u8,
    origin: String,
    rule: Rule,
}

/// Where a rule of a written element comes from: `blocks.<i>` or `fields.<key>`.
pub(crate) type RuleOrigin = String;

impl Out {
    fn new(element: Element) -> Self {
        Self {
            element,
            setters: Vec::new(),
            supports: Vec::new(),
            seq: 0,
            block_rules: Vec::new(),
            field_rules: Vec::new(),
        }
    }

    /// Add rules a field writes: the `i`-th goes back to the stored position `slots[i]`; new ones go before every
    /// other rule, ordered by `rank` (the order the creator writes them in, so a canonical write keeps the order).
    pub(crate) fn field_rules(
        &mut self,
        items: impl IntoIterator<Item = (u8, Rule)>,
        slots: &[usize],
        origin: &str,
    ) {
        for (i, (rank, rule)) in items.into_iter().enumerate() {
            self.field_rules.push(FieldRule {
                slot: slots.get(i).copied(),
                rank,
                origin: origin.to_string(),
                rule,
            });
        }
    }

    /// The first setter written so far called `name` (any case), for a field that edits part of another field's
    /// setter (a cost's currency).
    pub(crate) fn setter_mut(&mut self, name: &str) -> Option<&mut Setter> {
        self.setters
            .iter_mut()
            .map(|(_, s)| s)
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// The value of the first setter written so far called `name`.
    pub(crate) fn setter_value(&self, name: &str) -> Option<&str> {
        self.setters
            .iter()
            .map(|(_, s)| s)
            .find(|s| s.name.eq_ignore_ascii_case(name))
            .map(|s| s.value.as_str())
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

    fn finish(mut self) -> (Element, Vec<RuleOrigin>) {
        self.setters.sort_by_key(|(k, _)| *k);
        self.supports.sort_by_key(|(k, _)| *k);
        self.element.setters = self.setters.into_iter().map(|(_, s)| s).collect();
        self.element.supports = self.supports.into_iter().map(|(_, t)| t).collect();
        let (rules, origins) = merge_rules(self.block_rules, self.field_rules);
        self.element.rules = rules;
        (self.element, origins)
    }
}

/// The element's rules: field rules back at their stored positions, blocks in order in the positions left, then the
/// new field rules in front (by rank). Unchanged, this is the stored order.
fn merge_rules(
    blocks: Vec<(String, Rule)>,
    fields: Vec<FieldRule>,
) -> (Vec<Rule>, Vec<RuleOrigin>) {
    let (mut fixed, mut fresh): (Vec<FieldRule>, Vec<FieldRule>) =
        fields.into_iter().partition(|f| f.slot.is_some());
    fixed.sort_by_key(|f| f.slot);
    fresh.sort_by_key(|f| f.rank);
    let mut out: Vec<(String, Rule)> = Vec::new();
    let mut fixed = fixed.into_iter().peekable();
    let mut blocks = blocks.into_iter();
    loop {
        if fixed.peek().is_some_and(|f| f.slot <= Some(out.len())) {
            let f = fixed.next().expect("peeked");
            out.push((f.origin, f.rule));
        } else if let Some(b) = blocks.next() {
            out.push(b);
        } else if let Some(f) = fixed.next() {
            out.push((f.origin, f.rule));
        } else {
            break;
        }
    }
    let all: Vec<(String, Rule)> = fresh
        .into_iter()
        .map(|f| (f.origin, f.rule))
        .chain(out)
        .collect();
    all.into_iter().map(|(o, r)| (r, o)).unzip()
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
    write_traced(kind, form, cx).map(|(e, _)| e)
}

/// Where each rule of the element a form writes comes from (`blocks.<i>`, `fields.<key>`); `None` when the form
/// doesn't write.
pub(crate) fn rule_origins(form: &FormState, cx: &FormContext<'_>) -> Option<Vec<RuleOrigin>> {
    write_traced(kind(&form.kind)?, form, cx)
        .ok()
        .map(|(_, o)| o)
}

fn write_traced(
    kind: &KindDef,
    form: &FormState,
    cx: &FormContext<'_>,
) -> Result<(Element, Vec<RuleOrigin>), Vec<FormProblem>> {
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
            Ok(Some(rule)) => out.block_rules.push((format!("blocks.{i}"), rule)),
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
