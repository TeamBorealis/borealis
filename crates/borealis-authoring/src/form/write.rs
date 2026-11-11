//! Form to elements.

use super::child_refs::{self, ChildInfo, ParentInfo};
use super::codec::{self, Codec};
use super::kinds::{kind, KindDef};
use super::{preserved, repeat};
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
    /// The parent, when this is a child written with it (a class feature knows its class).
    pub(crate) parent: Option<ParentInfo>,
    /// The form's children (for the fields that give them).
    pub(crate) children: Vec<ChildInfo>,
    /// A class's multiclass section id (`Memo::multiclass_id`).
    pub(crate) multiclass_id: Option<String>,
    /// A class's "first class only" requirement (`!<multiclass id>`), when it has a multiclass section.
    pub(crate) first_class_only: Option<String>,
    /// The spellcasting section, for a kind that has one (spec 4.6).
    pub(crate) caster: Option<super::codec::caster_cx::CasterCx>,
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
            parent: None,
            children: Vec::new(),
            multiclass_id: None,
            first_class_only: None,
            caster: None,
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
/// order written). A block's rules (and its copies at other levels) have no order: they are no field's neighbours.
struct Placed {
    origin: String,
    order: Option<(u8, usize)>,
    rule: Rule,
}

/// The rules a block writes, its copies at other levels too (`blocks.<i>`).
fn is_block(origin: &str) -> bool {
    origin.starts_with("blocks.")
}

/// The element's rules. Field rules go back to their stored positions and blocks, in order, to the positions left for
/// blocks (the `""` places of `layout`, or without one the positions no field rule holds); more blocks go last.
/// Unchanged, this is the stored order, and a field that writes fewer rules than it read leaves the others in their
/// stored order. A field whose memo is gone (its stored text forgotten) takes its places from `layout`. Then each new
/// rule, by rank and order written:
///
/// * a field (or block) that has rules placed already: after its last one (a darkvision range after its Vision grant,
///   a class feature added to the LEVELS grid after the others, a block's new copy after the block);
/// * else, before the first rule of the placed field next after it in rank and form order;
/// * else, after the last rule of the placed field before it;
/// * with no field rule placed at all, in front (so a new element's field rules lead, by rank).
fn merge_rules(
    blocks: Vec<(String, Rule)>,
    fields: Vec<FieldRule>,
    layout: &[String],
) -> (Vec<Rule>, Vec<RuleOrigin>) {
    let fields = with_layout_slots(fields, layout);
    let held: Vec<usize> = fields.iter().filter_map(|f| f.slot).collect();
    let (mut stored, mut fresh): (Vec<_>, Vec<_>) = fields
        .into_iter()
        .enumerate()
        .map(|(seq, f)| {
            let order = (f.rank, seq);
            let p = Placed {
                order: (!is_block(&f.origin)).then_some(order),
                origin: f.origin,
                rule: f.rule,
            };
            let key = f.slot.map_or((usize::from(order.0), seq), |s| (s, 0));
            (key, f.slot.is_some(), p)
        })
        .partition(|(_, is_stored, _)| *is_stored);
    let places = block_places(layout, &held);
    stored.extend(blocks.into_iter().enumerate().map(|(j, (origin, rule))| {
        // more blocks than places go last, in order
        let place = places.get(j).map_or((usize::MAX, j), |&p| (p, 0));
        let p = Placed {
            origin,
            order: None,
            rule,
        };
        (place, true, p)
    }));
    stored.sort_by_key(|(place, _, _)| *place);
    fresh.sort_by_key(|(order, _, _)| *order);
    let mut out: Vec<Placed> = stored.into_iter().map(|(_, _, p)| p).collect();
    for (_, _, p) in fresh {
        let at = new_rule_position(&out, &p);
        out.insert(at, p);
    }
    out.into_iter().map(|p| (p.rule, p.origin)).unzip()
}

/// The stored positions of block rules, in order: the positions no stored field rule (nor block copy) holds, among
/// the `""` places of `layout` or, without one, up to the last held position.
fn block_places(layout: &[String], held: &[usize]) -> Vec<usize> {
    let end = if layout.is_empty() {
        held.iter().max().map_or(0, |m| m + 1)
    } else {
        layout.len()
    };
    (0..end)
        .filter(|p| !held.contains(p) && layout.get(*p).is_none_or(String::is_empty))
        .collect()
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

/// Where a new rule goes among the rules placed so far (see [`merge_rules`]).
fn new_rule_position(out: &[Placed], p: &Placed) -> usize {
    let last_of = |origin: &str| out.iter().rposition(|q| q.origin == origin);
    if let Some(i) = last_of(&p.origin) {
        return i + 1;
    }
    let Some(order) = p.order else {
        return out.len();
    };
    let placed = || out.iter().filter_map(|q| q.order.map(|o| (o, q)));
    if let Some((_, next)) = placed().filter(|(o, _)| *o > order).min_by_key(|(o, _)| *o) {
        return out
            .iter()
            .position(|q| q.origin == next.origin)
            .unwrap_or(0);
    }
    placed()
        .max_by_key(|(o, _)| *o)
        .and_then(|(_, prev)| last_of(&prev.origin))
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
    write_traced(kind, form, None, cx).map(|(e, _)| e)
}

/// Where each rule of the element a form writes comes from (`blocks.<i>`, `fields.<key>`); `None` when the form
/// doesn't write.
pub(crate) fn rule_origins(form: &FormState, cx: &FormContext<'_>) -> Option<Vec<RuleOrigin>> {
    write_traced(kind(&form.kind)?, form, None, cx)
        .ok()
        .map(|(_, o)| o)
}

/// What a class knows before its fields write: its multiclass id and "first class only" (`!<id>`, only with a
/// multiclass section).
fn prepare_class(form: &FormState, out: &mut Out) {
    out.multiclass_id.clone_from(&form.memo.multiclass_id);
    let on = form
        .fields
        .get("multiclass")
        .or_else(|| form.memo.fields.get("multiclass").map(|m| &m.value))
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    out.first_class_only = if on {
        out.multiclass_id.as_ref().map(|id| format!("!{id}"))
    } else {
        None
    };
}

fn write_traced(
    kind: &KindDef,
    form: &FormState,
    parent: Option<&ParentInfo>,
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
    out.parent = parent.cloned();
    out.children = form
        .children
        .iter()
        .map(|c| ChildInfo {
            key: c.key.clone(),
            id: c.id().map(str::to_string),
        })
        .collect();
    if kind.id == super::kinds::CLASS_KIND {
        prepare_class(form, &mut out);
    }
    if kind.field("casts").is_some() {
        out.caster = Some(super::codec::caster_cx::context(form, parent));
    }
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
    let repeats = repeat::repeats(kind.id);
    for (i, b) in form.blocks.iter().enumerate() {
        let origin = format!("blocks.{i}");
        match cx.blocks.write(b, cx) {
            Ok(Some(rule)) => {
                let (copies, slots) = if repeats {
                    repeat::copies(b, &rule)
                } else {
                    (Vec::new(), None)
                };
                out.block_rules.push((origin.clone(), rule));
                match slots {
                    Some(slots) => {
                        out.field_rules(copies.into_iter().map(|r| (0, r)), &slots, &origin)
                    }
                    None => out
                        .block_rules
                        .extend(copies.into_iter().map(|r| (origin.clone(), r))),
                }
            }
            Ok(None) => {}
            Err(p) => problems.push(prefixed(&format!("blocks.{i}."), p)),
        }
    }
    if let Err(p) = preserved::restore(&form.preserved, &mut out) {
        problems.push(p);
    }
    problems.extend(child_refs::orphans(form));
    if !problems.is_empty() {
        return Err(problems);
    }
    let (mut element, origins) = out.finish();
    if kind.id == super::kinds::CLASS_KIND {
        super::codec::caster_cx::sync_class(&mut element, form);
    }
    if let Some((old, new)) = codec::class_renamed(form) {
        codec::rename_requirement_ids(&mut element, &old, &new);
    }
    if let Some(p) = parent {
        child_refs::patch_child(&mut element, form, p);
    }
    Ok((element, origins))
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
    collect(form, None, "", cx, &mut elements, &mut problems);
    if problems.is_empty() {
        Ok(elements)
    } else {
        Err(problems)
    }
}

fn collect(
    form: &FormState,
    parent: Option<&ParentInfo>,
    prefix: &str,
    cx: &FormContext<'_>,
    elements: &mut Vec<Element>,
    problems: &mut Vec<FormProblem>,
) {
    let written = match kind(&form.kind) {
        Some(k) => write_traced(k, form, parent, cx).map(|(e, _)| e),
        None => form_to_element(form, cx),
    };
    match written {
        Ok(e) => elements.push(e),
        Err(ps) => problems.extend(ps.into_iter().map(|p| prefixed(prefix, p))),
    }
    for (i, c) in form.children.iter().enumerate() {
        let info = child_refs::parent_info(form, c);
        collect(
            c,
            Some(&info),
            &format!("{prefix}children.{i}."),
            cx,
            elements,
            problems,
        );
    }
}
