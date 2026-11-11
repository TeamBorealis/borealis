//! The small types (spec 4.10, one compact form each) and the generic "Other element" form (spec 4.12).
//!
//! Each small type has the shared parts (name, edition, tags, summary, description, sheet, rule blocks) and exactly
//! its own fields: a language's rarity, script and speakers; what a proficiency group covers (its grants of narrower
//! proficiencies); whether a condition is listed as one; a skill's ability; a deity's alignment, domains and symbol;
//! a mastery property's kind and key; a rules text's group. The generic form edits any other type: its type, its
//! setters as rows, and the shared parts.

use super::super::codec::{Codec, SmallField};
use super::super::other::OPEN_TYPES;
use super::{
    field, head, tail, Control, FieldDef, KindDef, KindGroup, BOTH, GENERAL, ONLY_2024, OTHER_KIND,
    TABS,
};
use borealis_content::Ruleset;

const RARITY: &[(&str, &str)] = &[
    ("standard", "Standard"),
    ("exotic", "Exotic (2024: rare)"),
    ("secret", "Secret"),
];
const ABILITIES: &[(&str, &str)] = &[
    ("Strength", "Strength"),
    ("Dexterity", "Dexterity"),
    ("Constitution", "Constitution"),
    ("Intelligence", "Intelligence"),
    ("Wisdom", "Wisdom"),
    ("Charisma", "Charisma"),
];
const ALIGNMENTS: &[(&str, &str)] = &[
    ("Lawful Good", "Lawful Good"),
    ("Neutral Good", "Neutral Good"),
    ("Chaotic Good", "Chaotic Good"),
    ("Lawful Neutral", "Lawful Neutral"),
    ("Neutral", "Neutral"),
    ("Chaotic Neutral", "Chaotic Neutral"),
    ("Lawful Evil", "Lawful Evil"),
    ("Neutral Evil", "Neutral Evil"),
    ("Chaotic Evil", "Chaotic Evil"),
];
/// The groups the bundled 2024 rules glossary sorts entries into (a new one can be typed).
const GLOSSARY_GROUPS: &[(&str, &str)] = &[
    ("Action", "Action"),
    ("Area of Effect", "Area of Effect"),
    ("Attitude", "Attitude"),
    ("Hazard", "Hazard"),
];
const PROFICIENCY: &[&str] = &["Proficiency"];
/// The pseudo picker type whose rows are the element types loaded content uses ([`super::super::other`]).
const ELEMENT_TYPES: &[&str] = &[super::super::other::ELEMENT_TYPE_PICKER];
/// The pseudo picker type whose rows are the setter names elements of a type use.
const SETTER_NAMES: &[&str] = &[super::super::other::SETTER_NAME_PICKER];

fn small(f: SmallField) -> Codec {
    Codec::Small(f)
}

/// A small kind's own fields between the head and the shared parts; `tags` changes the Tags field (help, default).
struct Small {
    id: &'static str,
    label: &'static str,
    plural: &'static str,
    description: &'static str,
    ty: &'static [&'static str],
    editions: &'static [Ruleset],
    own: Vec<FieldDef>,
    tags_help: Option<&'static str>,
    tags_default: Option<&'static str>,
}

impl Small {
    fn new(
        id: &'static str,
        label: &'static str,
        plural: &'static str,
        description: &'static str,
        ty: &'static [&'static str],
    ) -> Self {
        Self {
            id,
            label,
            plural,
            description,
            ty,
            editions: BOTH,
            own: Vec::new(),
            tags_help: None,
            tags_default: None,
        }
    }

    fn own(self, own: Vec<FieldDef>) -> Self {
        Self { own, ..self }
    }

    fn tags(self, help: &'static str, default: Option<&'static str>) -> Self {
        Self {
            tags_help: Some(help),
            tags_default: default,
            ..self
        }
    }

    fn only_2024(self) -> Self {
        Self {
            editions: ONLY_2024,
            ..self
        }
    }

    fn build(self) -> KindDef {
        let shared = tail().into_iter().map(|f| {
            if f.key != "tags" {
                return f;
            }
            let f = self.tags_help.map_or(f, |h| f.help(h));
            self.tags_default.map_or(f, |d| f.default(d))
        });
        KindDef {
            id: self.id,
            label: self.label,
            plural: self.plural,
            description: self.description,
            group: KindGroup::Small,
            element_types: self.ty,
            parent_kinds: &[],
            editions: self.editions,
            tabs: TABS,
            fields: head().into_iter().chain(self.own).chain(shared).collect(),
        }
    }
}

fn language() -> Small {
    Small::new(
        "language",
        "Language",
        "Languages",
        "A language characters can speak.",
        &["Language"],
    )
    .own(vec![
        field(
            "rarity",
            "Rarity",
            GENERAL,
            Control::Combo,
            Codec::LanguageRarity,
        )
        .choices(RARITY, false)
        .help("Language choices offer languages by rarity: most offer standard ones, some exotic (2024: rare) ones. Secret languages are given by a feature, not chosen."),
        field(
            "script",
            "Script",
            GENERAL,
            Control::Text,
            Codec::SetterText("script"),
        ),
        field(
            "speakers",
            "Typical speakers",
            GENERAL,
            Control::Text,
            Codec::SetterText("speakers"),
        ),
    ])
}

fn proficiency() -> Small {
    Small::new(
        "proficiency",
        "Proficiency",
        "Proficiencies",
        "A proficiency, or a group of them that gives each narrower one.",
        &["Proficiency"],
    )
    .own(vec![field(
        "covers",
        "What it covers",
        GENERAL,
        Control::References,
        small(SmallField::Covers),
    )
    .picker(PROFICIENCY)
    .help("For a group (all light armor, a kind of tool): the narrower proficiencies a character with this one also has.")])
}

fn condition() -> Small {
    Small::new(
        "condition",
        "Condition",
        "Conditions",
        "A condition, with its effects as rules.",
        &["Condition"],
    )
    .own(vec![field(
        "listed",
        "Listed as a condition",
        GENERAL,
        Control::Check,
        small(SmallField::Listed),
    )
    .default("true")
    .help("Shown with the conditions (Blinded, Prone...). Leave it off for helper conditions such as a damage resistance.")])
}

fn skill() -> Small {
    Small::new(
        "skill",
        "Skill",
        "Skills",
        "A skill and the ability it uses.",
        &["Skill"],
    )
    .own(vec![field(
        "ability",
        "Ability",
        GENERAL,
        Control::Combo,
        Codec::SetterText("ability"),
    )
    .choices(ABILITIES, true)
    .required()
    .help("The ability whose modifier the skill adds.")])
    .tags(
        "Skill choices offer skills by these tags: Skill, and the ability or class names a choice asks for.",
        Some("[\"Skill\"]"),
    )
}

fn deity() -> Small {
    Small::new(
        "deity",
        "Deity",
        "Deities",
        "A god, with alignment, domains and symbol.",
        &["Deity"],
    )
    .own(vec![
        field(
            "alignment",
            "Alignment",
            GENERAL,
            Control::Combo,
            Codec::SetterText("alignment"),
        )
        .choices(ALIGNMENTS, true)
        .required()
        .help("Every deity needs an alignment, for example Neutral Good."),
        field(
            "domains",
            "Domains",
            GENERAL,
            Control::Chips,
            Codec::SetterChips("domains"),
        )
        .required()
        .help("Every deity needs at least one domain, for example Life or Light."),
        field(
            "symbol",
            "Symbol",
            GENERAL,
            Control::Text,
            Codec::SetterText("symbol"),
        )
        .required()
        .help("Every deity needs a symbol, for example a sun with a face."),
    ])
}

fn data_key(key: &'static str, data: &'static str, label: &'static str) -> FieldDef {
    field(key, label, GENERAL, Control::Text, Codec::DataText(data)).expert()
}

fn weapon_property() -> Small {
    Small::new(
        "weaponProperty",
        "Weapon property",
        "Weapon properties",
        "A property weapons can have.",
        &["Weapon Property"],
    )
    .own(vec![data_key("propertyKey", "property", "Property key")
        .help(
            "2024 content: the property's name in lower case (ammunition, finesse).",
        )])
}

fn mastery_property() -> Small {
    Small::new(
        "masteryProperty",
        "Mastery property",
        "Mastery properties",
        "A weapon mastery property.",
        &["Mastery Property"],
    )
    .only_2024()
    .own(vec![
        data_key("masteryKind", "kind", "Kind").default("\"weapon_mastery\""),
        field(
            "mastery",
            "Mastery key",
            GENERAL,
            Control::Text,
            Codec::DataText("mastery"),
        )
        .help("The property's name in lower case (cleave, topple): weapons that name it get it."),
    ])
    .tags(
        "Mastery properties carry the tag Mastery Property.",
        Some("[\"Mastery Property\"]"),
    )
}

fn rules_text() -> Small {
    Small::new(
        "rulesText",
        "Rules text",
        "Rules text",
        "A rules or glossary entry (text only).",
        &["Rules Glossary Entry"],
    )
    .own(vec![field(
        "glossaryGroup",
        "Glossary group",
        GENERAL,
        Control::Combo,
        small(SmallField::GlossaryGroup),
    )
    .choices(GLOSSARY_GROUPS, true)
    .help("The group the rules glossary lists it under (also one of its tags). Leave it empty for none.")])
    .tags(
        "Glossary entries carry the tag Rules Glossary.",
        Some("[\"Rules Glossary\"]"),
    )
}

pub(super) fn small_kinds() -> Vec<KindDef> {
    vec![
        language(),
        proficiency(),
        condition(),
        Small::new(
            "damageType",
            "Damage type",
            "Damage types",
            "A type of damage.",
            &["Damage Type"],
        ),
        skill(),
        deity(),
        Small::new(
            "vision",
            "Vision",
            "Visions",
            "A kind of sight, such as darkvision.",
            &["Vision"],
        ),
        Small::new("size", "Size", "Sizes", "A creature size.", &["Size"]),
        Small::new(
            "alignment",
            "Alignment",
            "Alignments",
            "An alignment.",
            &["Alignment"],
        )
        .own(vec![field(
            "abbreviation",
            "Abbreviation",
            GENERAL,
            Control::Text,
            Codec::SetterText("abbreviation"),
        )]),
        weapon_property(),
        Small::new(
            "weaponGroup",
            "Weapon group",
            "Weapon groups",
            "A group of weapons.",
            &["Weapon Group"],
        ),
        Small::new(
            "weaponCategory",
            "Weapon category",
            "Weapon categories",
            "A weapon category.",
            &["Weapon Category"],
        ),
        Small::new(
            "armorGroup",
            "Armor group",
            "Armor groups",
            "A group of armor.",
            &["Armor Group"],
        ),
        mastery_property(),
        Small::new(
            "focusGroup",
            "Spellcasting focus group",
            "Spellcasting focus groups",
            "A kind of spellcasting focus.",
            &["Spellcasting Focus Group"],
        ),
        Small::new(
            "dragonmark",
            "Dragonmark",
            "Dragonmarks",
            "A mark with its rules.",
            &["Dragonmark"],
        ),
        rules_text(),
    ]
    .into_iter()
    .map(Small::build)
    .collect()
}

pub(super) fn other_kind() -> KindDef {
    let own = vec![
        field("type", "Type", GENERAL, Control::Combo, Codec::Type)
            .required()
            .choices(OPEN_TYPES, true)
            .picker(ELEMENT_TYPES)
            .help("Companion, Familiar, Property, or any type your content uses that has no form of its own."),
        field(
            "setters",
            "Properties",
            GENERAL,
            Control::Setters,
            Codec::OtherSetters,
        )
        .picker(SETTER_NAMES)
        .help("Each property is a name and a value; the names other elements of this type use are offered."),
    ];
    KindDef {
        id: OTHER_KIND,
        label: "Other element",
        plural: "Other elements",
        description: "Any other kind of element, with the parts every form shares (expert).",
        group: KindGroup::Expert,
        element_types: &[],
        parent_kinds: &[],
        editions: BOTH,
        tabs: TABS,
        fields: head().into_iter().chain(own).chain(tail()).collect(),
    }
}
