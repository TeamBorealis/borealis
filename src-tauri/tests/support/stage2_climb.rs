//! The climb pass of an option host ([`super::option_host`]): when the needed ids alone offer
//! nothing, the host also takes what a user would take on the way down to the option: the
//! elements on its climb paths, the items that carry them, the character options they need, the
//! subclass whose own spell list names a spell, a new multiclass level for a multiclass entry.
//! Everything is read from the installed content; no id is named here.

use super::items;
use super::{Edition, NON_OFFERABLE};
use crate::support::plan::{tokens, Plan};
use borealis_content::{ContentStore, Rule};
use std::collections::BTreeSet;

/// Items one climb pass carries at most (a class-gated item is carried by every class's host;
/// the ones of other classes do nothing).
const MAX_CLIMB_ITEMS: usize = 24;

/// Element type of a class's multiclass entry (offered by a new multiclass level's select).
pub(super) const MULTICLASS_TYPE: &str = "Multiclass";

/// Configurations a climb pass tries at most.
const MAX_CLIMB_CONFIGS: usize = 6;

/// Items an option host's climb pass carries: items on the climb paths and items that grant
/// the option or a needed id outright, in id order, at most [`MAX_CLIMB_ITEMS`].
pub(super) fn climb_items(
    store: &ContentStore,
    target: &str,
    roots: &Plan,
    groups: &[Vec<String>],
) -> Vec<String> {
    let needed: BTreeSet<&str> = groups
        .iter()
        .flatten()
        .map(String::as_str)
        .chain(std::iter::once(target))
        .collect();
    let mut out: BTreeSet<String> = roots
        .path
        .iter()
        .filter(|p| store.element(p).is_some_and(|x| items::is_item_type(&x.ty)))
        .cloned()
        .collect();
    for se in store.iter() {
        let el = &se.element;
        let grants = items::is_item_type(&el.ty)
            && el.rules.iter().any(|r| {
                matches!(r, Rule::Grant(g) if needed.contains(g.id.as_str())
                    && g.requirements.as_deref().unwrap_or("").trim().is_empty())
            });
        if grants {
            out.insert(el.id.clone());
        }
    }
    out.into_iter().take(MAX_CLIMB_ITEMS).collect()
}

/// An element on a climb path that an option host picks where a slot offers it: not the class,
/// level and origin roots (taken first) and not an element type no select offers.
pub(super) fn on_climb(store: &ContentStore, id: &str) -> bool {
    store.element(id).is_some_and(|x| {
        !matches!(
            x.ty.as_str(),
            "Class" | "Level" | "Race" | "Species" | "Background" | "Sub Race" | "Race Variant"
        ) && !NON_OFFERABLE.iter().any(|(t, _)| *t == x.ty)
    })
}

/// The climb pass's configurations and roots for a spell: the elements whose own spell list
/// (or list extension) names the spell or one of its supports come first (a subclass with its
/// own list of fusion-element spells; the class is then the subclass's class), then `classes`.
/// Other targets keep `classes` and `roots`.
pub(super) fn spell_list_hosts(
    e: &Edition<'_>,
    target: &str,
    classes: Vec<(String, Option<String>)>,
    mut roots: Plan,
) -> (Vec<(String, Option<String>)>, Plan) {
    let store = &e.ins.store;
    let Some(spell) = store.element(target).filter(|x| x.ty == "Spell") else {
        return (classes, roots);
    };
    let keys: BTreeSet<String> = spell
        .supports
        .iter()
        .map(|s| s.to_lowercase())
        .chain(std::iter::once(target.to_lowercase()))
        .collect();
    let mut first: Vec<(String, Option<String>)> = Vec::new();
    let usable = |c: &String| e.builder_classes.contains(c);
    for se in store.iter() {
        let owner = &se.element;
        let Some(sc) = &owner.spellcasting else {
            continue;
        };
        let names = sc
            .list
            .iter()
            .chain(&sc.extends)
            .any(|l| tokens(&l.supports).iter().any(|t| keys.contains(t)));
        if !names {
            continue;
        }
        match owner.ty.as_str() {
            "Class" if usable(&owner.id) => first.push((owner.id.clone(), None)),
            _ => {
                let mut q = Plan::default();
                e.offerers.place(&owner.id, &mut q);
                let archetype = (owner.ty == "Archetype").then(|| owner.id.clone());
                for c in q.classes.iter().filter(|c| usable(c)) {
                    first.push((c.clone(), archetype.clone()));
                }
                for (a, cs) in &q.archetypes {
                    for c in cs.iter().filter(|c| usable(c)) {
                        first.push((c.clone(), Some(a.clone())));
                    }
                }
                roots.path.insert(owner.id.clone());
                roots.path.extend(q.path);
                roots.origins.extend(q.origins);
            }
        }
    }
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    for c in first.into_iter().chain(classes) {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out.truncate(MAX_CLIMB_CONFIGS);
    (out, roots)
}
