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
    /// The form's [`super::Memo::layout`]: the stored places of fields whose memo is gone.
    layout: Vec<String>,
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
            layout: Vec::new(),
        }
    }

    /// Add rules a field writes: the `i`-th goes back to the stored position `slots[i]`; new ones are placed by
    /// [`merge_rules`] (with no field rule stored, before every other rule, ordered by `rank`: the order the creator
    /// writes them in, so a canonical write keeps the order). Fields are written in form order, so equal ranks keep it.
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
        let (rules, origins) = merge_rules(self.block_rules, self.field_rules, &self.layout);
        self.element.rules = rules;
        (self.element, origins)
    }
}

/// A rule of the element being merged: its origin and, for a field's, its order among new rules (rank, then the
/// order written).
struct Placed {
    origin: String,
    order: Option<(u8, usize)>,
    rule: Rule,
}

/// The element's rules. Field rules go back to their stored positions and blocks, in order, to the positions `layout`
/// gives blocks (without a layout, the positions no field rule holds); more blocks go last. Unchanged, this is the
/// stored order, and a field that writes fewer rules than it read leaves the others in their stored order. A field
/// whose memo is gone (its stored text forgotten) takes its places from `layout`. Then each new field rule, by rank:
///
/// * a field that has rules placed already: after its last one (a darkvision range after its Vision grant);
/// * else, before the first rule of the placed field next after it in rank and form order;
/// * else, after the last rule of the placed field before it;
/// * with no field rule placed at all, in front (so a new element's field rules lead, by rank).
fn merge_rules(
    blocks: Vec<(String, Rule)>,
    fields: Vec<FieldRule>,
    layout: &[String],
) -> (Vec<Rule>, Vec<RuleOrigin>) {
    let fields = with_layout_slots(fields, layout);
    let (mut stored, mut fresh): (Vec<_>, Vec<_>) = fields
        .into_iter()
        .enumerate()
        .map(|(seq, f)| {
            let p = Placed {
                origin: f.origin,
                order: Some((f.rank, seq)),
                rule: f.rule,
            };
            (f.slot.map(|s| (s, 0)), p)
        })
        .partition(|(slot, _)| slot.is_some());
    let places = block_places(layout, &stored);
    stored.extend(blocks.into_iter().enumerate().map(|(j, (origin, rule))| {
        // more blocks than places go last, in order
        let place = places.get(j).map_or((usize::MAX, j), |&p| (p, 0));
        let p = Placed {
            origin,
            order: None,
            rule,
        };
        (Some(place), p)
    }));
    stored.sort_by_key(|(place, _)| *place);
    fresh.sort_by_key(|(_, p)| p.order);
    let mut out: Vec<Placed> = stored.into_iter().map(|(_, p)| p).collect();
    for (_, p) in fresh {
        let at = new_rule_position(&out, &p);
        out.insert(at, p);
    }
    out.into_iter().map(|p| (p.rule, p.origin)).unzip()
}

/// The stored positions of block rules, in order: the `""` places of `layout`, or without one the positions no stored
/// field rule holds (up to the last of them).
fn block_places(layout: &[String], fields: &[(Option<(usize, usize)>, Placed)]) -> Vec<usize> {
    if !layout.is_empty() {
        return (0..layout.len())
            .filter(|p| layout[*p].is_empty())
            .collect();
    }
    let held: Vec<usize> = fields
        .iter()
        .filter_map(|(slot, _)| slot.map(|s| s.0))
        .collect();
    let end = held.iter().max().map_or(0, |m| m + 1);
    (0..end).filter(|p| !held.contains(p)).collect()
}

/// Field rules with no slot take the stored places `layout` gives their field, when no rule holds them.
fn with_layout_slots(mut fields: Vec<FieldRule>, layout: &[String]) -> Vec<FieldRule> {
    if layout.is_empty() {
        return fields;
    }
    let mut used: Vec<usize> = fields.iter().filter_map(|f| f.slot).collect();
    for f in fields.iter_mut().filter(|f| f.slot.is_none()) {
        let key = f.origin.strip_prefix("fields.").unwrap_or(&f.origin);
        let place = (0..layout.len()).find(|p| layout[*p] == key && !used.contains(p));
        if let Some(p) = place {
            used.push(p);
            f.slot = Some(p);
        }
    }
    fields
}

/// Where a new field rule goes among the rules placed so far (see [`merge_rules`]).
fn new_rule_position(out: &[Placed], p: &Placed) -> usize {
    let last_of = |origin: &str| out.iter().rposition(|q| q.origin == origin);
    if let Some(i) = last_of(&p.origin) {
        return i + 1;
    }
    let placed = || out.iter().filter(|q| q.order.is_some());
    if let Some(next) = placed()
        .filter(|q| q.order > p.order)
        .min_by_key(|q| q.order)
    {
        return out
            .iter()
            .position(|q| q.origin == next.origin)
            .unwrap_or(0);
    }
    placed()
        .max_by_key(|q| q.order)
        .and_then(|prev| last_of(&prev.origin))
        .map_or(0, |i| i + 1)
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
    out.layout.clone_from(&form.memo.layout);
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
