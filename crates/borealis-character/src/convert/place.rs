//! Carrying stored choices into the 2024 build, one slot at a time.

use super::mapping::MappingIndex;
use super::preview::{Category, ConversionItem, Outcome};
use borealis_engine::commands;
use borealis_engine::snapshot::{CharacterSnapshot, SlotSnapshot};
use borealis_engine::{CharacterState, Choice, Engine};

/// A stored 2014 choice waiting for a 2024 slot.
#[derive(Debug, Clone)]
pub(crate) struct OldPick {
    pub slot: SlotSnapshot,
    pub id: String,
    pub label: String,
    pub ty: String,
}

/// Slots handled by dedicated code instead of the generic carry-over.
pub(crate) fn is_special(slot: &SlotSnapshot) -> bool {
    slot.ty == "Sub Race"
        || slot.ty == "Language"
        || slot.name.starts_with("Improvement Option (")
        || slot.owner.id.starts_with("ID_INTERNAL_CLASS_FEATURE_ASI_")
        || slot.owner.id.starts_with("ID_INTERNAL_CLASS_FEATURE_FEAT_")
}

pub(crate) fn category_of(ty: &str) -> Category {
    match ty {
        "Race" | "Sub Race" | "Species" => Category::Species,
        "Background" => Category::Background,
        "Class" => Category::Class,
        "Archetype" => Category::Subclass,
        "Feat" => Category::Feat,
        "Spell" => Category::Spell,
        "Language" => Category::Language,
        "Weapon" | "Armor" | "Item" | "Magic Item" => Category::Item,
        _ => Category::Choice,
    }
}

fn base_name(name: &str) -> &str {
    name.split(" (").next().unwrap_or(name)
}

fn score(old: &SlotSnapshot, new: &SlotSnapshot) -> u8 {
    let same_owner = old.owner.id == new.owner.id;
    if same_owner && old.name == new.name && old.key.ordinal == new.key.ordinal {
        4
    } else if same_owner && old.name == new.name {
        3
    } else if base_name(&old.name) == base_name(&new.name) {
        2
    } else {
        1
    }
}

fn min_score(ty: &str) -> u8 {
    if matches!(ty, "Race" | "Class" | "Background") {
        3
    } else {
        1
    }
}

/// The best open 2024 slot that offers `target`, if any.
fn find_slot<'a>(
    snap: &'a CharacterSnapshot,
    pick: &OldPick,
    target: &str,
) -> Option<&'a SlotSnapshot> {
    let min = min_score(&pick.ty);
    let mut best: Option<(u8, &SlotSnapshot)> = None;
    for s in &snap.slots {
        if s.chosen.is_some()
            || s.list_choice.is_some()
            || !s.options.iter().any(|o| o.id == target)
        {
            continue;
        }
        let sc = score(&pick.slot, s);
        if sc >= min && !matches!(best, Some((b, _)) if sc <= b) {
            best = Some((sc, s));
        }
    }
    best.map(|(_, s)| s)
}

/// Collect the stored element choices of a 2014 character that the generic carry-over handles.
pub(crate) fn collect_picks(
    old: &CharacterSnapshot,
    names: &dyn Fn(&str) -> String,
) -> Vec<OldPick> {
    old.slots
        .iter()
        .filter(|s| !is_special(s))
        .filter_map(|s| {
            let chosen = s.chosen.as_ref()?;
            Some(OldPick {
                slot: s.clone(),
                id: chosen.id.clone(),
                label: names(&chosen.id),
                ty: chosen.ty.clone(),
            })
        })
        .collect()
}

/// Everything the placement loop works with.
pub(crate) struct Placer<'a> {
    pub engine: &'a Engine,
    pub map: &'a MappingIndex,
}

impl Placer<'_> {
    /// Try to place one pick; returns the new state and the report line on success.
    fn try_place(
        &self,
        state: &CharacterState,
        snap: &CharacterSnapshot,
        pick: &OldPick,
    ) -> Option<(CharacterState, ConversionItem)> {
        let mapped = self.map.get(&pick.id);
        let candidates = mapped
            .map(|t| t.id.as_str())
            .into_iter()
            .chain(std::iter::once(pick.id.as_str()));
        for target in candidates {
            let Some(slot) = find_slot(snap, pick, target) else {
                continue;
            };
            let Ok(out) = commands::select(
                self.engine,
                state,
                &slot.key,
                Choice::Element(target.to_string()),
            ) else {
                continue;
            };
            let cross = slot
                .options
                .iter()
                .find(|o| o.id == target)
                .is_some_and(|o| o.edition.is_some());
            let outcome = if target == pick.id {
                Outcome::Kept {
                    id: pick.id.clone(),
                    cross_edition: cross,
                }
            } else {
                Outcome::Mapped {
                    from: pick.id.clone(),
                    to: target.to_string(),
                    kind: mapped.and_then(|t| t.kind),
                }
            };
            let item = ConversionItem {
                category: category_of(&pick.ty),
                label: pick.label.clone(),
                outcome,
            };
            return Some((out.state, item));
        }
        None
    }

    /// Place every pick that fits; returns the picks that never found a slot.
    pub fn place_all(
        &self,
        mut state: CharacterState,
        mut pending: Vec<OldPick>,
        items: &mut Vec<ConversionItem>,
    ) -> (CharacterState, Vec<OldPick>) {
        loop {
            let snap = self.engine.derive(&state);
            let hit = pending
                .iter()
                .enumerate()
                .find_map(|(i, p)| self.try_place(&state, &snap, p).map(|r| (i, r)));
            let Some((i, (next, item))) = hit else { break };
            pending.remove(i);
            state = next;
            items.push(item);
        }
        (state, pending)
    }

    /// The reason a pick could not be carried over.
    pub fn reason(&self, pick: &OldPick) -> String {
        match self.map.get(&pick.id) {
            Some(t) => format!(
                "mapped to {} but the 2024 build has no open slot for it",
                t.id
            ),
            None => "no 2024 equivalent, and not allowed as a cross-edition pick here".to_string(),
        }
    }
}
