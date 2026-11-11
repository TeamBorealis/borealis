//! The subclass form (spec 4.7), its Subclass Feature form, and the class option form (spec 4.8).
//!
//! * **Subclass** (`Archetype`): GENERAL (its class, the choice that offers it, the level it is chosen at, the shared
//!   details), LEVELS (the subclass feature levels its class gives, then the 20-level grid of its features),
//!   SPELLCASTING (no fields of its own: the section of its Spellcasting feature, as the class form's), RULES (rules no
//!   field holds: subclass spells are `grant_spell` blocks), DESCRIPTION.
//! * **Subclass feature** (`Archetype Feature`, child of a subclass): the Class Feature form's parts, with a
//!   spellcasting section for a third caster (spec 4.6; named after the subclass, gated on the class level).
//! * **Class option** (`Option`): the group a class's choice offers it by, the class it belongs to, the level
//!   requirement (the condition builder, `[warlock:2]`), whether it can be taken again, rules.

use super::super::codec::{Codec, FeatField, SubclassField};
use super::super::subclass::{
    LEVELS as FEATURE_LEVELS, LEVEL_GAINED, PARENT, SUBCLASS_FEATURE_KIND, SUBCLASS_KIND, TAG,
};
use super::{
    field, head, tail, Control, FieldDef, KindDef, KindGroup, BOTH, DESCRIPTION, GENERAL, RULES,
};

/// The class option form's id.
pub const CLASS_OPTION_KIND: &str = "classOption";

const LEVELS: &str = "levels";
const SUBCLASS_TABS: &[(&str, &str)] = &[
    (GENERAL, "GENERAL"),
    (LEVELS, "LEVELS"),
    // the spellcasting section of one of its features (a third caster), as the class form's
    (super::caster::SPELLCASTING, "SPELLCASTING"),
    (RULES, "RULES"),
    (DESCRIPTION, "DESCRIPTION"),
];

const CLASS: &[&str] = &["Class"];
const FEATURE: &[&str] = &["Archetype Feature"];

/// Shared fields these forms replace with their own (a subclass can't be taken twice; an option's "taken again" is
/// its own field).
const NOT_HERE: &[&str] = &["allowDuplicate"];

/// The groups the bundled classes' choices offer options by (a homebrew class's choice can name a new one).
const GROUPS: &[(&str, &str)] = &[
    ("Eldritch Invocation", "Eldritch Invocation"),
    ("Fighting Style", "Fighting Style"),
    ("Metamagic", "Metamagic"),
    ("Brutal Strike", "Brutal Strike"),
    ("Cunning Strike", "Cunning Strike"),
    ("Divine Order", "Divine Order"),
    ("Primal Order", "Primal Order"),
    ("Monk Focus", "Monk Focus"),
];

const CLASSES: &[(&str, &str)] = &[
    ("Barbarian", "Barbarian"),
    ("Bard", "Bard"),
    ("Cleric", "Cleric"),
    ("Druid", "Druid"),
    ("Fighter", "Fighter"),
    ("Monk", "Monk"),
    ("Paladin", "Paladin"),
    ("Ranger", "Ranger"),
    ("Rogue", "Rogue"),
    ("Sorcerer", "Sorcerer"),
    ("Warlock", "Warlock"),
    ("Wizard", "Wizard"),
];

fn sub(f: SubclassField) -> Codec {
    Codec::Subclass(f)
}

/// The shared parts, without the ones the form replaces; GENERAL ones under "Details".
fn shared() -> impl Iterator<Item = FieldDef> {
    tail()
        .into_iter()
        .filter(|f| !NOT_HERE.contains(&f.key))
        .map(|f| match f.tab {
            GENERAL => f.section("Details"),
            _ => f,
        })
}

/// The subclass form.
pub(super) fn subclass_kind() -> KindDef {
    let own = [
        field(
            PARENT,
            "Class",
            GENERAL,
            Control::Reference,
            sub(SubclassField::Parent),
        )
        .required()
        .picker(CLASS)
        .section("Subclass")
        .default("null")
        .help("Its subclass choice offers this subclass."),
        field(
            TAG,
            "Offered by the choice",
            GENERAL,
            Control::Text,
            sub(SubclassField::Tag),
        )
        .read_only()
        .section("Subclass")
        .default("\"\"")
        .help("The tag the class's subclass choice offers subclasses by; written from the class."),
        field(
            LEVEL_GAINED,
            "Chosen at level",
            GENERAL,
            Control::Number,
            sub(SubclassField::LevelGained),
        )
        .read_only()
        .range(1, 20)
        .section("Subclass"),
        field(
            FEATURE_LEVELS,
            "Subclass feature levels",
            LEVELS,
            Control::Numbers,
            sub(SubclassField::FeatureLevels),
        )
        .read_only()
        .range(1, 20)
        .default("[]")
        .help("The levels the class gives subclass features at; the grid marks them."),
        field(
            "levels",
            "Features by level",
            LEVELS,
            Control::Levels,
            Codec::Children {
                ty: "Archetype Feature",
                levelled: true,
            },
        )
        .picker(FEATURE)
        .default("[]"),
    ];
    KindDef {
        id: SUBCLASS_KIND,
        label: "Subclass",
        plural: "Subclasses",
        description: "A subclass of an own or loaded class, with its features by level.",
        group: KindGroup::Main,
        element_types: &["Archetype"],
        parent_kinds: &[],
        editions: BOTH,
        tabs: SUBCLASS_TABS,
        fields: head().into_iter().chain(own).chain(shared()).collect(),
    }
}

/// The Subclass Feature form: a feature a subclass gives at a level, with a spellcasting section (a third caster).
pub(super) fn subclass_feature_kind() -> KindDef {
    KindDef {
        id: SUBCLASS_FEATURE_KIND,
        label: "Subclass feature",
        plural: "Subclass features",
        description: "A feature a subclass gives at a level, edited inside the subclass.",
        group: KindGroup::Child,
        element_types: &["Archetype Feature"],
        parent_kinds: &[SUBCLASS_KIND],
        editions: BOTH,
        tabs: super::CASTER_TABS,
        fields: head()
            .into_iter()
            .chain(super::caster::caster_fields())
            .chain(tail())
            .collect(),
    }
}

/// The class option form.
pub(super) fn class_option_kind() -> KindDef {
    let own = [
        field(
            "group",
            "Group",
            GENERAL,
            Control::Combo,
            sub(SubclassField::OptionGroup),
        )
        .required()
        .choices(GROUPS, true)
        .section("Option")
        .default("\"\"")
        .help("A class's choice offers the options of its group (its tag): Eldritch Invocation, Fighting Style, or a group your class's choice names."),
        field(
            "className",
            "Class",
            GENERAL,
            Control::Combo,
            Codec::DataText("class_name"),
        )
        .choices(CLASSES, true)
        .section("Option")
        .help("The class it belongs to (shown with it)."),
        field(
            "repeatable",
            "Can be taken more than once",
            GENERAL,
            Control::Check,
            Codec::Feat(FeatField::Repeatable),
        )
        .section("Option")
        .default("false"),
    ];
    // the level requirement goes with the option's own fields
    let is_prerequisite = |f: &FieldDef| matches!(f.key, "requirements" | "prerequisiteText");
    let prerequisite = shared().filter(is_prerequisite).map(|f| match f.key {
        "requirements" => FieldDef {
            help: Some(
                "A level in its class (Warlock level at least 2), or another option it needs.",
            ),
            ..f.section("Option")
        },
        _ => f.section("Option"),
    });
    let rest = shared().filter(|f| !is_prerequisite(f));
    KindDef {
        id: CLASS_OPTION_KIND,
        label: "Class option",
        plural: "Class options",
        description:
            "An option a class's choice offers: an invocation, a fighting style, a metamagic.",
        group: KindGroup::Main,
        element_types: &["Option"],
        parent_kinds: &[],
        editions: BOTH,
        tabs: super::TABS,
        fields: head()
            .into_iter()
            .chain(own)
            .chain(prerequisite)
            .chain(rest)
            .collect(),
    }
}
