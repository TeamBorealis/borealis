//! Ids of spellcasting blocks (`ClassSpellcasting.classSlotId`).
//!
//! The Magic page keys its tabs and blocks by this id, prepare ticks send it back, and spell
//! selection slots name the block they feed with it. A class's block keeps its class slot id
//! (`main`, `multi-N`), which saved state and commands decode. Every other block (a feat's, an
//! item's, a subclass list like Eldritch Knight, Other Spells) gets an id of its own, so it never
//! shares the main class's `main`.

use crate::ids::encode_class_slot;
use borealis_engine::snapshot::{CharacterSnapshot as Engine, SlotSnapshot};
use borealis_engine::ClassSlot;

/// Prefix of the id of a spellcasting block that belongs to no class. It can never read as a
/// class slot id (`main`, `multi-N`).
pub const BLOCK_ID_PREFIX: &str = "caster-";

/// Class progression of the class named `name`, if the character has one.
fn class_slot_named(snap: &Engine, name: &str) -> Option<ClassSlot> {
    snap.classes
        .iter()
        .find(|k| k.class.as_ref().is_some_and(|e| e.name == name))
        .map(|k| k.slot.clone())
}

/// Lower-case `name` with every run of other characters as one `-` (the id is used in HTML ids
/// and ARIA references, which can't hold spaces).
fn slug(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            out.push(ch);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// The id of a block named `name` that belongs to no class, unique among `taken`.
fn block_id(name: &str, taken: &[String]) -> String {
    let stem = match slug(name) {
        s if s.is_empty() => format!("{BLOCK_ID_PREFIX}block"),
        s => format!("{BLOCK_ID_PREFIX}{s}"),
    };
    let mut id = stem.clone();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{stem}-{n}");
        n += 1;
    }
    id
}

/// `ClassSpellcasting.classSlotId` of every spellcasting block, in snapshot order: the class slot
/// id of a class's block, else `caster-<slug of the block name>`. Stable while the block exists
/// and unique (a clash takes `-2`, `-3`...).
#[must_use]
pub fn caster_ids(snap: &Engine) -> Vec<String> {
    let mut ids: Vec<String> = Vec::with_capacity(snap.spellcasting.len());
    for c in &snap.spellcasting {
        let id = match class_slot_named(snap, &c.name) {
            Some(slot) => encode_class_slot(&slot),
            None => block_id(&c.name, &ids),
        };
        ids.push(id);
    }
    ids
}

/// Name of the spellcasting block whose `ClassSpellcasting.classSlotId` is `id`.
#[must_use]
pub fn caster_name(snap: &Engine, id: &str) -> Option<String> {
    caster_ids(snap)
        .iter()
        .position(|x| x == id)
        .and_then(|i| snap.spellcasting.get(i))
        .map(|c| c.name.clone())
}

/// `ClassSpellcasting.classSlotId` of the spellcasting entry a spell selection slot feeds, if
/// that entry is listed on the Magic page.
#[must_use]
pub fn slot_caster_id(snap: &Engine, slot: &SlotSnapshot) -> Option<String> {
    let name = slot.spellcasting.as_deref()?;
    let at = snap.spellcasting.iter().position(|c| c.name == name)?;
    caster_ids(snap).into_iter().nth(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_keep_letters_and_digits_only() {
        assert_eq!(slug("Magic Initiate (Wizard)"), "magic-initiate-wizard");
        assert_eq!(
            slug("  Ring of Spell-Storing +1 "),
            "ring-of-spell-storing-1"
        );
        assert_eq!(slug("()"), "");
    }

    #[test]
    fn block_ids_never_read_as_class_slots_and_never_clash() {
        let taken = vec!["caster-other-spells".to_string()];
        assert_eq!(block_id("Other Spells", &[]), "caster-other-spells");
        assert_eq!(block_id("Other: Spells", &taken), "caster-other-spells-2");
        assert_eq!(block_id("main", &[]), "caster-main");
        assert_eq!(block_id("()", &[]), "caster-block");
        assert!(crate::ids::decode_class_slot(&block_id("multi-3", &[])).is_err());
    }
}
