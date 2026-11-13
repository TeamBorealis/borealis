//! The class form (spec 4.5) for 2014 and 2024 classes, and the Class Feature form its LEVELS grid opens.
//!
//! Sub-tabs: GENERAL (hit die, primary abilities, the shared details), PROFICIENCIES (first class only: saving throws,
//! armor, weapons, other proficiencies, the skill choice), LEVELS (the 20-level grid of features and the class
//! table's counters), SPELLCASTING (WB-17), MULTICLASS (the `multiclass` section), EQUIPMENT (the starting-equipment
//! package), RULES (rules no field holds), DESCRIPTION. Try it is the editor's own command (WB-21).

use super::super::codec::{ClassField, Codec, FeatureField, ProficiencyGroup, ABILITY_CHOICES};
use super::super::FormState;
use super::{
    field, head, tail, Control, FieldDef, KindDef, KindGroup, BOTH, DESCRIPTION, GENERAL, RULES,
};
use borealis_content::{rich, Ruleset};
use serde_json::{json, Value};

/// The class form's id.
pub const CLASS_KIND: &str = "class";
/// The class feature form's id.
pub const CLASS_FEATURE_KIND: &str = "classFeature";

const PROFICIENCIES: &str = "proficiencies";
const LEVELS: &str = "levels";
const SPELLCASTING: &str = "spellcasting";
const MULTICLASS: &str = "multiclass";
const EQUIPMENT: &str = "equipment";

const CLASS_TABS: &[(&str, &str)] = &[
    (GENERAL, "GENERAL"),
    (PROFICIENCIES, "PROFICIENCIES"),
    (LEVELS, "LEVELS"),
    (SPELLCASTING, "SPELLCASTING"),
    (MULTICLASS, "MULTICLASS"),
    (EQUIPMENT, "EQUIPMENT"),
    (RULES, "RULES"),
    (DESCRIPTION, "DESCRIPTION"),
];

const HIT_DICE: &[(&str, &str)] = &[("d6", "d6"), ("d8", "d8"), ("d10", "d10"), ("d12", "d12")];
const PROFICIENCY: &[&str] = &["Proficiency"];
const PACKAGE: &[&str] = &["Equipment Package"];
const FEATURE: &[&str] = &["Class Feature"];

/// Shared fields a class doesn't use (a class can't be taken twice).
const NOT_FOR_CLASSES: &[&str] = &["allowDuplicate"];

fn class(f: ClassField) -> Codec {
    Codec::Class(f)
}

fn first_class_only(f: FieldDef) -> FieldDef {
    f.section("First class only")
}

fn own_fields() -> Vec<FieldDef> {
    vec![
        field("hitDie", "Hit die", GENERAL, Control::Combo, class(ClassField::HitDie))
            .required()
            .choices(HIT_DICE, false)
            .section("Class"),
        field(
            "primaryAbilities",
            "Primary abilities",
            GENERAL,
            Control::Chips,
            class(ClassField::PrimaryAbilities),
        )
        .help("The abilities the class relies on most, shown with the class.")
        .choices(ABILITY_CHOICES, false)
        .section("Class")
        .default("[]"),
        first_class_only(
            field(
                "savingThrows",
                "Saving throws",
                PROFICIENCIES,
                Control::Chips,
                class(ClassField::SavingThrows),
            )
            .choices(ABILITY_CHOICES, false)
            .default("[]")
            .help("Only a character who starts in this class gets what this section lists."),
        ),
        first_class_only(
            field(
                "armor",
                "Armor training",
                PROFICIENCIES,
                Control::References,
                class(ClassField::Proficiencies(ProficiencyGroup::Armor)),
            )
            .picker(PROFICIENCY)
            // a class lists its proficiencies by kind (read by id): the editor moves one picked under another kind (P3)
            .help("Armor and shield training. A proficiency of another kind picked here moves to its own list.")
            .default("[]"),
        ),
        first_class_only(
            field(
                "weapons",
                "Weapon proficiencies",
                PROFICIENCIES,
                Control::References,
                class(ClassField::Proficiencies(ProficiencyGroup::Weapons)),
            )
            .picker(PROFICIENCY)
            // a class lists its proficiencies by kind (read by id): the editor moves one picked under another kind (P3)
            .help("Weapon proficiencies. A proficiency of another kind picked here moves to its own list.")
            .default("[]"),
        ),
        first_class_only(
            field(
                "tools",
                "Tool and other proficiencies",
                PROFICIENCIES,
                Control::References,
                class(ClassField::Proficiencies(ProficiencyGroup::Other)),
            )
            .picker(PROFICIENCY)
            // a class lists its proficiencies by kind (read by id): the editor moves one picked under another kind (P3)
            .help("Tools and every other proficiency.")
            .default("[]"),
        ),
        first_class_only(
            field(
                "skills",
                "Skills",
                PROFICIENCIES,
                Control::SkillChoice,
                class(ClassField::Skills),
            )
            .picker(PROFICIENCY)
            .default("{\"count\":2,\"skills\":[]}"),
        ),
        field(
            "levels",
            "Features by level",
            LEVELS,
            Control::Levels,
            Codec::Children {
                ty: "Class Feature",
                levelled: true,
            },
        )
        .help("What the class gives at each level. Add a feature with the Plus on its level.")
        .picker(FEATURE)
        .default("[]"),
        field("table", "Class table", LEVELS, Control::ClassTable, class(ClassField::Table))
            .default("[]")
            .help("Counters per level (uses, dice); a rule's maximum can read a column."),
        field(
            "multiclass",
            "Can be taken as a second class",
            MULTICLASS,
            Control::Check,
            class(ClassField::Multiclass),
        )
        .help("Off: a character can only start in this class, never add it later.")
        .default("true"),
        field(
            "mcRequirements",
            "Prerequisite",
            MULTICLASS,
            Control::Condition,
            class(ClassField::McRequirements),
        )
        .help("The ability scores a character needs to take this class as a second class (and to leave it)."),
        field(
            "mcPrerequisite",
            "Prerequisite as shown",
            MULTICLASS,
            Control::Text,
            class(ClassField::McPrerequisite),
        )
        .default("\"\"")
        .help("Written from the prerequisite while left empty or as written; type to reword it."),
        field(
            "mcProficiencies",
            "Proficiencies gained",
            MULTICLASS,
            Control::References,
            class(ClassField::McProficiencies),
        )
        .picker(PROFICIENCY)
        .default("[]"),
        field(
            "mcProficiencyText",
            "Proficiencies as shown",
            MULTICLASS,
            Control::Text,
            class(ClassField::McProficiencyText),
        )
        .default("\"\"")
        .help("Written from the proficiencies while left empty or as written; type to reword it."),
        field(
            "equipment",
            "Starting equipment",
            EQUIPMENT,
            Control::Package,
            class(ClassField::Equipment),
        )
        .picker(PACKAGE)
        .default("[]")
        .help("The lettered options a character who starts in this class chooses from."),
    ]
}

/// The class form.
pub(super) fn class_kind() -> KindDef {
    let shared = tail()
        .into_iter()
        .filter(|f| !NOT_FOR_CLASSES.contains(&f.key))
        .map(|f| match f.tab {
            GENERAL => f.section("Details"),
            RULES => FieldDef {
                label: "Other rules",
                help: Some("Rules of the class that no field above holds."),
                ..f
            },
            _ => f,
        });
    KindDef {
        id: CLASS_KIND,
        label: "Class",
        plural: "Classes",
        description: "A class with its 20 levels of features, proficiencies, multiclassing and starting equipment.",
        group: KindGroup::Main,
        element_types: &["Class"],
        parent_kinds: &[],
        editions: BOTH,
        tabs: CLASS_TABS,
        fields: head().into_iter().chain(own_fields()).chain(shared).collect(),
    }
}

const ONLY_2024: &[Ruleset] = &[Ruleset::Dnd2024];

/// The Class Feature form: a feature with the parts every form shares; the generated features' own values
/// (ability score improvements, subclass choice, epic boon) are written from the class's name.
pub(super) fn class_feature_kind() -> KindDef {
    let generated = [
        field(
            "asiLevels",
            "Ability score improvements at levels",
            GENERAL,
            Control::Numbers,
            Codec::Feature(FeatureField::Asi),
        )
        .help("The class levels this feature gives an ability score improvement (or a feat) at.")
        .range(1, 20)
        .default("[]"),
        field(
            "subclassLabel",
            "Subclass choice",
            GENERAL,
            Control::Text,
            Codec::Feature(FeatureField::Subclass),
        )
        .default("\"\"")
        .help("The name of the choice (Sacred Oath, Martial Archetype); subclasses of this class are offered there."),
        field(
            "epicBoon",
            "Epic boon choice",
            GENERAL,
            Control::Check,
            Codec::Feature(FeatureField::EpicBoon),
        )
        .help("At this level the character chooses an Epic Boon feat.")
        .editions(ONLY_2024)
        .default("false"),
    ]
    .map(|f| f.section("Made by the class"));
    KindDef {
        id: CLASS_FEATURE_KIND,
        label: "Class feature",
        plural: "Class features",
        description: "A feature a class gives at a level, edited inside the class.",
        group: KindGroup::Child,
        element_types: &["Class Feature"],
        parent_kinds: &[CLASS_KIND],
        editions: BOTH,
        tabs: super::CASTER_TABS,
        fields: head()
            .into_iter()
            .chain(generated)
            .chain(super::caster::caster_fields())
            .chain(tail())
            .collect(),
    }
}

fn feature(key: &str, name: &str, text: &str, field: (&str, Value)) -> FormState {
    let mut form = class_feature_kind().new_form(key);
    form.fields.insert("name".into(), json!(name));
    form.fields.insert(
        "description".into(),
        serde_json::to_value(rich::parse(&format!("<p>{text}</p>"))).unwrap_or(Value::Null),
    );
    form.fields.insert(field.0.into(), field.1);
    form
}

/// A new class of `edition` (spec 4.5, the generated parts): its own edition (a class never changes edition, R4),
/// and the features every class has, on the LEVELS grid: Ability Score Improvement at 4, 8, 12, 16 and 19 (2024: 4,
/// 8, 12 and 16, and an Epic Boon at 19) and the subclass choice at 3. Every one can be changed or removed.
#[must_use]
pub fn new_class_form(edition: Ruleset) -> FormState {
    let new = edition == Ruleset::Dnd2024;
    let mut form = class_kind().new_form("e");
    form.fields
        .insert("edition".into(), json!(edition.as_str()));
    let asi_levels: &[u32] = if new {
        &[4, 8, 12, 16]
    } else {
        &[4, 8, 12, 16, 19]
    };
    let asi = if new {
        "You gain the Ability Score Improvement feat or another feat of your choice for which you qualify."
    } else {
        "You can increase one ability score of your choice by 2, or two ability scores of your choice by 1, or take a feat."
    };
    let mut children = vec![
        (
            feature(
                "n1",
                "Ability Score Improvement",
                asi,
                ("asiLevels", json!(asi_levels)),
            ),
            4,
        ),
        (
            feature(
                "n2",
                "Subclass",
                "You gain a subclass of your choice.",
                ("subclassLabel", json!("Subclass")),
            ),
            3,
        ),
    ];
    if new {
        children.push((
            feature(
                "n3",
                "Epic Boon",
                "You gain an Epic Boon feat or another feat of your choice for which you qualify.",
                ("epicBoon", json!(true)),
            ),
            19,
        ));
    }
    let mut entries: Vec<Value> = Vec::new();
    for (child, level) in children {
        entries.push(json!({ "level": level, "child": child.key }));
        form.children.push(child);
    }
    entries.sort_by_key(|e| e["level"].as_u64());
    form.fields.insert("levels".into(), Value::Array(entries));
    form
}
