//! The spellcasting section (spec 4.6): its fields, on a SPELLCASTING sub-tab of the forms that can give
//! spellcasting (a class's Spellcasting feature, which the class form's SPELLCASTING sub-tab edits; a feat). Subclass
//! forms (WB-18) add [`caster_fields`] the same way.

use super::super::codec::{CasterField, Codec};
use super::{field, Control, FieldDef, ONLY_2024};
use borealis_content::Ruleset;

/// The sub-tab id.
pub(crate) const SPELLCASTING: &str = "spellcasting";

const ONLY_2014: &[Ruleset] = &[Ruleset::Dnd2014];

const ABILITIES: &[(&str, &str)] = &[
    ("Strength", "Strength"),
    ("Dexterity", "Dexterity"),
    ("Constitution", "Constitution"),
    ("Intelligence", "Intelligence"),
    ("Wisdom", "Wisdom"),
    ("Charisma", "Charisma"),
];

const PREPARATION: &[(&str, &str)] = &[
    ("prepared", "Prepares spells from its whole list"),
    ("spellbook", "Prepares spells from a spellbook"),
    ("known", "Knows a number of spells"),
];

/// The caster types (spec 4.6), as `crate::caster::section::SlotTable` keys.
pub(crate) const CASTER_TYPES: &[(&str, &str)] = &[
    ("", "No spell slots"),
    ("full", "Full caster"),
    ("halfUp", "Half caster (round up)"),
    ("halfDown", "Half caster (round down)"),
    ("third", "Third caster"),
    ("pact", "Pact magic"),
    ("custom", "Custom table"),
];

const MULTICLASS: &[(&str, &str)] = &[
    ("auto", "As its caster type"),
    ("full", "Full caster"),
    ("halfUp", "Half caster (round up)"),
    ("half", "Half caster (round down)"),
    ("thirdUp", "Third caster (round up)"),
    ("third", "Third caster (round down)"),
    ("fourthUp", "Quarter caster (round up)"),
    ("fourth", "Quarter caster (round down)"),
    ("fifth", "Fifth caster"),
    ("solo", "Its own slots, not pooled"),
    ("none", "Not at all"),
];

const PREPARE_FORMULA: &[(&str, &str)] = &[
    ("level", "Ability modifier + class level"),
    ("half", "Ability modifier + half the class level"),
    ("", "By the table below"),
];

const PREPARED_CHANGE: &[(&str, &str)] = &[
    ("", "Not stated"),
    ("longRestAny", "After a long rest, any of them"),
    ("longRestOne", "After a long rest, one of them"),
    ("levelOne", "On gaining a level, one of them"),
];

fn caster(f: CasterField) -> Codec {
    Codec::Caster(f)
}

/// The section's fields, in the order they write (the node first, then its rules).
pub(crate) fn caster_fields() -> Vec<FieldDef> {
    let main = "Spellcasting";
    let slots = "Spell slots";
    let counts = "Spells by level";
    vec![
        field("casts", "Gives spellcasting", SPELLCASTING, Control::Check, caster(CasterField::Casts))
            .section(main)
            .default("false"),
        field("casterName", "Spellcasting name", SPELLCASTING, Control::Text, caster(CasterField::Name))
            .section(main)
            .expert()
            .default("\"\"")
            .help("The name its spells, slots and choices are counted under. Left empty: the class's name."),
        field("ability", "Spellcasting ability", SPELLCASTING, Control::Combo, caster(CasterField::Ability))
            .section(main)
            .choices(ABILITIES, false)
            .default("\"\""),
        field("preparation", "Spells", SPELLCASTING, Control::Combo, caster(CasterField::Preparation))
            .section(main)
            .choices(PREPARATION, false)
            .default("\"prepared\""),
        field("spellList", "Spell list", SPELLCASTING, Control::Text, caster(CasterField::List))
            .section(main)
            .default("\"\"")
            .help("Spells on this list are its spells (a spell's Lists). Left empty: the class's name."),
        field("swap", "Swap one spell per level", SPELLCASTING, Control::Check, caster(CasterField::Swap))
            .section(main)
            .default("false")
            .help("On gaining a level, one spell it knows can be replaced with another."),
        field("casterType", "Caster type", SPELLCASTING, Control::Combo, caster(CasterField::Type))
            .section(slots)
            .choices(CASTER_TYPES, false)
            .default("\"\""),
        field("slotTable", "Spell slots by level", SPELLCASTING, Control::SlotTable, caster(CasterField::Slots))
            .section(slots)
            .default("[]"),
        field(
            "multiclassSlots",
            "Counts for multiclass spell slots as",
            SPELLCASTING,
            Control::Combo,
            caster(CasterField::Multiclass),
        )
        .section(slots)
        .choices(MULTICLASS, false)
        .default("\"auto\""),
        field("prepared", "Prepared spells", SPELLCASTING, Control::LevelCounts, caster(CasterField::Prepared))
            .section(counts)
            .default("[]"),
        field(
            "prepareFormula",
            "Prepared spells",
            SPELLCASTING,
            Control::Combo,
            caster(CasterField::PrepareFormula),
        )
        .section(counts)
        .editions(ONLY_2014)
        .choices(PREPARE_FORMULA, false)
        .default("\"\""),
        field("cantrips", "Cantrips known", SPELLCASTING, Control::LevelCounts, caster(CasterField::Cantrips))
            .section(counts)
            .default("{\"levels\":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],\"statistic\":false}"),
        field("known", "Spells known", SPELLCASTING, Control::LevelCounts, caster(CasterField::Known))
            .section(counts)
            .default("{\"levels\":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],\"statistic\":true}"),
        field(
            "preparedChange",
            "Changing prepared spells",
            SPELLCASTING,
            Control::Combo,
            caster(CasterField::PreparedChange),
        )
        .section(counts)
        .editions(ONLY_2024)
        .choices(PREPARED_CHANGE, false)
        .default("\"\""),
    ]
}
