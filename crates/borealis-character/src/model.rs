//! The native character model.

use borealis_content::Ruleset;
use borealis_engine::CharacterState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Format identifier written in every character file.
pub const FORMAT: &str = "borealis-character";
/// Current version of the format.
pub const FORMAT_VERSION: u32 = 2;

/// List-card information.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    /// Library id (empty until the library assigns one).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// Id of the character this one was converted from (ruleset switch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub converted_from: Option<String>,
    /// Character name.
    #[serde(default)]
    pub name: String,
    /// Player name.
    #[serde(default)]
    pub player: String,
    /// Gender (free text).
    #[serde(default)]
    pub gender: String,
    /// Character list group.
    #[serde(default)]
    pub group: String,
    /// Pinned to the top of its group.
    #[serde(default)]
    pub favorite: bool,
}

/// Physical appearance.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Appearance {
    /// Age.
    #[serde(default)]
    pub age: String,
    /// Height.
    #[serde(default)]
    pub height: String,
    /// Weight.
    #[serde(default)]
    pub weight: String,
    /// Eyes.
    #[serde(default)]
    pub eyes: String,
    /// Skin.
    #[serde(default)]
    pub skin: String,
    /// Hair.
    #[serde(default)]
    pub hair: String,
}

/// Personality text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Personality {
    /// Trinket.
    #[serde(default)]
    pub trinket: String,
    /// Personality traits.
    #[serde(default)]
    pub traits: String,
    /// Ideals.
    #[serde(default)]
    pub ideals: String,
    /// Bonds.
    #[serde(default)]
    pub bonds: String,
    /// Flaws.
    #[serde(default)]
    pub flaws: String,
}

/// Organization block.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Organization {
    /// Name.
    #[serde(default)]
    pub name: String,
    /// Symbol image path or reference.
    #[serde(default)]
    pub symbol: String,
    /// Allies text.
    #[serde(default)]
    pub allies: String,
}

/// Background feature override.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureText {
    /// Name.
    pub name: String,
    /// Description.
    pub description: String,
}

/// A hand-made or customized line of the attacks table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualAttack {
    /// Linked inventory item uid for automatic attacks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_uid: Option<String>,
    /// Name.
    pub name: String,
    /// Range text.
    #[serde(default)]
    pub range: String,
    /// Attack bonus text.
    #[serde(default)]
    pub attack: String,
    /// Damage text.
    #[serde(default)]
    pub damage: String,
    /// Shown on the sheet.
    #[serde(default)]
    pub displayed: bool,
    /// Linked ability name.
    #[serde(default)]
    pub ability: String,
    /// Notes.
    #[serde(default)]
    pub description: String,
    /// Print this row as an attack card. Absent = the default: a derived row follows its
    /// weapon line's "show card" flag, a hand-typed row is off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_card: Option<bool>,
}

/// Two free note columns.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notes {
    /// Left column.
    #[serde(default)]
    pub left: String,
    /// Right column.
    #[serde(default)]
    pub right: String,
}

/// Free-form character details (none of it feeds the engine).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Details {
    /// Appearance.
    #[serde(default)]
    pub appearance: Appearance,
    /// Personality.
    #[serde(default)]
    pub personality: Personality,
    /// Backstory.
    #[serde(default)]
    pub backstory: String,
    /// Background feature override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_feature: Option<FeatureText>,
    /// Organization.
    #[serde(default)]
    pub organization: Organization,
    /// Additional features and traits.
    #[serde(default)]
    pub additional_features: String,
    /// Equipment notes.
    #[serde(default)]
    pub equipment_notes: String,
    /// Treasure notes.
    #[serde(default)]
    pub treasure: String,
    /// Quest items.
    #[serde(default)]
    pub quest: String,
    /// Notes columns.
    #[serde(default)]
    pub notes: Notes,
    /// Free text above the attacks table.
    #[serde(default)]
    pub attacks_text: String,
    /// Attack table overrides and manual lines.
    #[serde(default)]
    pub attacks: Vec<ManualAttack>,
    /// "As card" overrides for the automatic (weapon-derived) attack rows, by inventory item uid.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub auto_attack_cards: std::collections::BTreeMap<String, bool>,
    /// Conditional armor class note.
    #[serde(default)]
    pub conditional_ac: String,
    /// Companion name.
    #[serde(default)]
    pub companion_name: String,
}

/// An embedded portrait.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Portrait {
    /// File name (no directory).
    pub file_name: String,
    /// Base64 image bytes.
    pub data_base64: String,
}

/// A stored selection whose slot could not be mapped, kept so it can be repaired later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnresolvedSelection {
    /// Element declaring the rule in the source file.
    pub owner: String,
    /// Rule type.
    pub ty: String,
    /// Rule name.
    pub name: String,
    /// 1-based slot number.
    pub number: u32,
    /// Chosen element id or list entry.
    pub chosen: String,
    /// Legacy checksum.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    /// Level the slot is required at (used to pick between same-named rules).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_level: Option<u32>,
    /// The slot holds a list entry index rather than an element id.
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_list: bool,
    /// Level of the replaced selection, when the slot replaces one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace_level: Option<u32>,
    /// Inventory item instance the slot belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(b: &bool) -> bool {
    !*b
}

/// Where a character came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportInfo {
    /// Importer name.
    pub source: String,
    /// Version of the producing application.
    pub source_version: String,
    /// Number of unresolved content references at import time.
    pub missing_content: u32,
    /// Number of legacy divergence notes at import time.
    pub legacy_divergences: u32,
    /// Content references the installed content did not define, kept so they can be re-checked
    /// after more content is installed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<crate::import::MissingContent>,
}

/// A complete character.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Character {
    /// List-card information.
    pub meta: Meta,
    /// Free-form details.
    pub details: Details,
    /// Engine inputs.
    pub build: CharacterState,
    /// Embedded portrait.
    pub portrait: Option<Portrait>,
    /// Import provenance.
    pub import: Option<ImportInfo>,
    /// Selections that could not be mapped.
    pub unresolved: Vec<UnresolvedSelection>,
    /// Unknown top-level keys, preserved for forward compatibility.
    pub extra: BTreeMap<String, Value>,
}

impl Character {
    /// A new level-1 2014 character.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self::with_ruleset(name, Ruleset::Dnd2014)
    }

    /// A new level-1 character on a ruleset.
    #[must_use]
    pub fn with_ruleset(name: impl Into<String>, ruleset: Ruleset) -> Self {
        Self {
            meta: Meta {
                name: name.into(),
                group: "Characters".into(),
                ..Meta::default()
            },
            build: CharacterState {
                ruleset,
                ..CharacterState::default()
            },
            ..Self::default()
        }
    }

    /// The ruleset profile.
    #[must_use]
    pub fn ruleset(&self) -> Ruleset {
        self.build.ruleset
    }
}

#[derive(Serialize, Deserialize)]
struct Wire {
    format: String,
    format_version: u32,
    ruleset: Ruleset,
    #[serde(default)]
    meta: Meta,
    #[serde(default)]
    details: Details,
    build: CharacterState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    portrait: Option<Portrait>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    import: Option<ImportInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    unresolved: Vec<UnresolvedSelection>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

impl Serialize for Character {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        Wire {
            format: FORMAT.to_string(),
            format_version: FORMAT_VERSION,
            ruleset: self.build.ruleset,
            meta: self.meta.clone(),
            details: self.details.clone(),
            build: self.build.clone(),
            portrait: self.portrait.clone(),
            import: self.import.clone(),
            unresolved: self.unresolved.clone(),
            extra: self.extra.clone(),
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for Character {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = Wire::deserialize(d)?;
        if w.format != FORMAT {
            return Err(serde::de::Error::custom(format!("not a {FORMAT} file")));
        }
        let mut build = w.build;
        build.ruleset = w.ruleset;
        Ok(Self {
            meta: w.meta,
            details: w.details,
            build,
            portrait: w.portrait,
            import: w.import,
            unresolved: w.unresolved,
            extra: w.extra,
        })
    }
}
