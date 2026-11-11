//! The creator's forms (spec 3.1, 3.2, 4.10, 4.12): which element types each kind writes, its sub-tabs and its
//! fields, each bound to the model locations it edits.
//!
//! The parts every form shares are here; each kind's own fields are in its module (`small` holds the small types and
//! the generic "Other element" form, WB-19). Element types with no form of their own open in the generic form, so
//! every bundled element has a form.

use super::codec::Codec;
use super::FormState;
use borealis_content::Ruleset;
use serde_json::{json, Value};
use std::sync::OnceLock;

mod caster;
mod class;
mod feat;
mod small;
mod species;
mod spell;
mod subclass;

pub use class::{new_class_form, CLASS_FEATURE_KIND, CLASS_KIND};
pub use subclass::CLASS_OPTION_KIND;

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
    /// A whole number in `FieldDef::min..=max`.
    Number,
    /// Several element pickers.
    References,
    /// The tag filter builder (spec 6.3).
    TagFilter,
    /// An item pack's contents: items with amounts (spec 4.9).
    Contents,
    /// An Equipment Package's lettered options (spec 4.9).
    PackageOptions,
    /// A spell's lists: one check box per spell list loaded classes declare, plus new list names.
    SpellLists,
    /// Whole numbers in `FieldDef::min..=max` (levels).
    Numbers,
    /// A parent's children list (sub-features): each a child form or a reference ([`super::ChildEntry`]).
    Children,
    /// A class's LEVELS grid: its children, each at a level, with the class table beside them.
    Levels,
    /// A class table's counter columns ([`super::codec::TableColumn`]).
    ClassTable,
    /// A class's skill choice ([`super::codec::SkillChoice`]).
    SkillChoice,
    /// A starting-equipment package edited inside its owner (a child, or a reference).
    Package,
    /// A count at each of the 20 levels (cantrips known, prepared spells), maybe with expert flags
    /// ([`super::LevelCounts`]).
    LevelCounts,
    /// A spellcasting section's slot table: 20 levels x 9 slot levels.
    SlotTable,
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
            Self::Number => "number",
            Self::References => "references",
            Self::TagFilter => "tagFilter",
            Self::Contents => "contents",
            Self::PackageOptions => "packageOptions",
            Self::SpellLists => "spellLists",
            Self::Numbers => "numbers",
            Self::Children => "children",
            Self::Levels => "levels",
            Self::ClassTable => "classTable",
            Self::SkillChoice => "skillChoice",
            Self::Package => "package",
            Self::LevelCounts => "levelCounts",
            Self::SlotTable => "slotTable",
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
    /// The SectionSecondary heading it is grouped under.
    pub section: Option<&'static str>,
    /// Number range (`Control::Number`).
    pub min: Option<i32>,
    /// Number range (`Control::Number`).
    pub max: Option<i32>,
    /// Element types a reference field offers.
    pub picker: &'static [&'static str],
    /// The value a new element starts with, as JSON text.
    pub default: Option<&'static str>,
    /// The editions it is shown for (empty = both).
    pub editions: &'static [Ruleset],
}

pub(crate) const fn field(
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
        min: None,
        max: None,
        picker: &[],
        default: None,
        editions: &[],
    }
}

impl FieldDef {
    pub(crate) const fn required(self) -> Self {
        Self {
            required: true,
            ..self
        }
    }
    pub(crate) const fn choices(
        self,
        choices: &'static [(&'static str, &'static str)],
        custom: bool,
    ) -> Self {
        Self {
            choices,
            allow_custom: custom,
            ..self
        }
    }
    pub(crate) const fn expert(self) -> Self {
        Self {
            expert: true,
            ..self
        }
    }
    pub(crate) const fn read_only(self) -> Self {
        Self {
            read_only: true,
            ..self
        }
    }
    pub(crate) const fn help(self, help: &'static str) -> Self {
        Self {
            help: Some(help),
            ..self
        }
    }
    pub(crate) const fn section(self, section: &'static str) -> Self {
        Self {
            section: Some(section),
            ..self
        }
    }
    pub(crate) const fn range(self, min: i32, max: i32) -> Self {
        Self {
            min: Some(min),
            max: Some(max),
            ..self
        }
    }
    pub(crate) const fn picker(self, types: &'static [&'static str]) -> Self {
        Self {
            picker: types,
            ..self
        }
    }
    pub(crate) const fn editions(self, editions: &'static [Ruleset]) -> Self {
        Self { editions, ..self }
    }
    /// `default` is JSON text (`"\"1\""`, `"true"`).
    pub(crate) const fn default(self, default: &'static str) -> Self {
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

pub(crate) const GENERAL: &str = "general";
pub(crate) const RULES: &str = "rules";
pub(crate) const DESCRIPTION: &str = "description";
pub(crate) const TABS: &[(&str, &str)] = &[
    (GENERAL, "GENERAL"),
    (RULES, "RULES"),
    (DESCRIPTION, "DESCRIPTION"),
];
/// The sub-tabs of a form with a spellcasting section (spec 4.6).
pub(crate) const CASTER_TABS: &[(&str, &str)] = &[
    (GENERAL, "GENERAL"),
    (caster::SPELLCASTING, "SPELLCASTING"),
    (RULES, "RULES"),
    (DESCRIPTION, "DESCRIPTION"),
];
pub(crate) const BOTH: &[Ruleset] = &[Ruleset::Dnd2014, Ruleset::Dnd2024];
pub(crate) const ONLY_2024: &[Ruleset] = &[Ruleset::Dnd2024];

const EDITIONS: &[(&str, &str)] = &[
    ("", "The pack's edition"),
    ("dnd2014", "2014"),
    ("dnd2024", "2024"),
    ("any", "Either edition"),
];
/// GENERAL fields before the kind's own.
pub(crate) fn head() -> Vec<FieldDef> {
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
pub(crate) fn tail() -> Vec<FieldDef> {
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

/// Every creator form.
#[must_use]
pub fn kinds() -> &'static [KindDef] {
    static KINDS: OnceLock<Vec<KindDef>> = OnceLock::new();
    KINDS.get_or_init(|| {
        let mut all = vec![
            spell::spell_kind(),
            feat::feat_kind(),
            feat::feat_feature_kind(),
            class::class_kind(),
            class::class_feature_kind(),
            subclass::subclass_kind(),
            subclass::subclass_feature_kind(),
            subclass::class_option_kind(),
        ];
        all.extend(species::species_kinds());
        all.extend(small::small_kinds());
        all.extend(super::item_kinds::item_kinds());
        all.push(small::other_kind());
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

/// The form `e` opens in: [`kind_for_type`], except that a house-rule setting (an `Option` with a `default` setter:
/// the core's "Multiclassing", "Feats") is no class option (spec 4.8) and opens in the generic form.
#[must_use]
pub fn kind_for_element(e: &borealis_content::Element) -> &'static KindDef {
    if e.ty == "Option" && e.setter_value("default").is_some() {
        return kind(OTHER_KIND).expect("the generic form exists");
    }
    kind_for_type(&e.ty)
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
