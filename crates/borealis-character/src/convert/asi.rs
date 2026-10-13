//! Ability Score Improvement levels, re-expressed as 2024 feat choices where possible.
//!
//! A 2014 ASI becomes the 2024 Ability Score Improvement feat in the matching feat slot; the
//! old ability allocation is recorded in a pending choice. A 2014 feat is mapped into the same
//! slot when the slot offers it.

use super::gear::GearCx;
use super::mapping::MappingIndex;
use super::preview::{Category, ConversionItem, Outcome};
use borealis_engine::commands;
use borealis_engine::snapshot::{CharacterSnapshot, SlotSnapshot};
use borealis_engine::{CharacterState, Choice, Engine};

const ASI_FEAT_MARK: &str = "ABILITY_SCORE_IMPROVEMENT";
const OLD_SLOT_PREFIX: &str = "Improvement Option (";

/// What a 2014 improvement level held.
enum Taken {
    Abilities(Vec<String>),
    Feat(String),
}

fn level_label(slot: &SlotSnapshot) -> &str {
    slot.name
        .strip_prefix(OLD_SLOT_PREFIX)
        .and_then(|r| r.strip_suffix(')'))
        .unwrap_or(&slot.name)
}

fn taken(old: &CharacterSnapshot, slot: &SlotSnapshot) -> Option<Taken> {
    let chosen = slot.chosen.as_ref()?;
    let inner: Vec<&str> = old
        .slots
        .iter()
        .filter(|s| s.owner.id == chosen.id)
        .filter_map(|s| s.chosen.as_ref().map(|c| c.id.as_str()))
        .collect();
    if chosen.id.contains("_FEAT_") {
        return inner.first().map(|f| Taken::Feat((*f).to_string()));
    }
    let abilities: Vec<String> = inner
        .iter()
        .filter_map(|i| i.strip_prefix("ID_INTERNAL_ASI_"))
        .map(str::to_lowercase)
        .collect();
    (!abilities.is_empty()).then_some(Taken::Abilities(abilities))
}

fn open_slot<'a>(snap: &'a CharacterSnapshot, name: &str) -> Option<&'a SlotSnapshot> {
    snap.slots
        .iter()
        .find(|s| s.name == name && s.chosen.is_none())
}

fn describe(abilities: &[String]) -> String {
    let mut parts: Vec<(String, u32)> = Vec::new();
    for a in abilities {
        match parts.iter_mut().find(|(n, _)| n == a) {
            Some((_, c)) => *c += 1,
            None => parts.push((a.clone(), 1)),
        }
    }
    parts
        .iter()
        .map(|(n, c)| format!("{n} +{c}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn carry(
    engine: &Engine,
    map: &MappingIndex,
    gear: &GearCx<'_>,
    old: &CharacterSnapshot,
    mut state: CharacterState,
    items: &mut Vec<ConversionItem>,
) -> CharacterState {
    for slot in old
        .slots
        .iter()
        .filter(|s| s.name.starts_with(OLD_SLOT_PREFIX))
    {
        let Some(what) = taken(old, slot) else {
            continue;
        };
        let label = level_label(slot).to_string();
        let new_name = format!("Ability Score Improvement ({label})");
        let snap = engine.derive(&state);
        let Some(target_slot) = open_slot(&snap, &new_name) else {
            items.push(ConversionItem {
                category: Category::Feat,
                label: new_name,
                outcome: Outcome::Dropped {
                    id: slot.key.owner.clone(),
                    reason: "the 2024 class has no improvement at this level".into(),
                },
            });
            continue;
        };
        let (target, from, summary) = match &what {
            Taken::Abilities(a) => (
                target_slot
                    .options
                    .iter()
                    .find(|o| o.id.contains(ASI_FEAT_MARK))
                    .map(|o| o.id.clone()),
                slot.chosen
                    .as_ref()
                    .map(|c| c.id.clone())
                    .unwrap_or_default(),
                Some(describe(a)),
            ),
            Taken::Feat(f) => {
                let t = map.get(f).map_or(f.as_str(), |t| t.id.as_str());
                (
                    target_slot
                        .options
                        .iter()
                        .find(|o| o.id == t)
                        .map(|o| o.id.clone()),
                    f.clone(),
                    None,
                )
            }
        };
        let placed = target.and_then(|t| {
            commands::select(engine, &state, &target_slot.key, Choice::Element(t.clone()))
                .ok()
                .map(|o| (t, o.state))
        });
        match placed {
            Some((to, next)) => {
                state = next;
                if let Some(abilities) = abilities_of(&what) {
                    let (filled, st) = fill_allocation(engine, state, &to, abilities);
                    state = st;
                    if !filled {
                        items.push(pending_allocation(&label, &summary.unwrap_or_default()));
                    }
                }
                items.push(ConversionItem {
                    category: Category::Feat,
                    label: new_name.clone(),
                    outcome: Outcome::Mapped {
                        from,
                        to,
                        kind: None,
                    },
                });
            }
            None => {
                items.push(ConversionItem {
                    category: Category::Feat,
                    label: gear.name_of(&from),
                    outcome: Outcome::Dropped {
                        id: from,
                        reason: "the 2024 improvement slot does not offer it".into(),
                    },
                });
            }
        }
    }
    state
}

fn pending_allocation(label: &str, text: &str) -> ConversionItem {
    ConversionItem {
        category: Category::AbilityScore,
        label: format!("Ability Score Improvement ({label})"),
        outcome: Outcome::NeedsChoice {
            what: format!("ability increase for {label} (was {text})"),
        },
    }
}

fn abilities_of(t: &Taken) -> Option<&[String]> {
    match t {
        Taken::Abilities(a) => Some(a),
        Taken::Feat(_) => None,
    }
}

/// `"Strength +1, Dexterity +1"` as a sorted multiset of lowercase ability names.
fn parse_entry(text: &str) -> Vec<(String, u32)> {
    let mut out: Vec<(String, u32)> = text
        .split(',')
        .filter_map(|part| {
            let (name, amount) = part.trim().rsplit_once(" +")?;
            Some((name.trim().to_lowercase(), amount.trim().parse().ok()?))
        })
        .collect();
    out.sort();
    out
}

fn allocation(abilities: &[String]) -> Vec<(String, u32)> {
    let mut out: Vec<(String, u32)> = Vec::new();
    for a in abilities {
        match out.iter_mut().find(|(n, _)| n == a) {
            Some((_, c)) => *c += 1,
            None => out.push((a.clone(), 1)),
        }
    }
    out.sort();
    out
}

/// Pick the list entry of the new ASI feat that equals the old allocation, if it exists.
fn fill_allocation(
    engine: &Engine,
    state: CharacterState,
    feat: &str,
    abilities: &[String],
) -> (bool, CharacterState) {
    let snap = engine.derive(&state);
    let wanted = allocation(abilities);
    let found = snap
        .slots
        .iter()
        .filter(|s| s.owner.id == feat && s.list_choice.is_none() && s.chosen.is_none())
        .find_map(|s| {
            s.list_items
                .iter()
                .find(|(_, t)| parse_entry(t) == wanted)
                .map(|(i, _)| (s.key.clone(), *i))
        });
    let Some((key, idx)) = found else {
        return (false, state);
    };
    match commands::select(engine, &state, &key, Choice::ListItem(idx)) {
        Ok(out) => (true, out.state),
        Err(_) => (false, state),
    }
}
