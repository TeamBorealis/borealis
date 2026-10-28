//! Version switch (B-55, `specs/version-rows-and-filters.md` 3): replace a slot's pick by
//! another version of the same row (same group key, [`crate::versions`]) in place, and choose
//! the old pick's sub-choices again where the new version offers them.

use super::selection::{find_slot, prune};
use super::{CommandError, CommandOutcome};
use crate::derive::{option_ref, Evaluation};
use crate::engine::Engine;
use crate::graph::{Graph, NodeId, Origin};
use crate::snapshot::OptionRef;
use crate::state::{CharacterState, Choice, Selection, SlotKey};
use crate::universe::UElem;
use crate::versions::{group_key, name_key};
use borealis_content::Rule;
use std::collections::HashSet;

/// Re-evaluations a switch makes at most while it re-applies nested sub-choices (one per level
/// of nesting; content nests a few levels deep).
const MAX_PASSES: usize = 16;

/// A switched pick: the new state and the titles of the sub-choices that did not fit.
#[derive(Debug, Clone, PartialEq)]
pub struct VersionSwitch {
    /// The resulting state.
    pub outcome: CommandOutcome,
    /// Rule names of the dropped sub-choice slots, in slot order, each once.
    pub dropped: Vec<String>,
}

fn group_of(e: &UElem) -> String {
    group_key(&e.el.ty, &e.el.name)
}

fn no_pick() -> CommandError {
    CommandError::Version("This selection has no choice to switch.".into())
}

/// The id of the slot's current pick.
fn current_pick(
    engine: &Engine,
    state: &CharacterState,
    key: &SlotKey,
) -> Result<String, CommandError> {
    engine.with_evaluation(state, |ev| {
        let (owner, si, _) = find_slot(&ev.graph, key).ok_or(CommandError::UnknownSlot)?;
        let chosen = ev.graph.nodes[owner].slots[si].chosen.ok_or_else(no_pick)?;
        Ok(ev.graph.nodes[chosen].e.el.id.clone())
    })
}

/// The versions of the slot's current pick, the pick first: what the slot would offer were the
/// pick (and what it brought) not there, among the options of the pick's group key. An
/// equivalent twin of the pick, which the duplicate check hides while the pick is held, is one.
///
/// # Errors
/// [`CommandError::UnknownSlot`], or [`CommandError::Version`] for an empty slot.
pub fn pick_versions(
    engine: &Engine,
    state: &CharacterState,
    key: &SlotKey,
) -> Result<Vec<OptionRef>, CommandError> {
    let current_id = current_pick(engine, state, key)?;
    let current = engine
        .universe(state.ruleset)
        .get(&current_id)
        .ok_or_else(|| CommandError::UnknownElement(current_id.clone()))?;
    let group = group_of(current);
    let mut cleared = state.clone();
    cleared.selections.retain(|s| &s.slot != key);
    let others = engine.with_evaluation(&cleared, |ev| {
        let (owner, si, _) = find_slot(&ev.graph, key).ok_or(CommandError::UnknownSlot)?;
        Ok::<_, CommandError>(
            ev.graph
                .slot_options(owner, si)
                .into_iter()
                .filter(|o| o.el.id != current_id && group_of(o) == group)
                .map(option_ref)
                .collect::<Vec<_>>(),
        )
    })?;
    Ok(std::iter::once(option_ref(current)).chain(others).collect())
}

/// Switch the slot's pick to `element_id`, another version of the same row: the pick is
/// replaced in place (same slot key), then every sub-choice of the old pick (its own selects
/// and those of what it granted or had chosen, recursively) is chosen again in the new
/// version's slot of the same rule name and ordinal when that slot offers the same element, or
/// an option of the same group key. The rest is dropped and reported. Pure: on an error the
/// caller keeps its state.
///
/// # Errors
/// [`CommandError::UnknownSlot`], [`CommandError::UnknownElement`], or
/// [`CommandError::Version`] for an empty slot or an element that is not a version of the pick.
pub fn switch_version(
    engine: &Engine,
    state: &CharacterState,
    key: &SlotKey,
    element_id: &str,
) -> Result<VersionSwitch, CommandError> {
    let uni = engine.universe(state.ruleset);
    let target = uni
        .get(element_id)
        .ok_or_else(|| CommandError::UnknownElement(element_id.to_string()))?;
    let current_id = current_pick(engine, state, key)?;
    if current_id == target.el.id {
        return Ok(VersionSwitch {
            outcome: CommandOutcome::new(state.clone()),
            dropped: Vec::new(),
        });
    }
    let current = uni
        .get(&current_id)
        .ok_or_else(|| CommandError::UnknownElement(current_id.clone()))?;
    if group_of(target) != group_of(current) {
        return Err(CommandError::Version(format!(
            "{} ({}) is not a version of {}.",
            target.el.name, target.el.source, current.el.name
        )));
    }
    if !pick_versions(engine, state, key)?
        .iter()
        .any(|o| o.id == target.el.id)
    {
        return Err(CommandError::Version(format!(
            "{} from {} is not offered for this selection.",
            target.el.name, target.el.source
        )));
    }
    let old = engine.with_evaluation(state, |ev| old_tree(ev, state, key));
    let mut next = state.clone();
    if let Some(sel) = next.selections.iter_mut().find(|s| &s.slot == key) {
        sel.choice = Choice::Element(target.el.id.clone());
    }
    let next = prune(engine, &next);
    let (state, dropped) = reapply(engine, next, key, &old);
    Ok(VersionSwitch {
        outcome: CommandOutcome::new(state),
        dropped,
    })
}

/// A stored sub-choice of the old pick.
struct OldSlot {
    name: String,
    ordinal: u32,
    /// The stored choice (an element by its canonical id).
    choice: Choice,
    /// Group key of a chosen element, normalized text of a chosen list entry.
    likeness: Option<String>,
    /// The node the choice registered, if any.
    chosen: Option<NodeId>,
}

/// A node of the old pick's subtree with its stored sub-choices.
struct OldNode {
    id: NodeId,
    parent: Option<NodeId>,
    selected: bool,
    slots: Vec<OldSlot>,
}

/// Live nodes under `root` (itself included) in pre-order.
fn subtree(graph: &Graph<'_>, root: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if graph.nodes[n].alive {
            out.push(n);
            stack.extend(graph.nodes[n].children.iter().rev().copied());
        }
    }
    out
}

/// The old pick's subtree with every stored choice of its slots, in slot order.
fn old_tree(ev: &Evaluation<'_>, state: &CharacterState, key: &SlotKey) -> Vec<OldNode> {
    let g = &ev.graph;
    let Some(root) = find_slot(g, key).and_then(|(o, si, _)| g.nodes[o].slots[si].chosen) else {
        return Vec::new();
    };
    subtree(g, root)
        .into_iter()
        .map(|n| {
            let node = &g.nodes[n];
            let slots = node
                .slots
                .iter()
                .filter_map(|slot| {
                    let Rule::Select(r) = &node.e.el.rules[slot.rule] else {
                        return None;
                    };
                    let stored = state.selections.iter().find(|s| s.slot == slot.key)?;
                    let chosen = slot.chosen.map(|c| g.nodes[c].e);
                    let (choice, likeness) = match (&stored.choice, chosen) {
                        (Choice::Element(_), Some(e)) => {
                            (Choice::Element(e.el.id.clone()), Some(group_of(e)))
                        }
                        (Choice::Element(id), None) => {
                            (stored.choice.clone(), ev.uni.get(id).map(group_of))
                        }
                        (Choice::ListItem(i), _) => (
                            stored.choice.clone(),
                            r.items
                                .iter()
                                .find(|x| x.id == *i)
                                .map(|x| name_key(&x.text)),
                        ),
                    };
                    Some(OldSlot {
                        name: r.name.clone(),
                        ordinal: slot.ordinal,
                        choice,
                        likeness,
                        chosen: slot.chosen,
                    })
                })
                .collect();
            OldNode {
                id: n,
                parent: node.parent,
                selected: matches!(node.origin, Origin::Selected(_)),
                slots,
            }
        })
        .collect()
}

/// Where an old sub-choice lands in the new version.
enum Placement {
    /// A slot both versions share already holds it.
    Kept(SlotKey),
    /// Choose `choice` in the open slot `key`.
    Choose(SlotKey, Choice),
}

/// Re-apply the old sub-choices pass by pass (a nested slot appears once its owner is chosen
/// again); returns the final state and the dropped slot names.
fn reapply(
    engine: &Engine,
    mut state: CharacterState,
    key: &SlotKey,
    old: &[OldNode],
) -> (CharacterState, Vec<String>) {
    let root = old.first().map(|n| n.id);
    let mut carried: HashSet<NodeId> = root.into_iter().collect();
    let mut done: HashSet<(NodeId, usize)> = HashSet::new();
    let mut claimed: HashSet<SlotKey> = HashSet::new();
    for _ in 0..MAX_PASSES {
        let placed = engine.with_evaluation(&state, |ev| {
            one_pass(ev, &state, key, old, &mut carried, &mut done, &mut claimed)
        });
        if placed.is_empty() {
            break;
        }
        state.selections.extend(placed);
        state = prune(engine, &state);
    }
    let mut dropped: Vec<String> = Vec::new();
    for node in old.iter().filter(|n| carried.contains(&n.id)) {
        for (i, slot) in node.slots.iter().enumerate() {
            if !done.contains(&(node.id, i)) && !dropped.contains(&slot.name) {
                dropped.push(slot.name.clone());
            }
        }
    }
    (state, dropped)
}

/// One evaluation's worth of re-applied sub-choices: the new selections to store.
fn one_pass(
    ev: &Evaluation<'_>,
    state: &CharacterState,
    key: &SlotKey,
    old: &[OldNode],
    carried: &mut HashSet<NodeId>,
    done: &mut HashSet<(NodeId, usize)>,
    claimed: &mut HashSet<SlotKey>,
) -> Vec<Selection> {
    let mut placed = Vec::new();
    let mut taken: HashSet<String> = HashSet::new();
    for node in old {
        let parent_carried = node.parent.is_some_and(|p| carried.contains(&p));
        if !carried.contains(&node.id) && (node.selected || !parent_carried) {
            continue;
        }
        carried.insert(node.id);
        for (i, slot) in node.slots.iter().enumerate() {
            if done.contains(&(node.id, i)) {
                continue;
            }
            let Some(at) = place(ev, state, key, slot, claimed, &taken) else {
                continue;
            };
            done.insert((node.id, i));
            carried.extend(slot.chosen);
            match at {
                Placement::Kept(k) => {
                    claimed.insert(k);
                }
                Placement::Choose(k, choice) => {
                    if let Choice::Element(id) = &choice {
                        taken.insert(id.clone());
                    }
                    claimed.insert(k.clone());
                    placed.push(Selection {
                        slot: k,
                        choice,
                        replace_level: None,
                    });
                }
            }
        }
    }
    placed
}

/// The first slot under the new pick with the old slot's rule name and ordinal that holds or
/// offers the old choice.
fn place(
    ev: &Evaluation<'_>,
    state: &CharacterState,
    key: &SlotKey,
    old: &OldSlot,
    claimed: &HashSet<SlotKey>,
    taken: &HashSet<String>,
) -> Option<Placement> {
    let g = &ev.graph;
    let (owner, si, _) = find_slot(g, key)?;
    let root = g.nodes[owner].slots[si].chosen?;
    for n in subtree(g, root) {
        for (sj, slot) in g.nodes[n].slots.iter().enumerate() {
            let Rule::Select(r) = &g.nodes[n].e.el.rules[slot.rule] else {
                continue;
            };
            if r.name != old.name || slot.ordinal != old.ordinal || claimed.contains(&slot.key) {
                continue;
            }
            if let Some(held) = state.selections.iter().find(|s| s.slot == slot.key) {
                if held.choice == old.choice {
                    return Some(Placement::Kept(slot.key.clone()));
                }
                continue;
            }
            if let Some(choice) = fitting_choice(g, n, sj, old, taken) {
                return Some(Placement::Choose(slot.key.clone(), choice));
            }
        }
    }
    None
}

/// The old choice, or an offered element of its group key (a list entry of the same text), if
/// the open slot `si` of node `n` offers it.
fn fitting_choice(
    g: &Graph<'_>,
    n: NodeId,
    si: usize,
    old: &OldSlot,
    taken: &HashSet<String>,
) -> Option<Choice> {
    let rule = g.nodes[n].slots[si].rule;
    match &old.choice {
        Choice::Element(id) => {
            let options = g.slot_options(n, si);
            if options.iter().any(|o| &o.el.id == id) {
                return Some(old.choice.clone());
            }
            let like = old.likeness.as_deref()?;
            options
                .iter()
                .find(|o| !taken.contains(&o.el.id) && group_of(o) == like)
                .map(|o| Choice::Element(o.el.id.clone()))
        }
        Choice::ListItem(_) => {
            let Rule::Select(r) = &g.nodes[n].e.el.rules[rule] else {
                return None;
            };
            let like = old.likeness.as_deref()?;
            r.items
                .iter()
                .find(|x| name_key(&x.text) == like && g.list_item_allowed(n, rule, x.id))
                .map(|x| Choice::ListItem(x.id))
        }
    }
}
