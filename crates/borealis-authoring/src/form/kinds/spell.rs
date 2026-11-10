//! The spell form (spec 4.1): level, school, casting time, range, duration, components, concentration and ritual, the
//! spell lists, then the parts every form shares. A spell's support tags are its spell lists (the way Aurora content
//! puts a spell on a class list, and the way selects pick spells by tag), so the shared Tags field is not on it.

use super::super::codec::{Codec, Component, SpellField};
use super::{field, head, tail, Control, FieldDef, KindDef, KindGroup, BOTH, GENERAL, TABS};

const CASTING: &str = "Casting";
const COMPONENTS: &str = "Components";
const LISTS: &str = "Spell lists";
const DETAILS: &str = "Details";

const LEVELS: &[(&str, &str)] = &[
    ("0", "Cantrip"),
    ("1", "1st level"),
    ("2", "2nd level"),
    ("3", "3rd level"),
    ("4", "4th level"),
    ("5", "5th level"),
    ("6", "6th level"),
    ("7", "7th level"),
    ("8", "8th level"),
    ("9", "9th level"),
];

const SCHOOLS: &[(&str, &str)] = &[
    ("Abjuration", "Abjuration"),
    ("Conjuration", "Conjuration"),
    ("Divination", "Divination"),
    ("Enchantment", "Enchantment"),
    ("Evocation", "Evocation"),
    ("Illusion", "Illusion"),
    ("Necromancy", "Necromancy"),
    ("Transmutation", "Transmutation"),
];

const TIMES: &[(&str, &str)] = &[
    ("Action", "Action"),
    ("Bonus Action", "Bonus Action"),
    ("Reaction", "Reaction"),
    ("1 Minute", "1 Minute"),
    ("10 Minutes", "10 Minutes"),
    ("1 Hour", "1 Hour"),
    ("8 Hours", "8 Hours"),
    ("12 Hours", "12 Hours"),
    ("24 Hours", "24 Hours"),
];

const RANGES: &[(&str, &str)] = &[
    ("Self", "Self"),
    ("Touch", "Touch"),
    ("5 feet", "5 feet"),
    ("10 feet", "10 feet"),
    ("30 feet", "30 feet"),
    ("60 feet", "60 feet"),
    ("90 feet", "90 feet"),
    ("120 feet", "120 feet"),
    ("150 feet", "150 feet"),
    ("300 feet", "300 feet"),
    ("1 mile", "1 mile"),
    ("Sight", "Sight"),
    ("Unlimited", "Unlimited"),
    ("Special", "Special"),
];

const DURATIONS: &[(&str, &str)] = &[
    ("Instantaneous", "Instantaneous"),
    ("1 round", "1 round"),
    ("1 minute", "1 minute"),
    ("10 minutes", "10 minutes"),
    ("1 hour", "1 hour"),
    ("8 hours", "8 hours"),
    ("24 hours", "24 hours"),
    ("10 days", "10 days"),
    ("Until dispelled", "Until dispelled"),
    ("Special", "Special"),
];

fn spell(f: SpellField) -> Codec {
    Codec::Spell(f)
}

fn component(key: &'static str, label: &'static str, c: Component, on: bool) -> FieldDef {
    field(
        key,
        label,
        GENERAL,
        Control::Check,
        spell(SpellField::Component(c)),
    )
    .section(COMPONENTS)
    .default(if on { "true" } else { "false" })
}

fn own_fields() -> Vec<FieldDef> {
    vec![
        field("level", "Level", GENERAL, Control::Combo, spell(SpellField::Level))
            .required()
            .choices(LEVELS, false)
            .section(CASTING)
            .default("\"1\""),
        field(
            "school",
            "School",
            GENERAL,
            Control::Combo,
            spell(SpellField::School),
        )
        .required()
        .choices(SCHOOLS, false)
        .section(CASTING),
        field(
            "time",
            "Casting time",
            GENERAL,
            Control::Combo,
            Codec::SetterText("time"),
        )
        .required()
        .choices(TIMES, true)
        .section(CASTING)
        .default("\"Action\""),
        field(
            "reactionTrigger",
            "Reaction trigger",
            GENERAL,
            Control::Text,
            Codec::DataText("reaction_trigger"),
        )
        .section(CASTING)
        .help("For a reaction or a bonus action taken after something happens: \"when you take damage\"."),
        field(
            "range",
            "Range",
            GENERAL,
            Control::Combo,
            Codec::SetterText("range"),
        )
        .required()
        .choices(RANGES, true)
        .section(CASTING)
        .default("\"Self\""),
        field(
            "duration",
            "Duration",
            GENERAL,
            Control::Combo,
            Codec::SetterText("duration"),
        )
        .required()
        .choices(DURATIONS, true)
        .section(CASTING)
        .default("\"Instantaneous\""),
        field(
            "concentration",
            "Concentration",
            GENERAL,
            Control::Check,
            spell(SpellField::Concentration),
        )
        .section(CASTING)
        .default("false"),
        field(
            "ritual",
            "Ritual",
            GENERAL,
            Control::Check,
            spell(SpellField::Ritual),
        )
        .section(CASTING)
        .default("false"),
        component("verbal", "Verbal (V)", Component::Verbal, true),
        component("somatic", "Somatic (S)", Component::Somatic, true),
        component("material", "Material (M)", Component::Material, false),
        field(
            "materialText",
            "Material component",
            GENERAL,
            Control::Text,
            Codec::SetterText("materialComponent"),
        )
        .section(COMPONENTS)
        .help("What the caster needs, as the spell shows it: \"a diamond worth 300+ GP\"."),
        field(
            "materialCost",
            "Material cost (gp)",
            GENERAL,
            Control::Text,
            spell(SpellField::MaterialCost),
        )
        .section(COMPONENTS)
        .help("Only for a material with a price."),
        field(
            "materialConsumed",
            "The spell uses the material up",
            GENERAL,
            Control::Check,
            spell(SpellField::MaterialConsumed),
        )
        .section(COMPONENTS),
        field(
            "spellLists",
            "Lists",
            GENERAL,
            Control::SpellLists,
            spell(SpellField::Lists),
        )
        .section(LISTS)
        .help("The classes whose spell lists offer this spell."),
    ]
}

/// The spell form.
pub(super) fn spell_kind() -> KindDef {
    let shared = tail()
        .into_iter()
        .filter(|f| f.codec != Codec::Tags)
        .map(|f| {
            if f.tab == GENERAL {
                f.section(DETAILS)
            } else {
                f
            }
        });
    KindDef {
        id: "spell",
        label: "Spell",
        plural: "Spells",
        description: "A spell, with the class spell lists that offer it.",
        group: KindGroup::Main,
        element_types: &["Spell"],
        parent_kinds: &[],
        editions: BOTH,
        tabs: TABS,
        fields: head()
            .into_iter()
            .chain(own_fields())
            .chain(shared)
            .collect(),
    }
}
