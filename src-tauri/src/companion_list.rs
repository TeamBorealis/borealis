//! Every companion of a character (GitHub #13, decision U16(c)): the one content grants (a
//! `Companion` select, with the name and picture the Companion tab always had) and the ones the
//! user added by hand. Each carries the user's notes, traits and bonuses; the bonuses are added
//! to the creature's armor class, hit points and walking speed, and the user's traits follow the
//! creature's own.

use crate::companion::{creature_block, creature_stats};
use crate::companion_block;
use crate::content_dto::{CompanionFeature, CompanionState};
use crate::dto::{AbilityRecord, ElementRef};
use crate::extras::ui_extras;
use crate::snapshot_map::MapInput;
use borealis_character::{AddedCompanion, CompanionEdits};
use borealis_content::Element;
use borealis_engine::snapshot::CharacterSnapshot as Engine;

/// Id of the companion that content grants, next to the `companion-<n>` ids of added ones.
pub const GRANTED_ID: &str = "granted";

/// One companion before it is shown: where it comes from and the user's data on it.
struct Parts<'a> {
    id: String,
    granted: bool,
    element: Option<&'a Element>,
    creature: Option<ElementRef>,
    name: String,
    portrait: Option<String>,
    edits: &'a CompanionEdits,
}

/// A number with a bonus; a formula note (IM12a) gets the bonus written after it instead.
fn with_bonus(value: i32, note: Option<String>, bonus: i32) -> (i32, Option<String>) {
    match (note, bonus) {
        (note, 0) => (value, note),
        (Some(n), b) => (value, Some(format!("{n} {b:+}"))),
        (None, b) => (value.saturating_add(b), None),
    }
}

fn features(edits: &CompanionEdits) -> Vec<CompanionFeature> {
    edits
        .traits
        .iter()
        .map(|t| CompanionFeature {
            name: t.name.clone(),
            text: t.text.clone(),
        })
        .collect()
}

fn state_of(input: &MapInput<'_>, p: Parts<'_>) -> CompanionState {
    let stats = p.element.map(creature_stats).unwrap_or_default();
    let block = p
        .element
        .map(|e| creature_block(e, input))
        .unwrap_or_default();
    let skills = companion_block::skills(&block, &stats.abilities);
    let custom_traits = features(p.edits);
    let edits = p.edits;
    let (armor_class, armor_class_note) = with_bonus(
        stats.armor_class,
        stats.armor_class_note,
        edits.armor_class_bonus,
    );
    let (hp, hp_note) = with_bonus(stats.hp, stats.hp_note, edits.hit_points_bonus);
    let name = if p.name.is_empty() {
        p.creature
            .as_ref()
            .map(|e| e.name.clone())
            .unwrap_or_default()
    } else {
        p.name
    };
    CompanionState {
        id: p.id,
        granted: p.granted,
        name,
        creature: p.creature,
        portrait: p.portrait,
        armor_class,
        hp,
        armor_class_note,
        hp_note,
        challenge: stats.challenge,
        speed: stats.speed.saturating_add(edits.speed_bonus),
        abilities: AbilityRecord::from_fn(|i| stats.abilities[i]),
        proficiency: block.proficiency,
        initiative: block.initiative,
        build: block.build,
        fly_speed: block.speeds[0],
        climb_speed: block.speeds[1],
        swim_speed: block.speeds[2],
        burrow_speed: block.speeds[3],
        passive_perception: companion_block::passive_perception(&skills),
        skills,
        traits: block
            .traits
            .into_iter()
            .chain(custom_traits.clone())
            .collect(),
        actions: block.actions,
        reactions: block.reactions,
        notes: edits.notes.clone(),
        custom_traits,
        armor_class_bonus: edits.armor_class_bonus,
        hit_points_bonus: edits.hit_points_bonus,
        speed_bonus: edits.speed_bonus,
    }
}

/// The granted companion, when a companion creature is chosen or the user named, pictured or
/// edited one.
fn granted(input: &MapInput<'_>, snap: &Engine) -> Option<CompanionState> {
    let chosen = snap
        .slots
        .iter()
        .filter(|s| s.ty == "Companion")
        .find_map(|s| s.chosen.as_ref());
    let creature = chosen.map(|e| ElementRef {
        id: e.id.clone(),
        name: e.name.clone(),
        ty: "companion".to_string(),
        source: e.source.clone(),
    });
    let c = input.character;
    let portrait = ui_extras(c).companion_portrait;
    let name = c.details.companion_name.clone();
    let edits = &c.companions.granted;
    if creature.is_none() && portrait.is_none() && name.is_empty() && edits.is_empty() {
        return None;
    }
    let element = chosen.and_then(|e| input.hub.element(&e.id));
    Some(state_of(
        input,
        Parts {
            id: GRANTED_ID.to_string(),
            granted: true,
            element,
            creature,
            name,
            portrait,
            edits,
        },
    ))
}

/// An added companion. A creature whose content is no longer installed keeps its id as its name.
fn added(input: &MapInput<'_>, a: &AddedCompanion) -> CompanionState {
    let element = Some(a.creature.as_str())
        .filter(|id| !id.is_empty())
        .and_then(|id| input.hub.element(id));
    let creature = (!a.creature.is_empty()).then(|| ElementRef {
        id: a.creature.clone(),
        name: element.map_or_else(|| a.creature.clone(), |e| e.name.clone()),
        ty: "companion".to_string(),
        source: element.map(|e| e.source.clone()).unwrap_or_default(),
    });
    state_of(
        input,
        Parts {
            id: a.id.clone(),
            granted: false,
            element,
            creature,
            name: a.name.clone(),
            portrait: a.portrait.clone(),
            edits: &a.edits,
        },
    )
}

/// Every companion: the granted one first (when there is one), then the ones added by hand.
#[must_use]
pub fn companions(input: &MapInput<'_>, snap: &Engine) -> Vec<CompanionState> {
    let by_hand = input.character.companions.added.iter();
    granted(input, snap)
        .into_iter()
        .chain(by_hand.map(|a| added(input, a)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::with_bonus;

    #[test]
    fn a_bonus_adds_to_the_number_or_follows_the_formula() {
        assert_eq!(with_bonus(13, None, 0), (13, None));
        assert_eq!(with_bonus(13, None, 2), (15, None));
        assert_eq!(with_bonus(9, None, -3), (6, None));
        assert_eq!(
            with_bonus(0, Some("11 + the spell's level".into()), 1),
            (0, Some("11 + the spell's level +1".into()))
        );
        assert_eq!(with_bonus(i32::MAX, None, 5), (i32::MAX, None));
    }
}
