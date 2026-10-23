//! B-44: subclasses from an older edition on the classes of the universe's edition.
//!
//! The 2024 rules let a character take a subclass of its class from an older book, unless a
//! 2024 version of that subclass exists. The universe pass here, run once per universe whose
//! profile names a [`RulesetProfile::legacy_subclass_start`]:
//!
//! 1. Every class's subclass choices (`Archetype` selects on the class or on the class features
//!    it grants) are collected. A subclass belongs to the classes whose choice it fits (2014
//!    `Roguish Archetype`, 2024 `Rogue Subclass`, a 5etools class id) or that its `data.class`
//!    names. Classes are matched across editions by name, never by a list of class names.
//! 2. An older subclass is *reprinted* when a same-edition subclass of the same class has the
//!    same name (case and punctuation aside), maps to it (`maps_to`), maps to the same 2014
//!    subclass, or is named by its 5etools `data.reprinted_as`. A reprinted subclass is never
//!    offered for new picks (`superseded`); a stored pick still replays.
//! 3. Every other older subclass gets the support tags of the same-name classes' subclass
//!    choices, so they offer it, unless it already fits the choice or another subclass of the
//!    same name already does (the 5etools `_copy` of an older subclass for the 2024 class).
//! 4. Grants of an older subclass (or of its features) of a feature that a class feature of the
//!    class replaces ([`RulesetProfile::legacy_subclass_overlaps`], Blessed Strikes) require
//!    that the character does not have that class feature.
//!
//! The level shift of the subclass's features is per element, in the adapter
//! (`adapter::raise_subclass_levels`). Stored data is never rewritten (R4); every change is in
//! the compatibility notes.

use crate::adapter::{none_of, USED_AS_WRITTEN};
use crate::eval::{supports_match, NoDynamics};
use crate::profile::RulesetProfile;
use crate::universe::{Cond, EditionStatus, UElem, URule};
use borealis_content::expr::{BoolExpr, RequirementsExpr, SupportsExpr, SupportsLeaf};
use borealis_content::Rule;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

const SUBCLASS: &str = "Archetype";
const SUBCLASS_FEATURE: &str = "Archetype Feature";
const CLASS: &str = "Class";
const CLASS_FEATURE: &str = "Class Feature";
/// How deep grants are followed from a class (class -> feature -> feature) or a subclass.
const MAX_DEPTH: usize = 4;

/// A name compared without case, spaces or punctuation (`Way of the Open-Hand` = `way of the
/// open hand`).
#[must_use]
pub fn name_key(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// One subclass choice of a class.
struct Choice {
    /// Index of the class.
    class: usize,
    /// [`name_key`] of the class name.
    class_key: String,
    /// The class belongs to the universe's edition.
    native: bool,
    /// The choice's supports filter.
    expr: SupportsExpr,
}

/// What the pass decided for one older subclass.
#[derive(Default)]
struct Plan {
    reprinted: bool,
    /// Tags to add, with the class each is for.
    tags: Vec<String>,
    /// Same-edition classes whose choice offers the subclass (natively or through `tags`).
    classes: Vec<usize>,
}

/// What the planning step reads.
struct Ctx<'a> {
    elems: &'a [UElem],
    choices: &'a [Choice],
    /// Accepted subclasses, any edition.
    subclasses: &'a [usize],
    /// Class name keys of each subclass.
    class_of: HashMap<usize, HashSet<String>>,
    /// [`name_key`] of each subclass.
    keys: HashMap<usize, String>,
}

/// Run the pass (see the module docs). `elems` are the universe's elements, `pos` their index
/// by id.
pub(crate) fn adopt(
    elems: &mut [UElem],
    pos: &HashMap<String, usize>,
    profile: &dyn RulesetProfile,
) {
    if profile.legacy_subclass_start().is_none() {
        return;
    }
    let choices = subclass_choices(elems, pos);
    let subclasses: Vec<usize> = (0..elems.len())
        .filter(|&i| elems[i].el.ty == SUBCLASS && elems[i].edition.is_accepted())
        .collect();
    let ctx = Ctx {
        elems,
        choices: &choices,
        class_of: subclasses
            .iter()
            .map(|&i| (i, classes_of(elems, pos, &choices, i)))
            .collect(),
        keys: subclasses
            .iter()
            .map(|&i| (i, name_key(&elems[i].el.name)))
            .collect(),
        subclasses: &subclasses,
    };
    let plans: Vec<(usize, Plan)> = subclasses
        .iter()
        .filter(|&&i| elems[i].edition.is_foreign())
        .map(|&i| (i, plan(&ctx, i)))
        .collect();
    for (i, p) in plans {
        apply(elems, pos, profile, &choices, i, p);
    }
}

/// Every `Archetype` select of every class (on the class or a class feature it grants).
fn subclass_choices(elems: &[UElem], pos: &HashMap<String, usize>) -> Vec<Choice> {
    let mut out = Vec::new();
    for (class, u) in elems.iter().enumerate() {
        if u.el.ty != CLASS || u.derived {
            continue;
        }
        let native = !u.edition.is_foreign();
        let class_key = name_key(&u.el.name);
        for at in reachable(elems, pos, class, CLASS_FEATURE) {
            let e = &elems[at];
            for (rule, parsed) in e.el.rules.iter().zip(&e.rules) {
                if let (Rule::Select(s), URule::Select { supports, .. }) = (rule, parsed) {
                    if let (true, Cond::Expr(expr)) = (s.ty == SUBCLASS, supports) {
                        out.push(Choice {
                            class,
                            class_key: class_key.clone(),
                            native,
                            expr: expr.clone(),
                        });
                    }
                }
            }
        }
    }
    out
}

/// `root` and the elements of type `ty` its grants reach (breadth first, [`MAX_DEPTH`] deep).
fn reachable(elems: &[UElem], pos: &HashMap<String, usize>, root: usize, ty: &str) -> Vec<usize> {
    let mut out = vec![root];
    let mut seen: HashSet<usize> = out.iter().copied().collect();
    let mut frontier = vec![root];
    for _ in 0..MAX_DEPTH {
        let mut next = Vec::new();
        for &i in &frontier {
            for rule in &elems[i].el.rules {
                let Rule::Grant(g) = rule else { continue };
                if let Some(&j) = pos.get(&g.id) {
                    if elems[j].el.ty == ty && seen.insert(j) {
                        next.push(j);
                    }
                }
            }
        }
        out.extend(&next);
        frontier = next;
    }
    out
}

/// Class name keys a subclass belongs to: the classes whose choice it fits, and the class its
/// `data.class_name` / `data.class` names (a name, or the id of a class).
fn classes_of(
    elems: &[UElem],
    pos: &HashMap<String, usize>,
    choices: &[Choice],
    i: usize,
) -> HashSet<String> {
    let u = &elems[i];
    let mut out: HashSet<String> = choices
        .iter()
        .filter(|c| supports_match(&c.expr, u, &NoDynamics))
        .map(|c| c.class_key.clone())
        .collect();
    for field in ["class_name", "class"] {
        let Some(named) = u.el.data.get(field).and_then(Value::as_str) else {
            continue;
        };
        let name = pos.get(named).map_or(named, |&j| elems[j].el.name.as_str());
        let key = name_key(name);
        if !key.is_empty() {
            out.insert(key);
        }
    }
    out
}

/// Name keys an older subclass says it was reprinted as (5etools `reprintedAs`, kept by the
/// importer as `data.reprinted_as`: `Name|Class|ClassSource|Source` uids or plain names).
fn reprinted_as(u: &UElem) -> Vec<String> {
    u.el.data
        .get("reprinted_as")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|uid| name_key(uid.split('|').next().unwrap_or(uid)))
        .filter(|k| !k.is_empty())
        .collect()
}

/// Shortest reprint short name matched as the end of a subclass name (`Wild Heart` names
/// `Path of the Wild Heart`).
const MIN_SHORT_NAME: usize = 4;

/// Is native subclass `b` (name key `b_key`) a reprint of older subclass `a` (name key `a_key`,
/// reprint keys `reprints`), both of one class?
fn is_reprint(a: &UElem, a_key: &str, reprints: &[String], b: &UElem, b_key: &str) -> bool {
    let (ma, mb) = (a.el.maps_to.as_ref(), b.el.maps_to.as_ref());
    let same_2014 = ma
        .and_then(|m| m.dnd2014.as_ref())
        .is_some_and(|x| mb.and_then(|m| m.dnd2014.as_ref()) == Some(x));
    // 5etools reprint uids carry the short name (`Abjurer`, `Wild Heart`)
    let named = reprints
        .iter()
        .any(|k| k == b_key || (k.len() >= MIN_SHORT_NAME && b_key.ends_with(k.as_str())));
    a_key == b_key
        || mb.and_then(|m| m.dnd2014.as_deref()) == Some(a.el.id.as_str())
        || ma.and_then(|m| m.dnd2024.as_deref()) == Some(b.el.id.as_str())
        || same_2014
        || named
}

fn plan(ctx: &Ctx<'_>, a: usize) -> Plan {
    let elems = ctx.elems;
    let mine = &ctx.class_of[&a];
    let key = &ctx.keys[&a];
    let reprints = reprinted_as(&elems[a]);
    let reprinted = ctx.subclasses.iter().any(|&b| {
        b != a
            && !elems[b].edition.is_foreign()
            && !ctx.class_of[&b].is_disjoint(mine)
            && is_reprint(&elems[a], key, &reprints, &elems[b], &ctx.keys[&b])
    });
    if reprinted {
        return Plan {
            reprinted,
            ..Plan::default()
        };
    }
    let mut out = Plan::default();
    for c in ctx
        .choices
        .iter()
        .filter(|c| c.native && mine.contains(&c.class_key))
    {
        if supports_match(&c.expr, &elems[a], &NoDynamics) {
            out.classes.push(c.class);
            continue;
        }
        let twin_offered = ctx.subclasses.iter().any(|&b| {
            b != a && ctx.keys[&b] == *key && supports_match(&c.expr, &elems[b], &NoDynamics)
        });
        if twin_offered {
            continue;
        }
        if let Some(tags) = tags_for(c.expr.root()) {
            out.tags.extend(tags);
            out.classes.push(c.class);
        }
    }
    out
}

/// Tags that make a subclass fit a supports expression: a tag, the first fitting alternative of
/// an `||`, every positive operand of an `&&`. `None` when the expression needs anything else
/// (a negation, a placeholder).
fn tags_for(expr: &BoolExpr<SupportsLeaf>) -> Option<Vec<String>> {
    match expr {
        BoolExpr::Leaf(SupportsLeaf::Tag(t)) => Some(vec![t.clone()]),
        BoolExpr::Leaf(SupportsLeaf::Dynamic(_)) | BoolExpr::Not(_) => None,
        BoolExpr::Or(items) => items.iter().find_map(tags_for),
        BoolExpr::And(items) => {
            let mut out = Vec::new();
            for item in items.iter().filter(|x| !matches!(x, BoolExpr::Not(_))) {
                out.extend(tags_for(item)?);
            }
            Some(out)
        }
    }
}

fn apply(
    elems: &mut [UElem],
    pos: &HashMap<String, usize>,
    profile: &dyn RulesetProfile,
    choices: &[Choice],
    a: usize,
    p: Plan,
) {
    if p.reprinted {
        elems[a].superseded = true;
        add_note(
            &mut elems[a],
            "A 2024 version of this subclass is loaded: not offered for new picks".to_string(),
            false,
        );
        return;
    }
    let before = elems[a].supports.len();
    for t in p.tags {
        if !elems[a].supports.contains(&t) {
            elems[a].supports.push(t);
        }
    }
    // a tag set that still does not fit (an ID-range choice) is taken back
    let fits = |u: &UElem, c: &Choice| supports_match(&c.expr, u, &NoDynamics);
    let classes: Vec<usize> = p
        .classes
        .into_iter()
        .filter(|&c| choices.iter().any(|x| x.class == c && fits(&elems[a], x)))
        .collect();
    if classes.is_empty() {
        elems[a].supports.truncate(before);
        return;
    }
    if elems[a].supports.len() > before {
        let names: Vec<String> = classes.iter().map(|&c| elems[c].el.name.clone()).collect();
        add_note(
            &mut elems[a],
            format!(
                "Subclass from an older book, offered to the {} class",
                names.join(" and ")
            ),
            false,
        );
    }
    gate_overlaps(elems, pos, profile, a, &classes);
}

/// Rule 4: grants of replaced features under subclass `a` require that the character has none
/// of the replacing class features of `classes`.
fn gate_overlaps(
    elems: &mut [UElem],
    pos: &HashMap<String, usize>,
    profile: &dyn RulesetProfile,
    a: usize,
    classes: &[usize],
) {
    for overlap in profile.legacy_subclass_overlaps() {
        let wanted = name_key(overlap.class_feature);
        let mut ids: Vec<String> = classes
            .iter()
            .flat_map(|&c| reachable(elems, pos, c, CLASS_FEATURE))
            .filter(|&j| name_key(&elems[j].el.name) == wanted)
            .map(|j| elems[j].el.id.clone())
            .collect();
        ids.sort();
        ids.dedup();
        let Some(gate) = none_of(&ids) else { continue };
        let replaced: Vec<String> = overlap.replaces.iter().map(|n| name_key(n)).collect();
        let mut gated: Vec<String> = Vec::new();
        for at in reachable(elems, pos, a, SUBCLASS_FEATURE) {
            for k in 0..elems[at].el.rules.len() {
                let Rule::Grant(g) = &elems[at].el.rules[k] else {
                    continue;
                };
                let Some(&t) = pos.get(&g.id) else { continue };
                if !replaced.contains(&name_key(&elems[t].el.name)) {
                    continue;
                }
                let name = elems[t].el.name.clone();
                if gate_grant(&mut elems[at], k, &gate) && !gated.contains(&name) {
                    gated.push(name);
                }
            }
        }
        if !gated.is_empty() {
            add_note(
                &mut elems[a],
                format!(
                    "{} not gained with {}: the 2024 class feature replaces it",
                    gated.join(", "),
                    overlap.class_feature
                ),
                true,
            );
        }
    }
}

/// Add `gate` to the requirements of grant rule `k` (parsed copy included). False when the
/// combined expression does not parse (the rule is left as it was).
fn gate_grant(u: &mut UElem, k: usize, gate: &str) -> bool {
    let Rule::Grant(g) = &mut u.el.rules[k] else {
        return false;
    };
    let text = match g.requirements.as_deref().map(str::trim) {
        Some(r) if !r.is_empty() => format!("({r})&&{gate}"),
        _ => gate.to_string(),
    };
    let Ok(Some(expr)) = RequirementsExpr::parse(&text) else {
        return false;
    };
    g.requirements = Some(text);
    u.rules[k] = URule::Grant {
        req: Cond::Expr(expr),
    };
    true
}

/// Append a compatibility note; `converted` marks the element as changed by the adaptation.
fn add_note(u: &mut UElem, note: String, converted: bool) {
    if let EditionStatus::Foreign {
        notes,
        converted: c,
        ..
    } = &mut u.edition
    {
        notes.retain(|n| n != USED_AS_WRITTEN);
        notes.push(note);
        *c |= converted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_compare_without_case_or_punctuation() {
        assert_eq!(
            name_key("Way of the Open-Hand"),
            name_key("way of the open hand")
        );
        assert_ne!(name_key("Thief"), name_key("Thieves"));
    }

    #[test]
    fn tags_fit_simple_alternative_and_conjunctive_choices() {
        let tags = |s: &str| {
            let e = SupportsExpr::parse(s).expect("parses").expect("some");
            tags_for(e.root())
        };
        assert_eq!(tags("Rogue Subclass"), Some(vec!["Rogue Subclass".into()]));
        assert_eq!(tags("A||B"), Some(vec!["A".into()]));
        assert_eq!(tags("A,B"), Some(vec!["A".into(), "B".into()]));
        assert_eq!(tags("A,!B"), Some(vec!["A".into()]));
        assert_eq!(tags("!A"), None);
    }
}
