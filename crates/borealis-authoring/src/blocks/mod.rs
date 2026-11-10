//! The rule block catalog of the homebrew creator (`specs/homebrew-creator-spec.md` sections 6.1, 6.2).
//!
//! A rule of an element is shown as a **block**: one plain sentence ([`sentence`]) and a dialog whose fields come
//! from the block's [`BlockSpec`]. The catalog has the four core blocks (`grant`, `select`, `list` = a `select` of type
//! `List`, `stat`), one block per custom rule kind the engine reads
//! (`borealis_engine::custom_rules::KNOWN_KINDS`, 69 kinds) and the toggle effect `speed_multiply`, which exists only
//! inside a toggle's effects.
//!
//! Fields are exactly the fields the engine reads (`KindSpec::fields`) apart from the gate fields `level` and
//! `requirements`, which every dialog shows as "From level" ([`BlockSpec::level_gated`]) and "Only if"
//! ([`BlockSpec::only_if`]). Fields the engine keeps without reading (`KindSpec::unmodelled`) are never offered; a
//! copy keeps them (spec 3.4). The tests `tests/block_catalog.rs` diff the catalog against the engine registry, so a
//! new engine kind can't be added without its block (spec 6.6).

mod catalog_a;
mod catalog_b;
mod choices;
mod fit;
mod sentence;
mod words;

pub use choices::Choice;
pub use fit::{fits, misfits, value_fits};
pub use sentence::{sentence, NoNames, SentenceContext};

use crate::expr_build::formula::FormulaDialect;
use borealis_content::rules::Rule;
use serde::Serialize;

/// The Add menu group of a block (spec 6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BlockGroup {
    /// Gives, Lets the player choose, written lists, Changes a number (spec 6.1).
    Basic,
    /// Abilities & feats.
    Abilities,
    /// Spells.
    Spells,
    /// Uses & resources.
    Uses,
    /// Attacks & damage.
    Attacks,
    /// Defence & saves.
    Defence,
    /// Checks & movement.
    Checks,
    /// Effects only a toggle's effect list holds.
    Effects,
    /// Core rules only (hidden unless "Show core-rule blocks").
    CoreRules,
}

impl BlockGroup {
    /// Every group, in Add menu order.
    pub const ALL: [Self; 9] = [
        Self::Basic,
        Self::Abilities,
        Self::Spells,
        Self::Uses,
        Self::Attacks,
        Self::Defence,
        Self::Checks,
        Self::Effects,
        Self::CoreRules,
    ];

    /// The stable id (`abilities`).
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::Abilities => "abilities",
            Self::Spells => "spells",
            Self::Uses => "uses",
            Self::Attacks => "attacks",
            Self::Defence => "defence",
            Self::Checks => "checks",
            Self::Effects => "effects",
            Self::CoreRules => "coreRules",
        }
    }

    /// The Add menu heading.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Basic => "Basic",
            Self::Abilities => "Abilities & feats",
            Self::Spells => "Spells",
            Self::Uses => "Uses & resources",
            Self::Attacks => "Attacks & damage",
            Self::Defence => "Defence & saves",
            Self::Checks => "Checks & movement",
            Self::Effects => "Toggle effects",
            Self::CoreRules => "Core rules only",
        }
    }
}

/// What a name field names (a picker over names in scope, never typed by the user when something is in scope).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    /// A `resource` id (pools of this element and the elements it belongs to).
    Resource,
    /// A `toggle` id.
    Toggle,
    /// An `aura` id.
    Aura,
    /// The name of a `select` rule.
    Select,
    /// The name of a `select_param` (a named choice).
    Choice,
    /// The name of an `extra_damage` rule.
    ExtraDamage,
    /// The name of a feature (any element name).
    Feature,
    /// A class name.
    Class,
    /// A spell list tag (a class's `spellcasting.list`).
    SpellList,
    /// A spell name.
    Spell,
}

/// The control a block field is edited with (wire name in [`Control::wire`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Control {
    /// Written by the creator, never shown (a grant's element type, the numbers of a written list's rows).
    Auto,
    /// One line of text.
    Text,
    /// Several lines of plain text.
    Multiline,
    /// A whole number in a range.
    Number {
        /// Lowest value.
        min: i32,
        /// Highest value.
        max: i32,
    },
    /// A whole number, or one of `words` (`"any"`), or (with `custom`) any text.
    NumberOr {
        /// Lowest value.
        min: i32,
        /// Highest value.
        max: i32,
        /// The words also accepted.
        words: &'static [Choice],
        /// Any text is accepted too.
        custom: bool,
    },
    /// A list of whole numbers (spell levels).
    Numbers {
        /// Lowest value.
        min: i32,
        /// Highest value.
        max: i32,
    },
    /// A check box.
    Check,
    /// One value from `choices` (or, with `custom`, any text).
    Combo {
        /// The values offered.
        choices: &'static [Choice],
        /// Any text is accepted too.
        custom: bool,
    },
    /// A list of values from `choices` (or, with `custom`, any text).
    Chips {
        /// The values offered.
        choices: &'static [Choice],
        /// Any text is accepted too.
        custom: bool,
    },
    /// One element id, picked (spec 5.1); `self` = the element itself where the help says so.
    Reference {
        /// Element types offered; empty = any type.
        types: &'static [&'static str],
    },
    /// Several element ids, picked.
    References {
        /// Element types offered.
        types: &'static [&'static str],
    },
    /// A name of something in scope ([`Scope`]).
    Name {
        /// What it names.
        scope: Scope,
        /// Several names.
        multiple: bool,
    },
    /// A statistic name (spec 5.2, [`crate::stats`]).
    Statistic,
    /// A number or a statistic, optionally subtracted (`stat` `value`, `maximum`): text `"2"`, `"wisdom:modifier"`,
    /// `"-dexterity:modifier"`.
    StatValue,
    /// A number or `{"type": "stat", "stat": <statistic>}`.
    NumberOrStatistic,
    /// The condition builder (`requirements` language, `expr_build::condition`).
    Condition,
    /// The tag filter builder (`supports` language).
    TagFilter,
    /// The "while wearing/holding" builder (`equipped` language).
    Equipped,
    /// The formula builder in the dialect of the field's engine reader (`expr_build::formula::dialect_of`).
    Formula(FormulaDialect),
    /// A list of formulas in one dialect (each one term).
    Formulas(FormulaDialect),
    /// Dice text (`1d10`, `2d6+1`).
    Dice,
    /// Uses of a free cast: a number, `"proficiency"`, an ability name (its modifier, at least 1) or
    /// `{"type": "table", "key": <class counter>}`.
    Uses,
    /// An ability, or `{"param": <named choice>}` (the ability chosen there).
    AbilitySource,
    /// Which abilities an increase may go to: a number (any N different abilities) or a list of abilities.
    AbilityChoice,
    /// Point splits (`[[2, 1], [1, 1, 1]]`: +2/+1 or +1/+1/+1).
    Splits,
    /// A column of the owning class's level table (`data.progression` key: `rages`, `sneak_attack`).
    ClassCounter,
    /// An object with these parts.
    Group(&'static [BlockField]),
    /// A list of objects with these parts.
    Rows(&'static [BlockField]),
    /// An object whose `type` picks one of these shapes.
    Variant(&'static [Variant]),
    /// An object from keys to values (`{"6": 10}`); the two fields describe a key and a value.
    Map(&'static [BlockField; 2]),
    /// A nested block list holding these block kinds (toggle effects).
    Blocks(&'static [&'static str]),
}

impl Control {
    /// The wire name (`HomebrewControl` in `ui/src/api/types/homebrew.ts`).
    #[must_use]
    pub fn wire(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Text => "text",
            Self::Multiline => "multiline",
            Self::Number { .. } | Self::NumberOr { .. } => "number",
            Self::Numbers { .. } => "numbers",
            Self::Check => "check",
            Self::Combo { .. } => "combo",
            Self::Chips { .. } => "chips",
            Self::Reference { .. } => "reference",
            Self::References { .. } => "references",
            Self::Name { .. } => "name",
            Self::Statistic => "statistic",
            Self::StatValue => "statValue",
            Self::NumberOrStatistic => "numberOrStatistic",
            Self::Condition => "condition",
            Self::TagFilter => "tagFilter",
            Self::Equipped => "equipped",
            Self::Formula(_) => "formula",
            Self::Formulas(_) => "formulas",
            Self::Dice => "dice",
            Self::Uses => "uses",
            Self::AbilitySource => "abilitySource",
            Self::AbilityChoice => "abilityChoice",
            Self::Splits => "splits",
            Self::ClassCounter => "classCounter",
            Self::Group(_) => "group",
            Self::Rows(_) => "rows",
            Self::Variant(_) => "variant",
            Self::Map(_) => "map",
            Self::Blocks(_) => "blocks",
        }
    }

    /// The parts of a [`Control::Group`], [`Control::Rows`] or [`Control::Map`] (key, value).
    #[must_use]
    pub fn parts(&self) -> &'static [BlockField] {
        match self {
            Self::Group(f) | Self::Rows(f) => f,
            Self::Map(kv) => kv.as_slice(),
            _ => &[],
        }
    }
}

/// One shape of a [`Control::Variant`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Variant {
    /// The `type` value (`fixed`).
    pub value: &'static str,
    /// Plain words ("A fixed number").
    pub label: &'static str,
    /// The other parts of the object.
    pub fields: &'static [BlockField],
}

/// One field of a block dialog.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockField {
    /// The rule field (`max`), or the part's key inside its parent.
    pub key: &'static str,
    /// FieldHeader label, sentence case.
    pub label: &'static str,
    /// The control.
    pub control: Control,
    /// The block can't be saved without it.
    pub required: bool,
    /// Shown only with the Expert toggle (spec 6.4).
    pub expert: bool,
    /// Free text the engine shows verbatim: labelled "Shown as a note; Borealis does not apply it" (spec 6.2, R12).
    pub note: bool,
    /// Plain-words help under the field.
    pub help: Option<&'static str>,
}

impl BlockField {
    /// A field with no flags.
    #[must_use]
    pub const fn new(key: &'static str, label: &'static str, control: Control) -> Self {
        Self {
            key,
            label,
            control,
            required: false,
            expert: false,
            note: false,
            help: None,
        }
    }

    /// Required.
    #[must_use]
    pub const fn req(self) -> Self {
        Self {
            required: true,
            ..self
        }
    }

    /// Expert only.
    #[must_use]
    pub const fn expert(self) -> Self {
        Self {
            expert: true,
            ..self
        }
    }

    /// Shown verbatim as a note.
    #[must_use]
    pub const fn note(self) -> Self {
        Self { note: true, ..self }
    }

    /// With help text.
    #[must_use]
    pub const fn help(self, help: &'static str) -> Self {
        Self {
            help: Some(help),
            ..self
        }
    }
}

/// The note label of [`BlockField::note`] fields.
pub const NOTE_HELP: &str = "Shown as a note; Borealis does not apply it.";

/// One block of the catalog.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockSpec {
    /// `grant`, `select`, `list`, `stat`, a custom rule kind, or the toggle effect `speed_multiply`.
    pub kind: &'static str,
    /// Plain words ("Gives a spell").
    pub label: &'static str,
    /// Add menu group.
    pub group: BlockGroup,
    /// Listed first in the Add menu.
    pub common: bool,
    /// The dialog shows "From level" (`level`).
    pub level_gated: bool,
    /// The dialog shows "Only if" (`requirements`, the condition builder).
    pub only_if: bool,
    /// The dialog fields.
    pub fields: &'static [BlockField],
}

impl BlockSpec {
    /// Hidden unless "Show core-rule blocks" (expert), shown on copies of core content.
    #[must_use]
    pub fn core_only(&self) -> bool {
        self.group == BlockGroup::CoreRules
    }

    /// Exists only inside a toggle's effects.
    #[must_use]
    pub fn effect_only(&self) -> bool {
        self.group == BlockGroup::Effects
    }

    /// The field with this key.
    #[must_use]
    pub fn field(&self, key: &str) -> Option<&'static BlockField> {
        self.fields.iter().find(|f| f.key == key)
    }
}

/// The core block kinds (spec 6.1), in Add menu order. `list` is a `select` of type `List`.
pub const CORE_KINDS: [&str; 4] = ["grant", "select", "list", "stat"];

/// The block kinds a toggle's `effects` hold: what the engine's toggle readers read (`play_rules::toggle_stat_effects`
/// `stat`, `combat` `grant` and `roll_advantage`, `speed_toggles` `speed_multiply`).
pub const TOGGLE_EFFECT_KINDS: [&str; 4] = ["grant", "stat", "roll_advantage", "speed_multiply"];

/// Every block of the catalog: the core blocks, then the custom kinds sorted by kind, then the effect-only blocks.
#[must_use]
pub fn catalog() -> &'static [BlockSpec] {
    static ALL: std::sync::OnceLock<Vec<BlockSpec>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut custom: Vec<BlockSpec> = catalog_a::CUSTOM
            .iter()
            .chain(catalog_b::CUSTOM)
            .copied()
            .collect();
        custom.sort_by_key(|b| b.kind);
        catalog_a::CORE
            .iter()
            .copied()
            .chain(custom)
            .chain(catalog_b::EFFECTS.iter().copied())
            .collect()
    })
}

/// The block of a kind.
#[must_use]
pub fn block(kind: &str) -> Option<&'static BlockSpec> {
    catalog().iter().find(|b| b.kind == kind)
}

/// The block kind of a rule: `grant`, `select`, `list`, `stat` or the custom kind; `None` for a `Rule::Unknown` (kept
/// as is, spec 3.4) and for a custom kind the catalog doesn't know.
#[must_use]
pub fn kind_of(rule: &Rule) -> Option<&'static str> {
    let kind = match rule {
        Rule::Grant(_) => "grant",
        Rule::Select(s) if s.ty == "List" => "list",
        Rule::Select(_) => "select",
        Rule::Stat(_) => "stat",
        Rule::Unknown(_) => return None,
        Rule::Custom(c) => c.kind.as_str(),
    };
    block(kind).map(|b| b.kind)
}

#[cfg(test)]
mod tests;
