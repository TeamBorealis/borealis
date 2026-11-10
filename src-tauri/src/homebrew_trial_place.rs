//! Where Try it's test character takes the draft (spec 9 "plus the draft in its slot"), after the build and after a
//! choice that dropped it: the first choice that offers it (looked at level by level), a feat at the first ability
//! score improvement, a spell through its class's lists, an item added and equipped, anything else as a test grant.

use crate::builder::{apply, apply_filled, select_in};
use crate::homebrew_trial_build::{levels, Note, Placement, TrialDraft, MAX_LEVEL};
use crate::session::{Ctx, Draft};
use crate::state::ContentHub;
use borealis_engine::added_picks::AddedKind;
use borealis_engine::commands::{self, Improvement};
use borealis_engine::snapshot::SlotSnapshot;
use borealis_engine::CharacterState;

const IMPROVEMENT_PREFIX: &str = "Improvement Option (";

/// Is the draft on the character (registered, or in the inventory)?
pub(crate) fn present(hub: &ContentHub, state: &CharacterState, draft: &TrialDraft) -> bool {
    if draft.placement == Placement::Item {
        return state
            .inventory
            .items
            .iter()
            .any(|i| i.item == draft.root.id);
    }
    hub.engine
        .derive_with(state, false)
        .elements
        .iter()
        .any(|e| draft.is_draft(&e.element.id))
}

/// Put the draft on the character when it is not there yet (after the build, and after a choice that dropped it).
/// A class tried as a second class that could not be taken was reported by [`second_class`] already.
pub(crate) fn place(
    ctx: &Ctx<'_>,
    d: &mut Draft,
    draft: &TrialDraft,
    as_second_class: bool,
    notes: &mut Vec<Note>,
) {
    if present(ctx.hub, &d.character.build, draft) {
        return;
    }
    let name = draft.name().to_string();
    let not_offered = |what: &str| {
        Note::new(
            1,
            "error",
            format!("The test character's {what} choice doesn't offer {name}."),
        )
    };
    match draft.placement {
        Placement::Class if as_second_class => {}
        Placement::Class => notes.push(not_offered("class")),
        Placement::Species => notes.push(not_offered("species")),
        Placement::Background => notes.push(not_offered("background")),
        Placement::Item => add_item(ctx, d, draft, notes),
        Placement::Subclass | Placement::SubSpecies => {
            if !in_a_slot(ctx, d, draft) {
                notes.push(Note::new(
                    MAX_LEVEL,
                    "error",
                    format!("No choice of the test character offers {name}: check what it supports against its parent's choice."),
                ));
            }
        }
        Placement::Feat => {
            if !in_a_slot(ctx, d, draft) && !as_feat(ctx, d, draft, notes) {
                grant(ctx, d, draft, notes);
            }
        }
        Placement::Spell => {
            if !in_a_slot(ctx, d, draft) && !as_added_spell(ctx, d, draft) {
                grant(ctx, d, draft, notes);
            }
        }
        Placement::Grant => {
            if !in_a_slot(ctx, d, draft) {
                grant(ctx, d, draft, notes);
            }
        }
        Placement::Creature => {
            in_a_slot(ctx, d, draft);
        }
    }
}

/// The first choice that offers the draft takes it: levels are looked at from 1 up, each with the options it has at
/// that level (a level-20 character offers a level-1 spell choice every spell it casts), an empty choice first.
fn in_a_slot(ctx: &Ctx<'_>, d: &mut Draft, draft: &TrialDraft) -> bool {
    let root = draft.root.id.as_str();
    let offers = |s: &&SlotSnapshot| {
        !s.locked
            && s.rejected.is_none()
            && s.chosen.as_ref().is_none_or(|c| c.id != root)
            && s.options
                .iter()
                .any(|o| o.id == root && !o.prerequisites_unmet)
    };
    for state in levels(ctx.hub, &d.character.build) {
        let snap = ctx.hub.derive(&state);
        let key = snap
            .slots
            .iter()
            .filter(offers)
            .min_by_key(|s| (s.gained_level.max(s.level), s.chosen.is_some()))
            .map(|s| s.key.clone());
        if let Some(key) = key {
            return select_in(ctx, d, &key, root).is_ok();
        }
    }
    false
}

/// `Improvement Option (Fighter 4)` -> ("Fighter", 4).
fn improvement_of(name: &str) -> Option<(String, u32)> {
    let inner = name.strip_prefix(IMPROVEMENT_PREFIX)?.strip_suffix(')')?;
    let (class, level) = inner.rsplit_once(' ')?;
    Some((class.to_string(), level.parse().ok()?))
}

/// The feat at the first ability score improvement (as `check::load_into_engine` takes feats). A feat whose
/// prerequisites the test character misses is reported and taken anyway (as with "Ignore feat prerequisites").
fn as_feat(ctx: &Ctx<'_>, d: &mut Draft, draft: &TrialDraft, notes: &mut Vec<Note>) -> bool {
    let engine = &ctx.hub.engine;
    let snap = engine.derive_with(&d.character.build, false);
    let Some((class, level)) = snap
        .slots
        .iter()
        .filter(|s| s.chosen.is_none())
        .filter_map(|s| improvement_of(&s.name))
        .min_by_key(|(_, level)| *level)
    else {
        return false;
    };
    let feat = Improvement::Feat(draft.root.id.clone());
    match commands::choose_improvement(engine, &d.character.build, &class, level, &feat) {
        Ok(o) => {
            apply_filled(ctx, d, o);
            return true;
        }
        Err(e) => notes.push(Note::new(
            level,
            "warning",
            format!(
                "The test character can't take {} at {class} {level}: {e}. Try it takes it anyway, as with \"Ignore \
                 feat prerequisites\".",
                draft.name()
            ),
        )),
    }
    d.character.build.house_rules.ignore_feat_prerequisites = true;
    match commands::choose_improvement(engine, &d.character.build, &class, level, &feat) {
        Ok(o) => {
            apply_filled(ctx, d, o);
            true
        }
        Err(_) => false,
    }
}

/// A spell on a spellcasting class's lists, added to that class as the builder's Add spell does: it applies from the
/// level the class casts its spell level (prepared casters have no choice that offers it).
fn as_added_spell(ctx: &Ctx<'_>, d: &mut Draft, draft: &TrialDraft) -> bool {
    let engine = &ctx.hub.engine;
    let casters: Vec<String> = engine
        .derive_with(&d.character.build, false)
        .spellcasting
        .iter()
        .map(|c| c.name.clone())
        .collect();
    for cast in casters {
        let added = commands::add_pick(
            engine,
            &d.character.build,
            AddedKind::Spell,
            &draft.root.id,
            Some(&cast),
        );
        if let Ok(o) = added {
            apply(d, o);
            return true;
        }
    }
    false
}

/// The test grant: the draft registered on the character from level 1, like a character option.
fn grant(ctx: &Ctx<'_>, d: &mut Draft, draft: &TrialDraft, notes: &mut Vec<Note>) {
    match commands::set_option(&ctx.hub.engine, &d.character.build, &draft.root.id, true) {
        Ok(o) => apply_filled(ctx, d, o),
        Err(e) => notes.push(Note::new(
            1,
            "error",
            format!("The test character can't be granted {}: {e}", draft.name()),
        )),
    }
}

/// An item: added, then equipped and attuned when it can be (a pack or a trinket is only carried).
fn add_item(ctx: &Ctx<'_>, d: &mut Draft, draft: &TrialDraft, notes: &mut Vec<Note>) {
    let engine = &ctx.hub.engine;
    match commands::add_item(engine, &d.character.build, &draft.root.id, 1) {
        Ok(o) => apply(d, o),
        Err(e) => {
            notes.push(Note::new(
                1,
                "error",
                format!("The test character can't carry {}: {e}", draft.name()),
            ));
            return;
        }
    }
    let Some(uid) = d
        .character
        .build
        .inventory
        .items
        .iter()
        .rev()
        .find(|i| i.item == draft.root.id)
        .map(|i| i.uid.clone())
    else {
        return;
    };
    // Equipping and attuning are attempts: an item that can't be worn or needs no attunement stays as it is.
    if let Ok(o) = commands::equip(engine, &d.character.build, &uid, None) {
        apply(d, o);
    }
    if let Ok(o) = commands::attune(engine, &d.character.build, &uid) {
        apply(d, o);
    }
}

/// A spell added through its class's lists stays on the character when it levels down (an added pick is checked when
/// it is added): below the first level the class may add it, it comes off again, so the ladder shows it from there.
pub(crate) fn trim_added_spell(
    hub: &ContentHub,
    states: &mut [CharacterState],
    draft: &TrialDraft,
) {
    if draft.placement != Placement::Spell {
        return;
    }
    let engine = &hub.engine;
    for state in states.iter_mut() {
        let snap = engine.derive_with(state, false);
        let Some(pick) = snap
            .added_picks
            .iter()
            .find(|p| p.kind == AddedKind::Spell && p.element.id == draft.root.id)
        else {
            return;
        };
        let Ok(without) = commands::remove_pick(engine, state, &pick.uid) else {
            return;
        };
        let cast = pick.spellcasting.as_deref();
        let allowed = engine
            .added_pick_options(&without.state, AddedKind::Spell, cast)
            .iter()
            .any(|o| o.id == draft.root.id && o.qualifies);
        if allowed {
            return;
        }
        *state = without.state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn improvement_slots_name_their_class_and_level() {
        assert_eq!(
            improvement_of("Improvement Option (Fighter 4)"),
            Some(("Fighter".to_string(), 4))
        );
        assert_eq!(
            improvement_of("Improvement Option (Eldritch Knight 12)"),
            Some(("Eldritch Knight".to_string(), 12))
        );
        assert_eq!(improvement_of("Fighting Style"), None);
    }
}
