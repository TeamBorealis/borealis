//! The species forms (spec 4.4): the 2024 Species, the 2014 Race, Sub Race and Race Variant, and their child kinds
//! Racial Trait and Species Trait Option.
//!
//! A species' size, speeds, senses and traits (and, in 2014, its fixed ability score increases and languages) are
//! fields that write rules; the rest of what it gives (a lineage choice, a subrace choice, a trait from a later level)
//! is its rule blocks. A Sub Race or Race Variant names its race by the tag the race's choice offers it by. Traits and
//! lineage options are made inside their parent (child kinds).

use super::super::codec::{
    CastingField, Codec, Movement, NameKind, Sense, SpeciesField, ABILITY_CHOICES,
};
use super::{
    field, head, tail, Control, FieldDef, KindDef, KindGroup, BOTH, GENERAL, ONLY_2024, TABS,
};
use borealis_content::Ruleset;

const ONLY_2014: &[Ruleset] = &[Ruleset::Dnd2014];
const DETAILS: &str = "Details";
const SPEED: &str = "Speed";
const SENSES: &str = "Senses";
const ABILITIES: &str = "Ability score increases";
const LANGUAGES: &str = "Languages";
const TRAITS: &str = "Traits";
const NAMES: &str = "Random names";
/// The highest speed or range the form offers, in feet.
const MAX_FEET: i32 = 1000;

const CREATURE_TYPES: &[(&str, &str)] = &[
    ("Aberration", "Aberration"),
    ("Beast", "Beast"),
    ("Celestial", "Celestial"),
    ("Construct", "Construct"),
    ("Dragon", "Dragon"),
    ("Elemental", "Elemental"),
    ("Fey", "Fey"),
    ("Fiend", "Fiend"),
    ("Giant", "Giant"),
    ("Humanoid", "Humanoid"),
    ("Monstrosity", "Monstrosity"),
    ("Ooze", "Ooze"),
    ("Plant", "Plant"),
    ("Undead", "Undead"),
];

const SIZES: &[(&str, &str)] = &[
    ("Tiny", "Tiny"),
    ("Small", "Small"),
    ("Medium", "Medium"),
    ("Large", "Large"),
    ("Huge", "Huge"),
    ("Gargantuan", "Gargantuan"),
];

const ABILITY_FIELDS: [(&str, &str, &str); 6] = [
    ("strength", "strength", "Strength"),
    ("dexterity", "dexterity", "Dexterity"),
    ("constitution", "constitution", "Constitution"),
    ("intelligence", "intelligence", "Intelligence"),
    ("wisdom", "wisdom", "Wisdom"),
    ("charisma", "charisma", "Charisma"),
];

/// Common, the language a new 2014 race starts with.
const COMMON: &str = r#"[{"id":"ID_LANGUAGE_COMMON","name":"Common","type":"Language"}]"#;

fn sp(f: SpeciesField) -> Codec {
    Codec::Species(f)
}

/// What the parts of a species form are.
#[derive(Clone, Copy)]
struct Shape {
    /// The section of its own first fields ("Species", "Subrace").
    section: &'static str,
    /// It has a creature type and a size.
    body: bool,
    /// It names its race (Sub Race, Race Variant), with this help.
    parent: Option<&'static str>,
    /// 2014: fixed ability score increases, languages.
    aurora: bool,
    /// Random names (2014 Race).
    names: bool,
    /// A new one walks 30 feet and is Medium.
    defaults: bool,
}

fn identity(shape: Shape) -> Vec<FieldDef> {
    let mut out = Vec::new();
    if let Some(help) = shape.parent {
        out.push(
            field(
                "parent",
                "Parent race",
                GENERAL,
                Control::Text,
                sp(SpeciesField::Parent),
            )
            .required()
            .section(shape.section)
            .help(help),
        );
    }
    if shape.body {
        let creature = field(
            "creatureType",
            "Creature type",
            GENERAL,
            Control::Combo,
            sp(SpeciesField::CreatureType),
        )
        .choices(CREATURE_TYPES, true)
        .section(shape.section);
        let size = field(
            "size",
            "Size",
            GENERAL,
            Control::Chips,
            sp(SpeciesField::Size),
        )
        .choices(SIZES, false)
        .section(shape.section)
        .help("Two or more: the player chooses one.");
        if shape.defaults {
            out.push(creature.default("\"Humanoid\""));
            out.push(size.default("[\"Medium\"]"));
        } else {
            out.push(creature);
            out.push(size);
        }
    }
    out
}

fn abilities() -> Vec<FieldDef> {
    ABILITY_FIELDS
        .iter()
        .enumerate()
        .map(|(i, &(key, stat, label))| {
            let f = field(
                key,
                label,
                GENERAL,
                Control::Number,
                sp(SpeciesField::Ability(stat)),
            )
            .range(-10, 10)
            .section(ABILITIES);
            if i == 0 {
                f.help("Fixed increases. A choice the player makes is an Ability score increase block on RULES.")
            } else {
                f
            }
        })
        .collect()
}

fn feet(key: &'static str, label: &'static str, codec: Codec, section: &'static str) -> FieldDef {
    field(key, label, GENERAL, Control::Number, codec)
        .range(0, MAX_FEET)
        .section(section)
}

fn speeds(defaults: bool) -> Vec<FieldDef> {
    let walk = feet(
        "speed",
        "Speed",
        sp(SpeciesField::Speed(Movement::Walk)),
        SPEED,
    )
    .help("Walking speed in feet. Other speeds: 0 for none.");
    vec![
        if defaults { walk.default("30") } else { walk },
        feet(
            "flySpeed",
            "Fly speed",
            sp(SpeciesField::Speed(Movement::Fly)),
            SPEED,
        )
        .help("In feet; 0 for none."),
        feet(
            "swimSpeed",
            "Swim speed",
            sp(SpeciesField::Speed(Movement::Swim)),
            SPEED,
        )
        .help("In feet; 0 for none."),
        feet(
            "climbSpeed",
            "Climb speed",
            sp(SpeciesField::Speed(Movement::Climb)),
            SPEED,
        )
        .help("In feet; 0 for none."),
        feet(
            "burrowSpeed",
            "Burrow speed",
            sp(SpeciesField::Speed(Movement::Burrow)),
            SPEED,
        )
        .help("In feet; 0 for none."),
    ]
}

fn senses() -> Vec<FieldDef> {
    vec![
        feet(
            "darkvision",
            "Darkvision",
            sp(SpeciesField::Sense(Sense::Darkvision)),
            SENSES,
        )
        .help("Range in feet; 0 for none."),
        feet(
            "blindsight",
            "Blindsight",
            sp(SpeciesField::Sense(Sense::Blindsight)),
            SENSES,
        )
        .help("Range in feet; 0 for none."),
        feet(
            "tremorsense",
            "Tremorsense",
            sp(SpeciesField::Sense(Sense::Tremorsense)),
            SENSES,
        )
        .help("Range in feet; 0 for none."),
        feet(
            "truesight",
            "Truesight",
            sp(SpeciesField::Sense(Sense::Truesight)),
            SENSES,
        )
        .help("Range in feet; 0 for none."),
    ]
}

fn languages(defaults: bool) -> Vec<FieldDef> {
    let known = field(
        "languages",
        "Languages",
        GENERAL,
        Control::References,
        sp(SpeciesField::Languages),
    )
    .help("The languages every member knows. A choice of more is the number below.")
    .picker(&["Language"])
    .section(LANGUAGES);
    vec![
        if defaults {
            known.default(COMMON)
        } else {
            known
        },
        field(
            "languageChoice",
            "Languages of the player's choice",
            GENERAL,
            Control::Number,
            sp(SpeciesField::LanguageChoice),
        )
        .range(0, 10)
        .section(LANGUAGES)
        .help("How many standard or exotic languages the player picks."),
    ]
}

fn traits() -> FieldDef {
    field(
        "traits",
        "Traits",
        GENERAL,
        Control::Children,
        sp(SpeciesField::Traits),
    )
    .picker(&["Racial Trait"])
    .section(TRAITS)
    .help("Each is given at level 1: make it here, or give one that exists. A trait from a later level, or one with a condition, is a block on RULES.")
}

/// A racial trait's lineage (spec 4.4): the options the player chooses one of (Drow, High Elf, Wood Elf), made inside
/// the trait and offered by one choice by id.
fn lineage() -> FieldDef {
    field(
        "lineage",
        "Lineage options",
        GENERAL,
        Control::Children,
        Codec::Offered {
            ty: "Species Trait Option",
            optional: false,
        },
    )
    .picker(&["Species Trait Option"])
    .editions(ONLY_2024)
    .default("[]")
    .help("The player chooses one when the trait is gained (a lineage or an ancestry). Make each option here.")
}

/// The saving throw of a breath weapon (`data.save`, `data.dc`): the engine reads both for the trait's breath
/// attack (`breath.rs`, with the trait's `damage_scaling` rule).
const SAVES: &[(&str, &str)] = &[
    ("strength", "Strength"),
    ("dexterity", "Dexterity"),
    ("constitution", "Constitution"),
    ("intelligence", "Intelligence"),
    ("wisdom", "Wisdom"),
    ("charisma", "Charisma"),
];

fn breath_weapon() -> [FieldDef; 2] {
    [
        field(
            "save",
            "Saving throw",
            GENERAL,
            Control::Combo,
            Codec::DataText("save"),
        )
        .choices(SAVES, false)
        .section("Breath weapon")
        .help("The saving throw against the trait's breath weapon (its damage is the trait's damage scaling rule)."),
        field(
            "saveDc",
            "Save DC",
            GENERAL,
            Control::Text,
            Codec::DataText("dc"),
        )
        .section("Breath weapon")
        .help("A formula: 8+constitution+proficiency."),
    ]
}

/// A 2014 trait's cantrip casting ability (`spellcasting {name, ability}`, counted under the race's name).
fn casting_ability() -> FieldDef {
    field(
        "castingAbility",
        "Spellcasting ability",
        GENERAL,
        Control::Combo,
        Codec::Casting(CastingField::Ability),
    )
    .choices(ABILITY_CHOICES, false)
    .section("Spells")
    .help("The ability the trait's spells are cast with (a 2014 race's cantrip). 2024 traits say it on each spell they give.")
}

fn names() -> Vec<FieldDef> {
    [
        ("maleNames", "Male names", NameKind::Male),
        ("femaleNames", "Female names", NameKind::Female),
        ("familyNames", "Family or clan names", NameKind::Family),
    ]
    .into_iter()
    .map(|(key, label, k)| {
        field(
            key,
            label,
            GENERAL,
            Control::Chips,
            sp(SpeciesField::Names(k)),
        )
        .section(NAMES)
        .help("Names suggested for this race, one per chip.")
    })
    .collect()
}

/// The parts every form shares, the GENERAL ones under "Details".
fn details() -> impl Iterator<Item = FieldDef> {
    tail().into_iter().map(|f| {
        if f.tab == GENERAL {
            f.section(DETAILS)
        } else {
            f
        }
    })
}

fn fields(shape: Shape) -> Vec<FieldDef> {
    let mut own = identity(shape);
    if shape.aurora {
        own.extend(abilities());
    }
    own.extend(speeds(shape.defaults));
    own.extend(senses());
    if shape.aurora {
        own.extend(languages(shape.defaults));
    }
    own.push(traits());
    if shape.names {
        own.extend(names());
    }
    head().into_iter().chain(own).chain(details()).collect()
}

fn main_kind(
    id: &'static str,
    label: &'static str,
    plural: &'static str,
    description: &'static str,
    ty: &'static [&'static str],
    editions: &'static [Ruleset],
    shape: Shape,
) -> KindDef {
    KindDef {
        id,
        label,
        plural,
        description,
        group: KindGroup::Main,
        element_types: ty,
        parent_kinds: &[],
        editions,
        tabs: TABS,
        fields: fields(shape),
    }
}

fn child_kind(
    id: &'static str,
    label: &'static str,
    plural: &'static str,
    description: &'static str,
    ty: &'static [&'static str],
    parents: &'static [&'static str],
    editions: &'static [Ruleset],
) -> KindDef {
    KindDef {
        id,
        label,
        plural,
        description,
        group: KindGroup::Child,
        element_types: ty,
        parent_kinds: parents,
        editions,
        tabs: TABS,
        fields: head().into_iter().chain(tail()).collect(),
    }
}

const SUB: Shape = Shape {
    section: "Subrace",
    body: false,
    parent: Some("The tag the race's subrace choice offers, usually the race's name (Dwarf)."),
    aurora: true,
    names: false,
    defaults: false,
};

/// Every species form: Species, Race, Subrace, Race variant, then the child kinds.
pub(super) fn species_kinds() -> Vec<KindDef> {
    vec![
        main_kind(
            "species",
            "Species",
            "Species",
            "A 2024 species: size, speed, senses and traits.",
            &["Species"],
            ONLY_2024,
            Shape {
                section: "Species",
                body: true,
                parent: None,
                aurora: false,
                names: false,
                defaults: true,
            },
        ),
        main_kind(
            "race",
            "Race",
            "Races",
            "A 2014 race: ability score increases, size, speed, senses, languages and traits.",
            &["Race"],
            ONLY_2014,
            Shape {
                section: "Race",
                body: true,
                parent: None,
                aurora: true,
                names: true,
                defaults: true,
            },
        ),
        main_kind(
            "subRace",
            "Subrace",
            "Subraces",
            "A 2014 subrace, offered by its race's subrace choice.",
            &["Sub Race"],
            ONLY_2014,
            SUB,
        ),
        main_kind(
            "raceVariant",
            "Race variant",
            "Race variants",
            "A 2014 race variant, offered by its race's variant choice.",
            &["Race Variant"],
            ONLY_2014,
            Shape {
                section: "Variant",
                parent: Some("The tag the race's variant choice offers, such as Human."),
                ..SUB
            },
        ),
        KindDef {
            fields: head()
                .into_iter()
                .chain(std::iter::once(lineage()))
                .chain(breath_weapon())
                .chain(std::iter::once(casting_ability()))
                .chain(tail())
                .collect(),
            ..child_kind(
                "racialTrait",
                "Racial trait",
                "Racial traits",
                "A trait of a species or race, edited inside it.",
                &["Racial Trait"],
                &["species", "race", "subRace", "raceVariant"],
                BOTH,
            )
        },
        KindDef {
            fields: head()
                .into_iter()
                .chain(std::iter::once(super::damage_type_field("Option")))
                .chain(tail())
                .collect(),
            ..child_kind(
                "speciesTraitOption",
                "Species trait option",
                "Species trait options",
                "One option of a species' lineage or ancestry choice, edited inside the species.",
                &["Species Trait Option"],
                &["species", "racialTrait"],
                ONLY_2024,
            )
        },
    ]
}
