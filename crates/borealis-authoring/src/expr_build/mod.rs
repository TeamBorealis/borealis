//! Expression builders (spec 6.3, 6.5): the condition, tag filter, "while wearing" and formula builders, each a
//! model the creator edits with chips, and the exact text the engine reads.
//!
//! * [`condition`]: element and rule `requirements` (`borealis_content::expr::requirements`).
//! * [`tag_filter`]: select `supports`, spell lists, magic item `weapon`/`armor` (`expr::supports`).
//! * [`equipped`]: stat `equipped` (`expr::equipped`).
//! * [`formula`]: the formula fields of custom rules, in the dialect of the engine reader of each field
//!   ([`formula::FormulaDialect`]).
//!
//! Every builder has the same four operations ([`ExprLanguage`]): `parse` text into the model (or keep it as is,
//! [`Built::Kept`], spec 6.5), `render` the model to canonical text, `means_same` (does a text mean exactly what a
//! model means, read the way the engine reads it), and `check` (plain-sentence problems for values the text cannot
//! hold). An [`ExprValue`] carries a model with the text it was read from, so an unedited expression is written back
//! byte for byte and an edited one in canonical spelling.

pub mod condition;
pub mod equipped;
pub mod formula;
mod formula_text;
pub mod tag_filter;
mod tree;

pub use tree::{GroupMode, Node};

use serde::{Deserialize, Serialize};

/// A plain-language problem with a builder value, pointing at the part of the model it is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderProblem {
    /// Path inside the model (`items.1.min`), empty for the whole value.
    pub path: String,
    /// One plain sentence ("Give the feature a name.").
    pub message: String,
}

impl BuilderProblem {
    pub(crate) fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

/// The builder state of one expression: a model the chips edit, or text kept as is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "camelCase")]
pub enum Built<M> {
    /// The expression as a builder model.
    Model {
        /// The model.
        model: M,
    },
    /// Text the builder cannot represent (spec 6.5): shown as a read-only "kept as is" chip with Remove, written back
    /// byte for byte.
    Kept {
        /// The text, unchanged.
        text: String,
        /// Why it is kept, in plain words.
        reason: String,
    },
}

/// One expression field of a form: the builder state and the text it was read from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExprValue<M> {
    /// The builder state.
    pub value: Built<M>,
    /// The stored text this value was read from; absent for a new value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original: Option<String>,
}

/// One expression language and its builder.
pub trait ExprLanguage {
    /// The builder model.
    type Model: Clone + PartialEq;
    /// What reading needs besides the text (loaded class names, the formula dialect).
    type Context;

    /// Read `text` into the builder state. Blank text is the empty model.
    fn parse(text: &str, cx: &Self::Context) -> Built<Self::Model>;
    /// The canonical text of `model`. Values [`ExprLanguage::check`] reports problems for may not be readable.
    fn render(model: &Self::Model, cx: &Self::Context) -> String;
    /// True when the engine reads `text` exactly as it reads `model`.
    fn means_same(model: &Self::Model, text: &str, cx: &Self::Context) -> bool;
    /// Problems with values the text cannot hold (empty names, characters the language reserves...).
    fn check(model: &Self::Model, cx: &Self::Context) -> Vec<BuilderProblem>;
}

impl<M: Clone + PartialEq> ExprValue<M> {
    /// The builder state of stored `text`.
    #[must_use]
    pub fn read<L: ExprLanguage<Model = M>>(text: &str, cx: &L::Context) -> Self {
        Self {
            value: L::parse(text, cx),
            original: Some(text.to_string()),
        }
    }

    /// A new value from a model.
    #[must_use]
    pub fn new(model: M) -> Self {
        Self {
            value: Built::Model { model },
            original: None,
        }
    }

    /// The text to store: kept text unchanged; the original spelling while the model still means what it says;
    /// otherwise the canonical text.
    #[must_use]
    pub fn text<L: ExprLanguage<Model = M>>(&self, cx: &L::Context) -> String {
        match &self.value {
            Built::Kept { text, .. } => text.clone(),
            Built::Model { model } => match &self.original {
                Some(original) if L::means_same(model, original, cx) => original.clone(),
                _ => L::render(model, cx),
            },
        }
    }

    /// The problems of the model (none for kept text).
    #[must_use]
    pub fn check<L: ExprLanguage<Model = M>>(&self, cx: &L::Context) -> Vec<BuilderProblem> {
        match &self.value {
            Built::Kept { .. } => Vec::new(),
            Built::Model { model } => L::check(model, cx),
        }
    }
}

/// The six abilities, written in full in canonical text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Ability {
    /// Strength.
    Strength,
    /// Dexterity.
    Dexterity,
    /// Constitution.
    Constitution,
    /// Intelligence.
    Intelligence,
    /// Wisdom.
    Wisdom,
    /// Charisma.
    Charisma,
}

impl Ability {
    /// All six, in sheet order.
    pub const ALL: [Self; 6] = [
        Self::Strength,
        Self::Dexterity,
        Self::Constitution,
        Self::Intelligence,
        Self::Wisdom,
        Self::Charisma,
    ];

    /// `strength`.
    #[must_use]
    pub fn full(self) -> &'static str {
        [
            "strength",
            "dexterity",
            "constitution",
            "intelligence",
            "wisdom",
            "charisma",
        ][self.index()]
    }

    /// `str`.
    #[must_use]
    pub fn short(self) -> &'static str {
        ["str", "dex", "con", "int", "wis", "cha"][self.index()]
    }

    /// Sheet index (0 = Strength), the engine's ability order.
    #[must_use]
    pub fn index(self) -> usize {
        self as usize
    }

    /// The ability a key names, in full or short, any case (`Wisdom`, `wis`); the engine's `ability_index`.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        let k = key.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|a| a.full() == k || a.short() == k)
    }

    /// The ability a full name names (`strength`, any case); the readers that accept full names only.
    #[must_use]
    pub fn from_full_name(name: &str) -> Option<Self> {
        let k = name.trim().to_lowercase();
        Self::ALL.into_iter().find(|a| a.full() == k)
    }
}

#[cfg(test)]
mod tests;
