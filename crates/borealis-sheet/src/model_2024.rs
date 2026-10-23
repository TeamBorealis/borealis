//! 2024-only field set: species and lineage, origin feat, armour training, Weapon Mastery,
//! Heroic Inspiration and the shield tick.

use crate::model_ids::{self as id, FieldKind};
use crate::model_text::armor_training;
use crate::model_types::{Fields2024, ProficiencyData};
use borealis_engine::CharacterSnapshot;

fn first_origin(snap: &CharacterSnapshot, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| snap.origin.get(*k))
        .map(|e| e.name.clone())
}

/// Species traits whose choice is the lineage the header shows (SRD 5.2.1 "Elven Lineage",
/// "Gnomish Lineage", "Fiendish Legacy").
const LINEAGE_SUFFIXES: [&str; 2] = ["Lineage", "Legacy"];

/// The option chosen in a species trait's lineage slot (`Species Trait Option`), when the
/// origin has no sub-species entry.
fn chosen_lineage(snap: &CharacterSnapshot) -> Option<String> {
    let species_trait = |ty: &str| matches!(ty, "Racial Trait" | "Species Trait");
    snap.slots
        .iter()
        .filter(|s| species_trait(&s.owner.ty))
        .filter(|s| LINEAGE_SUFFIXES.iter().any(|x| s.name.ends_with(x)))
        .find_map(|s| s.chosen.as_ref().map(|c| c.name.clone()))
}

pub(crate) fn build(snap: &CharacterSnapshot, prof: &ProficiencyData) -> Fields2024 {
    let background = snap.origin.get("Background").map(|e| e.name.as_str());
    let origin_feat = snap
        .features
        .iter()
        .find(|f| f.ty == "Feat" && f.granted_by.as_deref() == background && background.is_some())
        .map(|f| f.name.clone());
    let mut masteries: Vec<String> = Vec::new();
    for m in &snap.masteries {
        if !masteries.contains(&m.name) {
            masteries.push(m.name.clone());
        }
    }
    Fields2024 {
        species: first_origin(snap, &["Species", "Race"]).unwrap_or_default(),
        lineage: first_origin(snap, &["Sub Species", "Lineage", "Sub Race"])
            .or_else(|| chosen_lineage(snap)),
        origin_feat,
        armor_training: armor_training(&prof.armor),
        weapon_masteries: masteries,
    }
}

/// Edition ids: `emit(id, kind, play)`. `attacks` is the number of attack lines.
pub(crate) fn fields(f: &Fields2024, attacks: usize, emit: &mut dyn FnMut(&str, FieldKind, bool)) {
    for text in [id::SPECIES, id::LINEAGE, id::ORIGIN_FEAT] {
        emit(text, FieldKind::Text, false);
    }
    emit(id::HEROIC_INSPIRATION, FieldKind::Check, true);
    emit(id::AC_SHIELD, FieldKind::Check, false);
    for tick in id::ARMOR_TRAINING {
        emit(tick, FieldKind::Check, false);
    }
    for n in 1..=f.weapon_masteries.len() {
        emit(&id::mastery(n), FieldKind::Text, false);
    }
    for n in 1..=attacks {
        emit(&id::atk(n, "mastery"), FieldKind::Text, false);
    }
}
