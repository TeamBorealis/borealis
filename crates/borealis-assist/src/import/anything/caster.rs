//! Deterministic spellcasting builder for imports. The model only declares a caster profile
//! ([`SpellcastingOutline`]); this module reads it and hands it to the builder in
//! `borealis_authoring::caster` (shared with the homebrew creator), which emits the spellcasting node, the
//! slot-progression stats and the level-gated cantrip / spell rules in exactly the shape the bundled SRD classes use.

/// Drop the spellcasting rules a model wrote, so the built ones replace them.
pub use borealis_authoring::caster::is_built_caster_rule as is_model_caster_rule;
use borealis_authoring::caster::CasterProfile;
pub use borealis_authoring::caster::{slot_rows, BuiltCaster, CasterKind};

use super::types::{Edition, SpellcastingOutline};

/// Parse the free-text kind the model wrote.
pub fn caster_kind(p: &SpellcastingOutline) -> CasterKind {
    let k = p.progression.to_lowercase();
    if k.contains("none") || k.trim() == "no" {
        CasterKind::None
    } else if k.contains("point") || k.contains("pool") || k.contains("mana") {
        CasterKind::Points
    } else if k.contains("pact") {
        CasterKind::Pact
    } else if k.contains("third") || k.contains("1/3") {
        CasterKind::Third
    } else if k.contains("half") || k.contains("1/2") {
        CasterKind::Half
    } else {
        CasterKind::Full
    }
}

/// Is this feature name the class's spellcasting feature?
pub fn is_caster_feature(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("spellcasting") || n.contains("pact magic")
}

/// Sub-heading words of a spellcasting section ("Cantrips", "Spell Slots", "Spells Known",
/// "Preparing Spells", "Spellcasting Ability"): a feature with such parts is the class's
/// spellcasting feature whatever its own name.
const CASTER_PART_WORDS: [&str; 5] = [
    "cantrip",
    "spell slot",
    "spells known",
    "preparing",
    "spellcasting",
];

/// Is this a sub-heading of a spellcasting section?
pub(crate) fn is_caster_part(name: &str) -> bool {
    let n = name.to_lowercase();
    CASTER_PART_WORDS.iter().any(|w| n.contains(w))
}

/// Build everything the engine needs for the profile, or `None` when the class does not cast.
pub fn build_caster(
    p: &SpellcastingOutline,
    class_name: &str,
    edition: Edition,
) -> Option<BuiltCaster> {
    let profile = CasterProfile {
        kind: caster_kind(p),
        ability: p.ability.clone(),
        list: p.list.clone(),
        prepared: p.prepared,
        known: p.known,
        ritual: p.ritual,
        cantrips_by_level: p.cantrips_by_level.clone(),
        spells_known_by_level: p.spells_known_by_level.clone(),
        prepared_formula: p.prepared_formula.clone(),
    };
    borealis_authoring::caster::build_caster(&profile, class_name, edition)
}
