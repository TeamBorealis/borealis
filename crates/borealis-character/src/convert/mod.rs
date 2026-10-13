//! The 2014 -> 2024 character conversion: a reviewable preview and a new copy.
//!
//! The original character is never touched. The copy gets a new id and a " (2024)" name suffix;
//! anything the 2024 build cannot take over is reported and left as an empty slot.

mod asi;
mod gear;
mod mapping;
mod place;
mod preview;

pub use mapping::{MappingIndex, Target};
pub use preview::{Category, ConversionItem, ConversionPreview, KeyStats, Outcome};

use crate::model::Character;
use borealis_content::{ContentStore, Ruleset};
use borealis_engine::commands;
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::{CharacterState, Engine};
use gear::GearCx;
use place::{category_of, collect_picks, Placer};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

/// Suffix appended to the copy's name.
pub const NAME_SUFFIX: &str = " (2024)";

/// Why a conversion could not start.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConvertError {
    /// Only 2014 characters can be converted; 2024 -> 2014 is not offered.
    #[error("only 2014 characters can be converted to 2024 (this one is {0})")]
    NotDnd2014(String),
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A fresh character id (unique per process and time; the library may replace it).
fn new_id(seed: &str) -> String {
    use std::hash::{Hash, Hasher};
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (seed, nanos, COUNTER.fetch_add(1, Ordering::Relaxed)).hash(&mut h);
    format!("c-{:016x}", h.finish())
}

/// Preview converting a 2014 character to 2024 (builds an engine over `store`).
///
/// # Errors
/// [`ConvertError::NotDnd2014`] for any other ruleset.
pub fn convert_to_2024(
    store: &ContentStore,
    character: &Character,
) -> Result<ConversionPreview, ConvertError> {
    convert_to_2024_with(store, &Engine::new(store), character)
}

/// Like [`convert_to_2024`] with an engine the caller already built over `store`.
///
/// # Errors
/// [`ConvertError::NotDnd2014`] for any other ruleset.
pub fn convert_to_2024_with(
    store: &ContentStore,
    engine: &Engine,
    character: &Character,
) -> Result<ConversionPreview, ConvertError> {
    if character.ruleset() != Ruleset::Dnd2014 {
        return Err(ConvertError::NotDnd2014(
            character.ruleset().as_str().into(),
        ));
    }
    let map = MappingIndex::build(store);
    let old = &character.build;
    let old_snap = engine.derive(old);
    let gear = GearCx {
        store,
        engine,
        map: &map,
    };
    let placer = Placer { engine, map: &map };
    let mut items = Vec::new();

    let mut state = base_state(engine, old);
    let picks = collect_picks(&old_snap, &|id| gear.name_of(id));
    let (state_after, unplaced) = placer.place_all(state, picks, &mut items);
    state = state_after;
    for p in &unplaced {
        items.push(ConversionItem {
            category: category_of(&p.ty),
            label: p.label.clone(),
            outcome: Outcome::Dropped {
                id: p.id.clone(),
                reason: placer.reason(p),
            },
        });
    }
    state = asi::carry(engine, &map, &gear, &old_snap, state, &mut items);
    state = gear.carry(old, state, &mut items);
    let new_snap = engine.derive(&state);
    folded_subraces(&old_snap, &new_snap, &gear, &mut items);
    language_items(&old_snap, &mut items);
    ability_items(&old_snap, &new_snap, &mut items);
    pending_items(&new_snap, &mut items);
    items.sort_by_key(|i| order(i.category));

    let converted = build_copy(character, state);
    Ok(ConversionPreview {
        items,
        before: KeyStats::from_snapshot(&old_snap),
        after: KeyStats::from_snapshot(&new_snap),
        converted,
    })
}

/// The converted copy of a preview (a new character; nothing is saved here).
#[must_use]
pub fn apply_conversion(preview: &ConversionPreview) -> Character {
    preview.converted().clone()
}

fn base_state(engine: &Engine, old: &CharacterState) -> CharacterState {
    let mut s = commands::new_character(engine, Ruleset::Dnd2024);
    s.levels = old.levels.clone();
    s.experience = old.experience;
    s.abilities = old.abilities.clone();
    s.hp_rolls = old.hp_rolls.clone();
    s.house_rules = old.house_rules.clone();
    s.allow_cross_edition = old.allow_cross_edition;
    s
}

fn build_copy(original: &Character, state: CharacterState) -> Character {
    let mut copy = original.clone();
    copy.build = state;
    copy.meta.converted_from = Some(original.meta.id.clone()).filter(|i| !i.is_empty());
    copy.meta.id = new_id(&original.meta.name);
    copy.meta.name = format!("{}{NAME_SUFFIX}", original.meta.name);
    copy
}

fn order(c: Category) -> u8 {
    match c {
        Category::Species => 0,
        Category::Background => 1,
        Category::Class => 2,
        Category::Subclass => 3,
        Category::AbilityScore => 4,
        Category::Feat => 5,
        Category::Spell => 6,
        Category::Item => 7,
        Category::Language => 8,
        Category::Choice => 9,
    }
}

/// Subraces fold into the species they belong to.
fn folded_subraces(
    old: &CharacterSnapshot,
    new: &CharacterSnapshot,
    gear: &GearCx<'_>,
    items: &mut Vec<ConversionItem>,
) {
    for slot in old.slots.iter().filter(|s| s.ty == "Sub Race") {
        let Some(sub) = &slot.chosen else { continue };
        let species = new
            .slots
            .iter()
            .find(|s| s.ty == "Race")
            .and_then(|s| s.chosen.as_ref());
        let outcome = match species {
            Some(species) => Outcome::Mapped {
                from: sub.id.clone(),
                to: species.id.clone(),
                kind: Some(borealis_content::MapKind::Approximate),
            },
            None => Outcome::Dropped {
                id: sub.id.clone(),
                reason: "its race has no 2024 species".into(),
            },
        };
        items.push(ConversionItem {
            category: Category::Species,
            label: gear.name_of(&sub.id),
            outcome,
        });
    }
}

/// Languages become free choices.
fn language_items(old: &CharacterSnapshot, items: &mut Vec<ConversionItem>) {
    for slot in old.slots.iter().filter(|s| s.ty == "Language") {
        let Some(l) = &slot.chosen else { continue };
        items.push(ConversionItem {
            category: Category::Language,
            label: l.name.clone(),
            outcome: Outcome::Dropped {
                id: l.id.clone(),
                reason: "2024 languages are free choices; pick them again".into(),
            },
        });
    }
}

/// Racial increases go away; the background distributes the bonus.
fn ability_items(
    old: &CharacterSnapshot,
    new: &CharacterSnapshot,
    items: &mut Vec<ConversionItem>,
) {
    if old.origin.contains_key("Race") {
        items.push(ConversionItem {
            category: Category::AbilityScore,
            label: "Racial ability score increases".into(),
            outcome: Outcome::Dropped {
                id: "racial_ability_score_increases".into(),
                reason: "2024 species grant no ability score increases".into(),
            },
        });
    }
    if let Some(bg) = new.origin.get("Background") {
        items.push(ConversionItem {
            category: Category::AbilityScore,
            label: format!("{} ability scores", bg.name),
            outcome: Outcome::NeedsChoice {
                what: "background ability score distribution (+2/+1 or +1/+1/+1)".into(),
            },
        });
    }
}

/// Every empty required slot of the 2024 build is a pending choice.
fn pending_items(new: &CharacterSnapshot, items: &mut Vec<ConversionItem>) {
    for s in new.slots.iter().filter(|s| {
        s.chosen.is_none()
            && s.requires_selection
            && !s.name.starts_with("Ability Scores")
            && (!s.options.is_empty() || !s.list_items.is_empty())
    }) {
        let category = category_of(&s.ty);
        let what = if s.owner.name == s.name || s.owner.id.starts_with("ID_LEVEL") {
            s.name.clone()
        } else {
            format!("{} ({})", s.name, s.owner.name)
        };
        let exists = items
            .iter()
            .any(|i| matches!(&i.outcome, Outcome::NeedsChoice { what: w } if *w == what));
        if !exists {
            items.push(ConversionItem {
                category,
                label: s.name.clone(),
                outcome: Outcome::NeedsChoice { what },
            });
        }
    }
    let subclass_dropped = items
        .iter()
        .any(|i| i.category == Category::Subclass && matches!(i.outcome, Outcome::Dropped { .. }));
    let subclass_pending = items.iter().any(|i| {
        i.category == Category::Subclass && matches!(i.outcome, Outcome::NeedsChoice { .. })
    });
    if subclass_dropped && !subclass_pending {
        items.push(ConversionItem {
            category: Category::Subclass,
            label: "Subclass".into(),
            outcome: Outcome::NeedsChoice {
                what: "subclass (no 2024 equivalent for the previous one)".into(),
            },
        });
    }
}
