//! Entry to the typed sheet model: picks the edition and builds [`SheetData`](crate::model_types::SheetData).

use borealis_character::Character;
use borealis_engine::CharacterSnapshot;

/// Typed sheet model (see [`crate::model_types`]); the edition follows the snapshot's ruleset.
pub(crate) fn build_sheet_data(
    character: &Character,
    snap: &CharacterSnapshot,
    spell_facts: &crate::model_types::SpellFactsMap,
    edition: crate::SheetEdition,
) -> crate::model_types::SheetData {
    let edition = match edition {
        crate::SheetEdition::Auto => crate::model_common::edition_of(snap),
        crate::SheetEdition::Y2014 => crate::model_types::Edition::Dnd2014,
        crate::SheetEdition::Y2024 => crate::model_types::Edition::Dnd2024,
    };
    crate::model_common::build(character, snap, spell_facts, edition)
}
