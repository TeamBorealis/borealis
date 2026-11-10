//! The creator's forms (spec 3.1, 3.2, 4.10, 4.12): which element types each kind writes, its sub-tabs and its
//! fields, each bound to the model locations it edits.
//!
//! WB-04 defines the parts every form shares, the small types and the generic "Other element" form. Element types
//! with no form of their own yet open in the generic form, so every bundled element has a form (the per-kind forms
//! of WB-11..WB-20 take their types over).

use super::codec::Codec;
use super::FormState;
use borealis_content::Ruleset;
use serde_json::{json, Value};
use std::sync::OnceLock;

mod feat;
mod spell;

/// The generic form (spec 4.12), also the form of every type no other kind writes.
pub const OTHER_KIND: &str = "other";

/// The control a field is edited with (`HomebrewControl`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// Single-line text.
    Text,
    /// Multi-line text.
    Multiline,
    /// A list of choices, maybe with custom text.
    Combo,
    /// A check box.
    Check,
    /// Tag chips picked from the tags loaded content uses.
    Tags,
    /// Free chips.
    Chips,
    /// The condition builder.
    Condition,
    /// The rich-text editor.
    RichText,
    /// The sheet editor.
    Sheet,
    /// The rule block list.
    Blocks,
    /// Name/value rows (expert).
    Setters,
    /// One element picker.
    Reference,
    /// The 2014/2024 counterpart picker (expert).
    MapsTo,
    /// A spell's lists: one check box per spell list loaded classes declare, plus new list names.
    SpellLists,
}

impl Control {
    /// The wire name (`HomebrewControl`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Multiline => "multiline",
            Self::Combo => "combo",
            Self::Check => "check",
            Self::Tags => "tags",
            Self::Chips => "chips",
            Self::Condition => "condition",
            Self::RichText => "richText",
            Self::Sheet => "sheet",
            Self::Blocks => "blocks",
            Self::Setters => "setters",
            Self::Reference => "reference",
            Self::MapsTo => "mapsTo",
            Self::SpellLists => "spellLists",
        }
    }
}

/// Where a kind sits in the New element dialog and the CATEGORIES list (`HomebrewKindGroup`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindGroup {
    /// The main kinds (spell, class...).
    Main,
    /// One compact form each.
    Small,
    /// Edited inside a parent.
    Child,
    /// Expert.
    Expert,
}

impl KindGroup {
    /// The wire name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Small => "small",
            Self::Child => "child",
            Self::Expert => "expert",
        }
    }
}

/// One form field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldDef {
    /// Key in `FormState::fields`.
    pub key: &'static str,
    /// Sentence-case label.
    pub label: &'static str,
    /// Sub-tab id.
    pub tab: &'static str,
    /// The control.
    pub control: Control,
    /// How it maps to the element.
    pub codec: Codec,
    /// The validator needs it.
    pub required: bool,
    /// Combo choices `(value, label)`.
    pub choices: &'static [(&'static str, &'static str)],
    /// A combo that takes custom text.
    pub allow_custom: bool,
    /// Shown, not edited (Source).
    pub read_only: bool,
    /// Only an expert sees it.
    pub expert: bool,
    /// Plain-words help.
    pub help: Option<&'static str>,
    /// The SectionSecondary heading it is grouped under (spec 13.4).
    pub section: Option<&'static str>,
    /// The value a new element starts with, as JSON text.
    pub default: Option<&'static str>,
}

const fn field(
    key: &'static str,
    label: &'static str,
    tab: &'static str,
    control: Control,
    codec: Codec,
) -> FieldDef {
    FieldDef {
        key,
        label,
        tab,
        control,
        codec,
        required: false,
        choices: &[],
        allow_custom: false,
        read_only: false,
        expert: false,
        help: None,
        section: None,
        default: None,
    }
}

impl FieldDef {
    const fn required(self) -> Self {
        Self {
            required: true,
            ..self
        }
    }
    const fn choices(self, choices: &'static [(&'static str, &'static str)], custom: bool) -> Self {
        Self {
            choices,
            allow_custom: custom,
            ..self
        }
    }
    const fn expert(self) -> Self {
        Self {
            expert: true,
            ..self
        }
    }
    const fn read_only(self) -> Self {
        Self {
            read_only: true,
            ..self
        }
    }
    const fn help(self, help: &'static str) -> Self {
        Self {
            help: Some(help),
            ..self
        }
    }
    const fn section(self, section: &'static str) -> Self {
        Self {
            section: Some(section),
            ..self
        }
    }
    /// `default` is JSON text (`"\"1\""`, `"true"`).
    const fn default(self, default: &'static str) -> Self {
        Self {
            default: Some(default),
            ..self
        }
    }

    /// The value a new element starts with, if the field has one.
    #[must_use]
    pub fn default_value(&self) -> Option<Value> {
        self.default.and_then(|d| serde_json::from_str(d).ok())
    }
}

/// One creator form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KindDef {
    /// `language`, `other`...
    pub id: &'static str,
    /// "Language".
    pub label: &'static str,
    /// "Languages".
    pub plural: &'static str,
    /// One line for the New element card.
    pub description: &'static str,
    /// Group.
    pub group: KindGroup,
    /// Element types it writes (the first for a new element); empty = any type (the generic form's Type field).
    pub element_types: &'static [&'static str],
    /// Kinds it is edited inside.
    pub parent_kinds: &'static [&'static str],
    /// Editions it exists in.
    pub editions: &'static [Ruleset],
    /// Sub-tabs `(id, label)`.
    pub tabs: &'static [(&'static str, &'static str)],
    /// Fields, in order.
    pub fields: Vec<FieldDef>,
}

impl KindDef {
    /// The field with this key.
    #[must_use]
    pub fn field(&self, key: &str) -> Option<&FieldDef> {
        self.fields.iter().find(|f| f.key == key)
    }

    /// The type a new element of this kind gets (`None` for the generic form: its Type field decides).
    #[must_use]
    pub fn new_element_type(&self) -> Option<&'static str> {
        self.element_types.first().copied()
    }

    /// The form of a new element of this kind: an empty name and every field's starting value.
    #[must_use]
    pub fn new_form(&self, key: impl Into<String>) -> FormState {
        let mut form = FormState::new(key, self.id);
        form.fields.insert("name".into(), json!(""));
        for f in &self.fields {
            if let Some(v) = f.default_value() {
                form.fields.insert(f.key.to_string(), v);
            }
        }
        form
    }
}

const GENERAL: &str = "general";
const RULES: &str = "rules";
const DESCRIPTION: &str = "description";
const TABS: &[(&str, &str)] = &[
    (GENERAL, "GENERAL"),
    (RULES, "RULES"),
    (DESCRIPTION, "DESCRIPTION"),
];
const BOTH: &[Ruleset] = &[Ruleset::Dnd2014, Ruleset::Dnd2024];
const ONLY_2024: &[Ruleset] = &[Ruleset::Dnd2024];

const EDITIONS: &[(&str, &str)] = &[
    ("", "The pack's edition"),
    ("dnd2014", "2014"),
    ("dnd2024", "2024"),
    ("any", "Either edition"),
];
const RARITY: &[(&str, &str)] = &[
    ("standard", "Standard"),
    ("exotic", "Exotic"),
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

/// GENERAL fields before the kind's own.
fn head() -> Vec<FieldDef> {
    vec![
        field("name", "Name", GENERAL, Control::Text, Codec::Name).required(),
        field(
            "edition",
            "Edition",
            GENERAL,
            Control::Combo,
            Codec::Edition,
        )
        .choices(EDITIONS, false),
        field("source", "Source", GENERAL, Control::Text, Codec::Source)
            .read_only()
            .expert(),
    ]
}

/// The parts every form shares after the kind's own fields (spec 3.2).
fn tail() -> Vec<FieldDef> {
    vec![
        field("tags", "Tags", GENERAL, Control::Tags, Codec::Tags),
        field(
            "summary",
            "Summary line",
            GENERAL,
            Control::Text,
            Codec::SetterText("short"),
        )
        .help("Shown in the second column of pickers."),
        field(
            "requirements",
            "Prerequisite",
            GENERAL,
            Control::Condition,
            Codec::Requirements,
        ),
        field(
            "prerequisiteText",
            "Prerequisite as shown",
            GENERAL,
            Control::Text,
            Codec::PrerequisiteText,
        ),
        field(
            "allowDuplicate",
            "Can be taken more than once",
            GENERAL,
            Control::Check,
            Codec::SetterFlag("allow duplicate"),
        ),
        field(
            "keywords",
            "Keywords",
            GENERAL,
            Control::Chips,
            Codec::SetterChips("keywords"),
        ),
        field("rules", "Rules", RULES, Control::Blocks, Codec::Blocks),
        field(
            "description",
            "Description",
            DESCRIPTION,
            Control::RichText,
            Codec::Description,
        ),
        field(
            "sheet",
            "On the character sheet",
            DESCRIPTION,
            Control::Sheet,
            Codec::Sheet,
        ),
        field(
            "compendium",
            "Show in Compendium",
            DESCRIPTION,
            Control::Check,
            Codec::Compendium,
        ),
        field(
            "mapsTo",
            "2014/2024 counterpart",
            DESCRIPTION,
            Control::MapsTo,
            Codec::MapsTo,
        )
        .expert(),
    ]
}

fn with_own(own: Vec<FieldDef>) -> Vec<FieldDef> {
    head().into_iter().chain(own).chain(tail()).collect()
}

fn small(
    id: &'static str,
    label: &'static str,
    plural: &'static str,
    description: &'static str,
    element_type: &'static [&'static str],
    editions: &'static [Ruleset],
    own: Vec<FieldDef>,
) -> KindDef {
    KindDef {
        id,
        label,
        plural,
        description,
        group: KindGroup::Small,
        element_types: element_type,
        parent_kinds: &[],
        editions,
        tabs: TABS,
        fields: with_own(own),
    }
}

fn data_tag(label: &'static str) -> FieldDef {
    field(
        "dataTag",
        label,
        GENERAL,
        Control::Text,
        Codec::DataText("tag"),
    )
    .expert()
}

fn small_kinds() -> Vec<KindDef> {
    vec![
        small(
            "language",
            "Language",
            "Languages",
            "A language characters can speak.",
            &["Language"],
            BOTH,
            vec![
                field(
                    "rarity",
                    "Rarity",
                    GENERAL,
                    Control::Combo,
                    Codec::LanguageRarity,
                )
                .choices(RARITY, false),
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
            ],
        ),
        small(
            "proficiency",
            "Proficiency",
            "Proficiencies",
            "A proficiency, or a group of them (its rules grant the narrower ones).",
            &["Proficiency"],
            BOTH,
            vec![],
        ),
        small(
            "condition",
            "Condition",
            "Conditions",
            "A condition, with its effects as rules.",
            &["Condition"],
            BOTH,
            vec![data_tag("Condition group")],
        ),
        small(
            "damageType",
            "Damage type",
            "Damage types",
            "A type of damage.",
            &["Damage Type"],
            BOTH,
            vec![],
        ),
        small(
            "skill",
            "Skill",
            "Skills",
            "A skill and the ability it uses.",
            &["Skill"],
            BOTH,
            vec![field(
                "ability",
                "Ability",
                GENERAL,
                Control::Combo,
                Codec::SetterText("ability"),
            )
            .choices(ABILITIES, true)],
        ),
        small(
            "deity",
            "Deity",
            "Deities",
            "A god, with alignment, domains and symbol.",
            &["Deity"],
            BOTH,
            vec![
                field(
                    "alignment",
                    "Alignment",
                    GENERAL,
                    Control::Text,
                    Codec::SetterText("alignment"),
                )
                .required(),
                field(
                    "domains",
                    "Domains",
                    GENERAL,
                    Control::Chips,
                    Codec::SetterChips("domains"),
                )
                .required(),
                field(
                    "symbol",
                    "Symbol",
                    GENERAL,
                    Control::Text,
                    Codec::SetterText("symbol"),
                )
                .required(),
            ],
        ),
        small(
            "vision",
            "Vision",
            "Visions",
            "A kind of sight, such as darkvision.",
            &["Vision"],
            BOTH,
            vec![],
        ),
        small(
            "size",
            "Size",
            "Sizes",
            "A creature size.",
            &["Size"],
            BOTH,
            vec![],
        ),
        small(
            "alignment",
            "Alignment",
            "Alignments",
            "An alignment.",
            &["Alignment"],
            BOTH,
            vec![field(
                "abbreviation",
                "Abbreviation",
                GENERAL,
                Control::Text,
                Codec::SetterText("abbreviation"),
            )],
        ),
        small(
            "weaponProperty",
            "Weapon property",
            "Weapon properties",
            "A property weapons can have.",
            &["Weapon Property"],
            BOTH,
            vec![],
        ),
        small(
            "weaponGroup",
            "Weapon group",
            "Weapon groups",
            "A group of weapons.",
            &["Weapon Group"],
            BOTH,
            vec![],
        ),
        small(
            "weaponCategory",
            "Weapon category",
            "Weapon categories",
            "A weapon category.",
            &["Weapon Category"],
            BOTH,
            vec![],
        ),
        small(
            "armorGroup",
            "Armor group",
            "Armor groups",
            "A group of armor.",
            &["Armor Group"],
            BOTH,
            vec![],
        ),
        small(
            "masteryProperty",
            "Mastery property",
            "Mastery properties",
            "A weapon mastery property.",
            &["Mastery Property"],
            ONLY_2024,
            vec![
                field(
                    "masteryKind",
                    "Kind",
                    GENERAL,
                    Control::Text,
                    Codec::DataText("kind"),
                )
                .expert(),
                field(
                    "mastery",
                    "Mastery key",
                    GENERAL,
                    Control::Text,
                    Codec::DataText("mastery"),
                )
                .expert(),
            ],
        ),
        small(
            "focusGroup",
            "Spellcasting focus group",
            "Spellcasting focus groups",
            "A kind of spellcasting focus.",
            &["Spellcasting Focus Group"],
            BOTH,
            vec![],
        ),
        small(
            "dragonmark",
            "Dragonmark",
            "Dragonmarks",
            "A mark with its rules.",
            &["Dragonmark"],
            BOTH,
            vec![],
        ),
        small(
            "rulesText",
            "Rules text",
            "Rules text",
            "A rules or glossary entry (text only).",
            &["Rules Glossary Entry"],
            BOTH,
            vec![data_tag("Glossary group")],
        ),
    ]
}

fn other_kind() -> KindDef {
    let own = vec![
        field("type", "Type", GENERAL, Control::Combo, Codec::Type)
            .required()
            .choices(&[], true),
        field(
            "setters",
            "Other properties",
            GENERAL,
            Control::Setters,
            Codec::OtherSetters,
        )
        .expert(),
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
        fields: with_own(own),
    }
}

/// Every creator form.
#[must_use]
pub fn kinds() -> &'static [KindDef] {
    static KINDS: OnceLock<Vec<KindDef>> = OnceLock::new();
    KINDS.get_or_init(|| {
        let mut all = vec![
            spell::spell_kind(),
            feat::feat_kind(),
            feat::feat_feature_kind(),
        ];
        all.extend(small_kinds());
        all.push(other_kind());
        all
    })
}

/// The append form ("add to this list", spec 10.3): not offered in the New element dialog.
#[must_use]
pub fn append_kind() -> &'static KindDef {
    static APPEND: OnceLock<KindDef> = OnceLock::new();
    APPEND.get_or_init(|| KindDef {
        id: super::APPEND_KIND,
        label: "Addition to an element",
        plural: "Additions to elements",
        description: "Adds tags, properties or rules to an element of another pack.",
        group: KindGroup::Expert,
        element_types: &[],
        parent_kinds: &[],
        editions: BOTH,
        tabs: &[(GENERAL, "GENERAL"), (RULES, "RULES")],
        fields: vec![
            field(
                "target",
                "Adds to",
                GENERAL,
                Control::Reference,
                Codec::Target,
            )
            .required(),
            field("type", "Type", GENERAL, Control::Text, Codec::Type).expert(),
            field("tags", "Tags it adds", GENERAL, Control::Tags, Codec::Tags),
            field(
                "setters",
                "Properties it adds",
                GENERAL,
                Control::Setters,
                Codec::OtherSetters,
            )
            .expert()
            .help("A property is added only when the element has none with that name."),
            field(
                "rules",
                "Rules it adds",
                RULES,
                Control::Blocks,
                Codec::Blocks,
            ),
        ],
    })
}

/// The form with this id.
#[must_use]
pub fn kind(id: &str) -> Option<&'static KindDef> {
    kinds()
        .iter()
        .find(|k| k.id == id)
        .or_else(|| (id == super::APPEND_KIND).then(append_kind))
}

/// The form an element of type `ty` opens in: the kind that writes the type, else the generic form.
#[must_use]
pub fn kind_for_type(ty: &str) -> &'static KindDef {
    kinds()
        .iter()
        .find(|k| k.element_types.contains(&ty))
        .or_else(|| kind(OTHER_KIND))
        .expect("the generic form exists")
}
