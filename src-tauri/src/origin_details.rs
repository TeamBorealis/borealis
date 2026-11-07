//! B-191: Manage > Character ALIGNMENT and DEITY are the character's Alignment and Deity
//! selection slots, as in Aurora (its Manage pickers are `AlignmentContentViewModel` /
//! `DeityContentViewModel`, expanders of the level 1 `select type="Alignment"` and
//! `select type="Deity"` rules, and its sheet prints the selected element's name).
//!
//! Text that names an element the slot offers selects that element. Any other text (GitHub #30
//! made the deity combo editable; alignment text from older files) empties the slot and is kept
//! as custom text in the UI extras, which Manage and the sheet show when no element is chosen:
//! the chosen element wins over custom text.

use crate::builder::{apply, apply_filled, LOCKED_SLOT};
use crate::dto::DetailChoices;
use crate::error::{ApiError, ApiResult};
use crate::session::{Ctx, Draft};
use borealis_engine::commands;
use borealis_engine::snapshot::{CharacterSnapshot as Engine, SlotSnapshot};
use borealis_engine::state::Choice;

/// Element type of the alignment slot.
pub const ALIGNMENT: &str = "Alignment";
/// Element type of the deity slot.
pub const DEITY: &str = "Deity";

/// Older Borealis builds listed the element "Neutral" as "True Neutral" in Manage.
const NEUTRAL_ALIASES: [(&str, &str); 1] = [("true neutral", "neutral")];

/// The character's slot selecting elements of type `ty` (the level 1 rule), if the content has one.
fn origin_slot<'s>(snap: &'s Engine, ty: &str) -> Option<&'s SlotSnapshot> {
    snap.slots
        .iter()
        .find(|s| s.ty == ty && s.rejected.is_none())
}

/// Does typed `text` name element `name`? Case and outer spaces do not count.
fn names(text: &str, name: &str) -> bool {
    let (t, n) = (text.trim().to_lowercase(), name.trim().to_lowercase());
    t == n
        || NEUTRAL_ALIASES
            .iter()
            .any(|(alias, element)| t == *alias && n == *element)
}

/// The id of the element `text` names in `slot`: the chosen one first, then the offered options.
fn element_named(slot: &SlotSnapshot, text: &str) -> Option<String> {
    if let Some(c) = slot.chosen.as_ref().filter(|c| names(text, &c.name)) {
        return Some(c.id.clone());
    }
    slot.options
        .iter()
        .find(|o| names(text, &o.name))
        .map(|o| o.id.clone())
}

/// Apply Manage text for the `ty` slot: select the element it names, else empty the slot.
/// Returns the custom text to keep in the UI extras (empty when an element was selected).
///
/// # Errors
/// `invalid_argument` for a locked slot; engine errors of the selection.
pub fn bind(ctx: &Ctx<'_>, d: &mut Draft, ty: &str, text: &str) -> ApiResult<String> {
    let snap = ctx.hub.derive(&d.character.build);
    let Some(slot) = origin_slot(&snap, ty) else {
        return Ok(text.to_string());
    };
    let wanted = (!text.trim().is_empty())
        .then(|| element_named(slot, text))
        .flatten();
    let current = slot.chosen.as_ref().map(|c| c.id.as_str());
    if wanted.as_deref() != current && slot.locked {
        return Err(ApiError::invalid(LOCKED_SLOT));
    }
    let custom = if wanted.is_some() {
        String::new()
    } else {
        text.to_string()
    };
    match wanted {
        Some(id) if current == Some(id.as_str()) => {}
        Some(id) => {
            let out = commands::select_derived(
                &ctx.hub.engine,
                &d.character.build,
                &snap,
                &slot.key,
                Choice::Element(id),
            )?;
            apply_filled(ctx, d, out);
        }
        None if current.is_some() => {
            let out = commands::clear(&ctx.hub.engine, &d.character.build, &slot.key)?;
            apply(d, out);
        }
        None => {}
    }
    Ok(custom)
}

/// What Manage shows for the `ty` field: the chosen element's name, else the custom text.
#[must_use]
pub fn shown(snap: &Engine, ty: &str, custom: String) -> String {
    snap.origin.get(ty).map_or(custom, |e| e.name.clone())
}

/// Element names the slot of type `ty` offers (the chosen one included), unique, in slot order.
fn choice_names(snap: &Engine, ty: &str) -> Vec<String> {
    let Some(slot) = origin_slot(snap, ty) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    let chosen = slot.chosen.as_ref().map(|c| c.name.clone());
    for name in slot.options.iter().map(|o| o.name.clone()).chain(chosen) {
        if !out.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
            out.push(name);
        }
    }
    out
}

/// The Manage alignment and deity lists: the elements their slots offer.
#[must_use]
pub fn detail_choices(snap: &Engine) -> DetailChoices {
    DetailChoices {
        alignment: choice_names(snap, ALIGNMENT),
        deity: choice_names(snap, DEITY),
    }
}

#[cfg(test)]
mod tests {
    use super::names;

    #[test]
    fn typed_text_names_an_element_ignoring_case_spaces_and_the_old_neutral_label() {
        assert!(names(" lawful good ", "Lawful Good"));
        assert!(names("True Neutral", "Neutral"));
        assert!(!names("Neutral", "True Neutral"));
        assert!(!names("The Lantern Keeper", "Lawful Good"));
    }
}
