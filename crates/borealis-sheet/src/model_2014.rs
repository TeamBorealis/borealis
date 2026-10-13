//! 2014-only field set: race and sub-race, Inspiration (the 2024 Heroic Inspiration and armour
//! training ticks do not exist here).

use crate::model_ids::{self as id, FieldKind};
use crate::model_types::Fields2014;
use borealis_engine::CharacterSnapshot;

pub(crate) fn build(snap: &CharacterSnapshot) -> Fields2014 {
    let name = |k: &str| snap.origin.get(k).map(|e| e.name.clone());
    Fields2014 {
        race: name("Race").unwrap_or_default(),
        subrace: name("Sub Race"),
    }
}

/// Edition ids: `emit(id, kind, play)`.
pub(crate) fn fields(emit: &mut dyn FnMut(&str, FieldKind, bool)) {
    emit(id::RACE, FieldKind::Text, false);
    emit(id::SUBRACE, FieldKind::Text, false);
    emit(id::INSPIRATION, FieldKind::Check, true);
}
